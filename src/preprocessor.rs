use std::marker::PhantomData;
use std::path::PathBuf;

use mdbook::book::{Book, Chapter};
use mdbook::errors::Result;
use mdbook::preprocess::{Preprocessor, PreprocessorContext};
use mdbook::utils::new_cmark_parser;
use mdbook::BookItem;
use pulldown_cmark::{CodeBlockKind, Event, Tag};
use pulldown_cmark_to_cmark::cmark;
use toml::Value;

use crate::renderer::{CLIGraphviz, CLIGraphvizToFile, GraphvizRenderer};

pub static PREPROCESSOR_NAME: &str = "graphviz";
pub static INFO_STRING_PREFIX: &str = "dot process";

pub struct GraphvizPreprocessor;

pub struct Graphviz<R: GraphvizRenderer> {
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
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let src_dir = ctx.root.clone().join(&ctx.config.book.src);
        let mut error = Ok(());

        book.for_each_mut(|item: &mut BookItem| {
            // only continue editing the book if we don't have any errors
            if error.is_ok() {
                if let BookItem::Chapter(ref mut chapter) = item {
                    let path = chapter.path.as_ref().unwrap();
                    let mut full_path = src_dir.join(path);

                    // remove the chapter filename
                    full_path.pop();

                    error = if !output_to_file {
                        Graphviz::<CLIGraphviz>::new().process_chapter(chapter, full_path)
                    } else {
                        Graphviz::<CLIGraphvizToFile>::new().process_chapter(chapter, full_path)
                    };
                }
            }
        });

        error.map(|_| book)
    }

    fn supports_renderer(&self, _renderer: &str) -> bool {
        // since we're just outputting markdown images or inline html, this "should" support any renderer
        true
    }
}

impl<R: GraphvizRenderer> Graphviz<R> {
    fn new() -> Graphviz<R> {
        Graphviz {
            _phantom: PhantomData,
        }
    }

    fn process_chapter(&self, chapter: &mut Chapter, chapter_path: PathBuf) -> Result<()> {
        let mut buf = String::with_capacity(chapter.content.len());
        let mut graphviz_block_builder: Option<GraphvizBlockBuilder> = None;
        let mut image_index = 0;

        let event_results: Result<Vec<Vec<Event>>> = new_cmark_parser(&chapter.content)
            .map(|e| {
                if let Some(mut builder) = graphviz_block_builder.take() {
                    match e {
                        Event::Text(ref text) => {
                            builder.append_code(&**text);
                            graphviz_block_builder = Some(builder);

                            Ok(vec![])
                        }
                        Event::End(Tag::CodeBlock(CodeBlockKind::Fenced(ref info_string))) => {
                            assert_eq!(
                                Some(0),
                                (&**info_string).find(INFO_STRING_PREFIX),
                                "We must close our graphviz block"
                            );

                            // finish our digraph
                            let block = builder.build(image_index);
                            image_index += 1;

                            R::render_graphviz(block)
                        }
                        _ => {
                            graphviz_block_builder = Some(builder);

                            Ok(vec![])
                        }
                    }
                } else {
                    match e {
                        Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(ref info_string)))
                            if (&**info_string).find(INFO_STRING_PREFIX) == Some(0) =>
                        {
                            graphviz_block_builder = Some(GraphvizBlockBuilder::new(
                                &**info_string,
                                &chapter.name.clone(),
                                chapter_path.clone(),
                            ));

                            Ok(vec![])
                        }
                        _ => Ok(vec![e]),
                    }
                }
            })
            .collect();

        // get our result and combine our internal Vecs
        let events = event_results?.into_iter().flatten();

        cmark(events, &mut buf, None)?;

        chapter.content = buf;

        Ok(())
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
            &info_string[INFO_STRING_PREFIX.len() + 1..].trim()
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

        format!("{}.svg", image_name)
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
    use super::*;

    static CHAPTER_NAME: &str = "Test Chapter";
    static NORMALIZED_CHAPTER_NAME: &str = "test_chapter";

    struct NoopRenderer;

    impl GraphvizRenderer for NoopRenderer {
        fn render_graphviz<'a>(block: GraphvizBlock) -> Result<Vec<Event<'a>>> {
            let file_name = block.file_name();
            let output_path = block.output_path();
            let GraphvizBlock {
                graph_name, index, ..
            } = block;

            Ok(vec![Event::Text(
                format!("{}|{:?}|{}|{}", file_name, output_path, graph_name, index).into(),
            )])
        }
    }

    #[test]
    fn only_preprocess_flagged_blocks() {
        let expected = r#"# Chapter

````dot
digraph Test {
    a -> b
}
````"#;

        let mut chapter = new_chapter(expected.into());

        process_chapter(&mut chapter).unwrap();

        assert_eq!(chapter.content, expected);
    }

    #[test]
    fn no_name() {
        let mut chapter = new_chapter(
            r#"# Chapter
```dot process
digraph Test {
    a -> b
}
```
"#
            .into(),
        );

        let expected = format!(
            r#"# Chapter

{}_0.generated.svg|"./{}_0.generated.svg"||0"#,
            NORMALIZED_CHAPTER_NAME, NORMALIZED_CHAPTER_NAME
        );

        process_chapter(&mut chapter).unwrap();

        assert_eq!(chapter.content, expected);
    }

    #[test]
    fn named_blocks() {
        let mut chapter = new_chapter(
            r#"# Chapter
```dot process Graph Name
digraph Test {
    a -> b
}
```
"#
            .into(),
        );

        let expected = format!(
            r#"# Chapter

{}_graph_name_0.generated.svg|"./{}_graph_name_0.generated.svg"|Graph Name|0"#,
            NORMALIZED_CHAPTER_NAME, NORMALIZED_CHAPTER_NAME
        );

        process_chapter(&mut chapter).unwrap();

        assert_eq!(chapter.content, expected);
    }

    #[test]
    fn preserve_escaping() {
        let mut chapter = new_chapter(
            r#"# Chapter

*asteriks*
/*asteriks/*
( \int x dx = \frac{x^2}{2} + C)

```dot process Graph Name
digraph Test {
    a -> b
}
```
"#
            .into(),
        );

        let expected = format!(
            r#"# Chapter

*asteriks*
/*asteriks/*
( \int x dx = \frac{{x^2}}{{2}} + C)

{}_graph_name_0.generated.svg|"./{}_graph_name_0.generated.svg"|Graph Name|0"#,
            NORMALIZED_CHAPTER_NAME, NORMALIZED_CHAPTER_NAME
        );

        process_chapter(&mut chapter).unwrap();

        assert_eq!(chapter.content, expected);
    }

    #[test]
    fn preserve_tables() {
        let mut chapter = new_chapter(
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
"#
            .into(),
        );

        let expected = format!(
            r#"# Chapter

|Tables|Are|Cool|
|------|:-:|---:|
|col 1 is|left-aligned|$1600|
|col 2 is|centered|$12|
|col 3 is|right-aligned|$1|

{}_graph_name_0.generated.svg|"./{}_graph_name_0.generated.svg"|Graph Name|0"#,
            NORMALIZED_CHAPTER_NAME, NORMALIZED_CHAPTER_NAME
        );

        process_chapter(&mut chapter).unwrap();

        assert_eq!(chapter.content, expected);
    }

    fn process_chapter(chapter: &mut Chapter) -> Result<()> {
        let graphviz = Graphviz::<NoopRenderer>::new();

        graphviz.process_chapter(chapter, PathBuf::from("./"))
    }

    fn new_chapter(content: String) -> Chapter {
        Chapter::new(CHAPTER_NAME, content.into(), PathBuf::from("./"), vec![])
    }
}
