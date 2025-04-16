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
use pulldown_cmark::{CodeBlockKind::Fenced, Event, Tag, TagEnd};
use pulldown_cmark_to_cmark::cmark;

use crate::renderer::{CLIGraphviz, CLIGraphvizToFile, GraphvizRenderer};

pub static PREPROCESSOR_NAME: &str = "graphviz";
pub static DEFAULT_INFO_STRING_PREFIX: &str = "dot process";

pub struct GraphvizConfig {
    pub output_to_file: bool,
    pub link_to_file: bool,
    pub theme_colors: Option<ThemeColors>,
    pub info_string: String,
    pub arguments: Vec<String>,
}

impl Default for GraphvizConfig {
    fn default() -> Self {
        Self {
            output_to_file: false,
            link_to_file: false,
            theme_colors: None,
            info_string: DEFAULT_INFO_STRING_PREFIX.to_string(),
            arguments: vec![String::from("-Tsvg")],
        }
    }
}

pub struct ThemeColors {
    pub foreground: String,
}

pub struct GraphvizPreprocessor;

pub struct Graphviz<R: GraphvizRenderer> {
    src_dir: PathBuf,
    config: GraphvizConfig,
    _phantom: PhantomData<*const R>,
}

impl Preprocessor for GraphvizPreprocessor {
    fn name(&self) -> &str {
        PREPROCESSOR_NAME
    }

    fn run(&self, ctx: &PreprocessorContext, mut book: Book) -> Result<Book> {
        let mut config = GraphvizConfig::default();

        if let Some(ctx_config) = ctx.config.get_preprocessor(self.name()) {
            if let Some(value) = ctx_config.get("output-to-file") {
                config.output_to_file = value
                    .as_bool()
                    .expect("output-to-file option is required to be a boolean");
            }

            if let Some(value) = ctx_config.get("link-to-file") {
                config.link_to_file = value
                    .as_bool()
                    .expect("link-to-file option is required to be a boolean");
            }

            if let Some(value) = ctx_config.get("info-string") {
                config.info_string = value
                    .as_str()
                    .expect("info-string option is required to be a string")
                    .to_string();
            }

            if let Some(value) = ctx_config.get("theme-colors") {
                let theme_colors = value
                    .as_table()
                    .expect("theme-colors option is required to be a table");
                let foreground = theme_colors
                    .get("foreground")
                    .and_then(|v| v.as_str())
                    .expect("theme-colors.foreground is required to be a string")
                    .to_owned();
                assert_eq!(
                    theme_colors.len(),
                    1,
                    "theme-colors table is required to contain 1 field"
                );
                config.theme_colors = Some(ThemeColors { foreground });
            }

            if config.theme_colors.is_some() && config.output_to_file {
                eprintln!("Warning: `theme-colors` and `output-to-file` flags are incompatible with each other");
            }

            if let Some(value) = ctx_config.get("arguments") {
                config.arguments = value
                    .as_array()
                    .expect("arguments option is required to be an array")
                    .iter()
                    .map(|v| {
                        String::from(
                            v.as_str()
                                .expect("arguments option is required to contain strings"),
                        )
                    })
                    .collect()
            }
        }

        let src_dir = ctx.root.clone().join(&ctx.config.book.src);

        // we really only need 1 thread since we're just calling out to the Graphviz CLI
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .unwrap()
            .block_on(async {
                if config.output_to_file {
                    Graphviz::<CLIGraphvizToFile>::new(src_dir, config)
                        .process_sub_items(&mut book.sections)
                        .await
                } else {
                    Graphviz::<CLIGraphviz>::new(src_dir, config)
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
    pub fn new(src_dir: PathBuf, config: GraphvizConfig) -> Graphviz<R> {
        Self {
            src_dir,
            config,
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
                        builder.append_code(text.to_string());
                        graphviz_block_builder = Some(builder);
                    }
                    Event::End(TagEnd::CodeBlock) => {
                        // finish our digraph
                        let block = builder.build(image_index);
                        image_index += 1;

                        event_futures.push(R::render_graphviz(block, &self.config));
                    }
                    _ => {
                        graphviz_block_builder = Some(builder);
                    }
                }
            } else {
                if let Event::Start(Tag::CodeBlock(Fenced(info_string))) = &e {
                    let prefix_len = self.config.info_string.len();
                    // The following split is safe because the characters have
                    // to be byte equal to be a match, therefore we are
                    // guaranteed to split at a character boundary.
                    let (prefix, graph_name) =
                        info_string.split_at(std::cmp::min(info_string.len(), prefix_len));
                    if prefix == self.config.info_string {
                        // check if we can have a name at the end of our info string
                        graphviz_block_builder = Some(GraphvizBlockBuilder::new(
                            chapter_path.clone(),
                            chapter.name.clone().trim().to_string(),
                            graph_name.trim().to_string(),
                        ));
                        continue;
                    }
                }
                // pass through all events that don't impact our Graphviz block
                event_futures.push(Box::pin(async { Ok(vec![e]) }));
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
    path: PathBuf,
    chapter_name: String,
    graph_name: String,
    code: String,
}

impl GraphvizBlockBuilder {
    fn new(path: PathBuf, chapter_name: String, graph_name: String) -> GraphvizBlockBuilder {
        GraphvizBlockBuilder {
            path,
            chapter_name,
            graph_name,
            code: String::new(),
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
        async fn render_graphviz<'a>(
            block: GraphvizBlock,
            _config: &GraphvizConfig,
        ) -> Result<Vec<Event<'a>>> {
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
    async fn preprocess_flagged_blocks_with_custom_flag() {
        let chapter = new_chapter(
            r#"# Chapter
```graphviz
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

        let config = GraphvizConfig {
            info_string: "graphviz".to_string(),
            ..GraphvizConfig::default()
        };
        let chapter = process_chapter_with_config(chapter, config).await.unwrap();

        assert_eq!(chapter.content, expected);
    }

    #[tokio::test]
    async fn do_not_preprocess_flagged_blocks_without_custom_flag() {
        let expected = r#"# Chapter

````dot
digraph Test {
    a -> b
}
````"#;

        let config = GraphvizConfig {
            info_string: "graphviz".to_string(),
            ..GraphvizConfig::default()
        };
        let chapter = process_chapter_with_config(new_chapter(expected), config)
            .await
            .unwrap();

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
            r"# Chapter

*asteriks*
/*asteriks/*
( \int x dx = \frac{x^2}{2} + C)

```dot process Graph Name
digraph Test {
    a -> b
}
```
",
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
        async fn render_graphviz<'a>(
            _block: GraphvizBlock,
            _config: &GraphvizConfig,
        ) -> Result<Vec<Event<'a>>> {
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
        Graphviz::<SleepyRenderer>::new(PathBuf::from("/"), GraphvizConfig::default())
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

        Graphviz::<NoopRenderer>::new(PathBuf::from("/"), GraphvizConfig::default())
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
        process_chapter_with_config(chapter, GraphvizConfig::default()).await
    }

    async fn process_chapter_with_config(
        chapter: Chapter,
        config: GraphvizConfig,
    ) -> Result<Chapter> {
        Graphviz::<NoopRenderer>::new(PathBuf::from("/"), config)
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
