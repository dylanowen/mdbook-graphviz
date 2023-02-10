use core::mem;
use std::marker::PhantomData;
use std::path::PathBuf;

use async_recursion::async_recursion;
use futures::future;
use mdbook::book::{Book, Chapter};
use mdbook::errors::Result;
use mdbook::preprocess::{Preprocessor, PreprocessorContext};
use mdbook::utils::new_cmark_parser;
use mdbook::BookItem;
use pulldown_cmark::{CodeBlockKind, Event, Tag};
use pulldown_cmark_to_cmark::cmark;

use crate::renderer::{CLIGraphviz, CLIGraphvizToFile, GraphvizRenderer};

pub static PREPROCESSOR_NAME: &str = "graphviz";
pub static INFO_STRING_PREFIX: &str = "dot process";

pub struct GraphvizPreprocessor;

pub struct Graphviz<R: GraphvizRenderer> {
    src_dir: PathBuf,
    _phantom: PhantomData<*const R>,
}

impl Preprocessor for GraphvizPreprocessor {
    fn name(&self) -> &str {
        PREPROCESSOR_NAME
    }

    fn run(&self, ctx: &PreprocessorContext, mut book: Book) -> Result<Book> {
        let output_to_file = ctx
            .config
            .get_preprocessor(self.name())
            .and_then(|t| t.get("output-to-file"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let src_dir = ctx.root.clone().join(&ctx.config.book.src);

        // we really only need 1 thread since we're just calling out to the Graphviz CLI
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .unwrap()
            .block_on(async {
                if !output_to_file {
                    Graphviz::<CLIGraphviz>::new(src_dir)
                        .process_sub_items(&mut book.sections)
                        .await
                } else {
                    Graphviz::<CLIGraphvizToFile>::new(src_dir)
                        .process_sub_items(&mut book.sections)
                        .await
                }
            })?;

        Ok(book)
    }

    fn supports_renderer(&self, _renderer: &str) -> bool {
        // since we're just outputting markdown images or inline html, this "should" support any renderer
        true
    }
}

impl<R: GraphvizRenderer> Graphviz<R> {
    pub fn new(src_dir: PathBuf) -> Graphviz<R> {
        Self {
            src_dir,
            _phantom: PhantomData,
        }
    }

    #[async_recursion(?Send)]
    async fn process_sub_items(&'async_recursion self, items: &mut Vec<BookItem>) -> Result<()> {
        let mut item_futures = Vec::with_capacity(items.len());
        for item in mem::take(items) {
            item_futures.push(async {
                match item {
                    BookItem::Chapter(chapter) => {
                        self.process_chapter(chapter).await.map(BookItem::Chapter)
                    }
                    item => {
                        // pass through all non-chapters
                        Ok(item)
                    }
                }
            });
        }

        *items = future::join_all(item_futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        Ok(())
    }

    #[async_recursion(?Send)]
    async fn process_chapter(&self, mut chapter: Chapter) -> Result<Chapter> {
        // make sure to process our chapter sub-items
        self.process_sub_items(&mut chapter.sub_items).await?;

        if chapter.path.is_none() {
            return Ok(chapter);
        }

        // assume we've already filtered out all the draft chapters
        let mut chapter_path = self.src_dir.join(chapter.path.as_ref().unwrap());
        // remove the chapter filename
        chapter_path.pop();

        let mut buf = String::with_capacity(chapter.content.len());
        let mut graphviz_block_builder: Option<GraphvizBlockBuilder> = None;
        let mut image_index = 0;

        let events = new_cmark_parser(&chapter.content, false);
        let mut event_futures = Vec::new();

        for e in events {
            if let Some(mut builder) = graphviz_block_builder.take() {
                match e {
                    Event::Text(ref text) => {
                        builder.append_code(&**text);
                        graphviz_block_builder = Some(builder);
                    }
                    Event::End(Tag::CodeBlock(CodeBlockKind::Fenced(ref info_string))) => {
                        assert_eq!(
                            Some(0),
                            (**info_string).find(INFO_STRING_PREFIX),
                            "We must close our graphviz block"
                        );

                        // finish our digraph
                        let block = builder.build(image_index);
                        image_index += 1;

                        event_futures.push(R::render_graphviz(block));
                    }
                    _ => {
                        graphviz_block_builder = Some(builder);
                    }
                }
            } else {
                match e {
                    Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(ref info_string)))
                        if (**info_string).find(INFO_STRING_PREFIX) == Some(0) =>
                    {
                        graphviz_block_builder = Some(GraphvizBlockBuilder::new(
                            &**info_string,
                            &chapter.name.clone(),
                            chapter_path.clone(),
                        ));
                    }
                    _ => {
                        // pass through all events that don't impact our Graphviz block
                        event_futures.push(Box::pin(async { Ok(vec![e]) }));
                    }
                }
            }
        }

        let events = future::join_all(event_futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten();

        cmark(events, &mut buf)?;

        chapter.content = buf;

        Ok(chapter)
    }
}

struct GraphvizBlockBuilder {
    chapter_name: String,
    graph_name: String,
    code: String,
    path: PathBuf,
}

impl GraphvizBlockBuilder {
    fn new<S: Into<String>>(
        info_string: S,
        chapter_name: S,
        path: PathBuf,
    ) -> GraphvizBlockBuilder {
        let info_string: String = info_string.into();

        let chapter_name = chapter_name.into();

        // check if we can have a name at the end of our info string
        let graph_name = if Some(' ') == info_string.chars().nth(INFO_STRING_PREFIX.len()) {
            info_string[INFO_STRING_PREFIX.len() + 1..].trim()
        } else {
            ""
        };

        GraphvizBlockBuilder {
            chapter_name: chapter_name.trim().into(),
            graph_name: graph_name.into(),
            code: String::new(),
            path,
        }
    }

    fn append_code<S: Into<String>>(&mut self, code: S) {
        self.code.push_str(&code.into());
    }

    fn build(self, index: usize) -> GraphvizBlock {
        let GraphvizBlockBuilder {
            chapter_name,
            graph_name,
            code,
            path,
        } = self;
        let cleaned_code = code.trim();

        GraphvizBlock {
            graph_name,
            code: cleaned_code.into(),
            chapter_name,
            chapter_path: path,
            index,
        }
    }
}

#[derive(Debug)]
pub struct GraphvizBlock {
    pub graph_name: String,
    pub code: String,
    pub chapter_name: String,
    pub chapter_path: PathBuf,
    pub index: usize,
}

impl GraphvizBlock {
    pub fn file_name(&self) -> String {
        let image_name = if !self.graph_name.is_empty() {
            format!(
                "{}_{}_{}.generated",
                normalize_id(&self.chapter_name),
                normalize_id(&self.graph_name),
                self.index
            )
        } else {
            format!(
                "{}_{}.generated",
                normalize_id(&self.chapter_name),
                self.index
            )
        };

        format!("{image_name}.svg")
    }

    pub fn output_path(&self) -> PathBuf {
        self.chapter_path.join(self.file_name())
    }
}

fn normalize_id(content: &str) -> String {
    content
        .chars()
        .filter_map(|ch| {
            if ch.is_alphanumeric() {
                Some(ch.to_ascii_lowercase())
            } else if ch.is_whitespace() || ch == '_' || ch == '-' {
                Some('_')
            } else {
                None
            }
        })
        .collect::<String>()
}

#[cfg(test)]
mod test {
    use async_trait::async_trait;

    use super::*;

    use std::time::{Duration, Instant};

    static CHAPTER_NAME: &str = "Test Chapter";
    static NORMALIZED_CHAPTER_NAME: &str = "test_chapter";

    struct NoopRenderer;

    #[async_trait]
    impl GraphvizRenderer for NoopRenderer {
        async fn render_graphviz<'a>(block: GraphvizBlock) -> Result<Vec<Event<'a>>> {
            let file_name = block.file_name();
            let output_path = block.output_path();
            let GraphvizBlock {
                graph_name, index, ..
            } = block;

            Ok(vec![Event::Text(
                format!("{file_name}|{output_path:?}|{graph_name}|{index}").into(),
            )])
        }
    }

    #[tokio::test]
    async fn only_preprocess_flagged_blocks() {
        let expected = r#"# Chapter

````dot
digraph Test {
    a -> b
}
````"#;
        let chapter = process_chapter(new_chapter(expected)).await.unwrap();

        assert_eq!(chapter.content, expected);
    }

    #[tokio::test]
    async fn no_name() {
        let chapter = new_chapter(
            r#"# Chapter
```dot process
digraph Test {
    a -> b
}
```
"#,
        );

        let expected = format!(
            r#"# Chapter

{NORMALIZED_CHAPTER_NAME}_0.generated.svg|"/./book/{NORMALIZED_CHAPTER_NAME}_0.generated.svg"||0"#
        );

        let chapter = process_chapter(chapter).await.unwrap();

        assert_eq!(chapter.content, expected);
    }

    #[tokio::test]
    async fn named_blocks() {
        let chapter = new_chapter(
            r#"# Chapter
```dot process Graph Name
digraph Test {
    a -> b
}
```
"#,
        );

        let expected = format!(
            r#"# Chapter

{NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg|"/./book/{NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg"|Graph Name|0"#
        );

        let chapter = process_chapter(chapter).await.unwrap();

        assert_eq!(chapter.content, expected);
    }

    #[tokio::test]
    async fn preserve_escaping() {
        let chapter = new_chapter(
            r#"# Chapter

*asteriks*
/*asteriks/*
( \int x dx = \frac{x^2}{2} + C)

```dot process Graph Name
digraph Test {
    a -> b
}
```
"#,
        );

        let expected = format!(
            r#"# Chapter

*asteriks*
/*asteriks/*
( \int x dx = \frac{{x^2}}{{2}} + C)

{NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg|"/./book/{NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg"|Graph Name|0"#
        );

        let chapter = process_chapter(chapter).await.unwrap();

        assert_eq!(chapter.content, expected);
    }

    #[tokio::test]
    async fn preserve_tables() {
        let chapter = new_chapter(
            r#"# Chapter

|Tables|Are|Cool|
|------|:-:|---:|
|col 1 is|left-aligned|$1600|
|col 2 is|centered|$12|
|col 3 is|right-aligned|$1|

```dot process Graph Name
digraph Test {
    a -> b
}
```
"#,
        );

        let expected = format!(
            r#"# Chapter

|Tables|Are|Cool|
|------|:-:|---:|
|col 1 is|left-aligned|$1600|
|col 2 is|centered|$12|
|col 3 is|right-aligned|$1|

{NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg|"/./book/{NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg"|Graph Name|0"#
        );

        let chapter = process_chapter(chapter).await.unwrap();

        assert_eq!(chapter.content, expected);
    }

    const SLEEP_DURATION: Duration = Duration::from_millis(100);
    struct SleepyRenderer;
    #[async_trait]
    impl GraphvizRenderer for SleepyRenderer {
        async fn render_graphviz<'a>(_block: GraphvizBlock) -> Result<Vec<Event<'a>>> {
            tokio::time::sleep(SLEEP_DURATION).await;
            Ok(vec![Event::Text("".into())])
        }
    }

    /// Test that we are actually running Graphviz concurrently
    #[tokio::test]
    async fn concurrent_execution() {
        const TOTAL_CHAPTERS: usize = 10;
        let mut chapters = Vec::with_capacity(TOTAL_CHAPTERS);
        for _ in 0..TOTAL_CHAPTERS {
            chapters.push(BookItem::Chapter(new_chapter(
                r#"# Chapter
```dot process Graph Name
digraph Test {
    a -> b
}
```
"#,
            )));
        }

        let start = Instant::now();
        Graphviz::<SleepyRenderer>::new(PathBuf::from("/"))
            .process_sub_items(&mut chapters)
            .await
            .unwrap();
        let duration = start.elapsed();

        for item in chapters {
            if let BookItem::Chapter(chapter) = item {
                // make sure we used the correct renderer
                assert_eq!(chapter.content, "# Chapter\n\n");
            } else {
                panic!("We should only have chapters here");
            }
        }

        assert!(
            duration < SLEEP_DURATION * 2,
            "{duration:?} should be less than 2 * {SLEEP_DURATION:?} since we expect some variation when running"
        );
    }

    /// Test that we correctly process Chapter sub-items
    #[tokio::test]
    async fn chapter_sub_items() {
        let content = r#"# Chapter

```dot process Graph Name
digraph Test {
    a -> b
}
```
"#;
        let mut chapter = new_chapter(content);
        chapter
            .sub_items
            .push(BookItem::Chapter(new_chapter(content)));

        let expected = format!(
            r#"# Chapter

{NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg|"/./book/{NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg"|Graph Name|0"#
        );

        let mut chapter = process_chapter(chapter).await.unwrap();

        assert_eq!(chapter.content, expected);
        if let BookItem::Chapter(child_chapter) = chapter.sub_items.remove(0) {
            assert_eq!(child_chapter.content, expected);
        }
    }

    #[tokio::test]
    async fn skip_draft_chapters() {
        let draft_chapter = Chapter::new_draft(CHAPTER_NAME, vec![]);
        let mut book_items = vec![
            BookItem::Chapter(draft_chapter.clone()),
            BookItem::Chapter(new_chapter(
                r#"# Chapter
```dot process Graph Name
digraph Test {
    a -> b
}
```
"#,
            )),
        ];

        Graphviz::<NoopRenderer>::new(PathBuf::from("/"))
            .process_sub_items(&mut book_items)
            .await
            .unwrap();

        assert_eq!(
            book_items,
            vec![
                BookItem::Chapter(draft_chapter),
                BookItem::Chapter(new_chapter(format!(
                    r#"# Chapter

{NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg|"/./book/{NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg"|Graph Name|0"#
                )))
            ]
        )
    }

    async fn process_chapter(chapter: Chapter) -> Result<Chapter> {
        Graphviz::<NoopRenderer>::new(PathBuf::from("/"))
            .process_chapter(chapter)
            .await
    }

    fn new_chapter<S: ToString>(content: S) -> Chapter {
        Chapter::new(
            CHAPTER_NAME,
            content.to_string(),
            PathBuf::from("./book/chapter.md"),
            vec![],
        )
    }
}
