use mdbook::book::{Book, Chapter};
use mdbook::errors::Error;
use mdbook::errors::Result;
use mdbook::preprocess::{Preprocessor, PreprocessorContext};
use mdbook::BookItem;
use pulldown_cmark::{Event, LinkType, Parser, Tag};
use pulldown_cmark_to_cmark::fmt::cmark;
use std::path::PathBuf;

use crate::renderer::{CommandLineGraphviz, GraphvizRenderer};

pub static PREPROCESSOR_NAME: &str = "mdbook-graphviz";
pub static INFO_STRING_PREFIX: &str = "dot process";

pub struct Graphviz {
    renderer: Box<dyn GraphvizRenderer>,
}

impl Graphviz {
    pub fn command_line_renderer() -> Graphviz {
        let renderer = CommandLineGraphviz;

        Graphviz {
            renderer: Box::new(renderer),
        }
    }
}

impl Preprocessor for Graphviz {
    fn name(&self) -> &str {
        PREPROCESSOR_NAME
    }

    fn run(&self, ctx: &PreprocessorContext, mut book: Book) -> Result<Book> {
        let src_dir = ctx.root.clone().join(&ctx.config.book.src);
        let mut error = Ok(());

        book.for_each_mut(|item: &mut BookItem| {
            // only continue editing the book if we don't have any errors
            if error.is_ok() {
                if let BookItem::Chapter(ref mut chapter) = item {
                    let mut full_path = src_dir.join(&chapter.path);

                    // remove the chapter filename
                    full_path.pop();

                    error = self.process_chapter(chapter, &full_path)
                }
            }
        });

        error.map(|_| book)
    }

    fn supports_renderer(&self, _renderer: &str) -> bool {
        // since we're just outputting markdown images, this should support any renderer
        true
    }
}

impl Graphviz {
    fn process_chapter(&self, chapter: &mut Chapter, chapter_path: &PathBuf) -> Result<()> {
        let mut buf = String::with_capacity(chapter.content.len());
        let mut graphviz_block_builder: Option<GraphvizBlockBuilder> = None;
        let mut image_index = 0;

        let event_results: Result<Vec<Vec<Event>>> = Parser::new(&chapter.content)
            .map(|e| {
                if let Some(ref mut builder) = graphviz_block_builder {
                    match e {
                        Event::Text(ref text) => {
                            builder.append_code(&**text);

                            Ok(vec![])
                        }
                        Event::End(Tag::CodeBlock(ref info_string)) => {
                            assert_eq!(
                                Some(0),
                                (&**info_string).find(INFO_STRING_PREFIX),
                                "We must close our graphviz block"
                            );

                            // finish our digraph
                            let block = builder.build(image_index);
                            image_index += 1;
                            graphviz_block_builder = None;

                            block.render_graphviz(&*self.renderer)?;

                            Ok(block.tag_events())
                        }
                        _ => Ok(vec![]),
                    }
                } else {
                    match e {
                        Event::Start(Tag::CodeBlock(ref info_string))
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
        let events = event_results?.into_iter().flat_map(|e| e);

        cmark(events, &mut buf, None)
            .map_err(|err| Error::from(format!("Markdown serialization failed: {}", err)))?;

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

        let mut graph_name = "";
        // check if we can have a name at the end of our info string
        if Some(' ') == info_string.chars().nth(INFO_STRING_PREFIX.len()) {
            graph_name = &info_string[INFO_STRING_PREFIX.len() + 1..].trim();
        }

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

    fn build(&self, index: usize) -> GraphvizBlock {
        let cleaned_code = self.code.trim();

        let image_name = if !self.graph_name.is_empty() {
            format!(
                "{}_{}_{}.generated",
                normalize_id(&self.chapter_name),
                normalize_id(&self.graph_name),
                index
            )
        } else {
            format!("{}_{}.generated", normalize_id(&self.chapter_name), index)
        };

        GraphvizBlock::new(
            self.graph_name.clone(),
            image_name,
            cleaned_code.into(),
            self.path.clone(),
        )
    }
}

struct GraphvizBlock {
    graph_name: String,
    image_name: String,
    code: String,
    chapter_path: PathBuf,
}

impl GraphvizBlock {
    fn new<S: Into<String>>(graph_name: S, image_name: S, code: S, path: PathBuf) -> GraphvizBlock {
        let image_name = image_name.into();

        GraphvizBlock {
            graph_name: graph_name.into(),
            image_name,
            code: code.into(),
            chapter_path: path,
        }
    }

    fn tag_events<'a, 'b>(&'a self) -> Vec<Event<'b>> {
        vec![
            Event::Start(self.image_tag()),
            Event::End(self.image_tag()),
            Event::Text("\n\n".into()),
        ]
    }

    fn render_graphviz(&self, renderer: &dyn GraphvizRenderer) -> Result<()> {
        let output_path = self.chapter_path.join(self.file_name());

        renderer.render_graphviz(&self.code, &output_path)
    }

    fn image_tag<'a, 'b>(&'a self) -> Tag<'b> {
        Tag::Image(
            LinkType::Inline,
            self.file_name().into(),
            self.graph_name.clone().into(),
        )
    }

    fn file_name(&self) -> String {
        format!("{}.svg", self.image_name)
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
        fn render_graphviz(&self, _code: &String, _output_path: &PathBuf) -> Result<()> {
            Ok(())
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

        assert_eq!(expected, chapter.content);
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

![]({}_0.generated.svg)

"#,
            NORMALIZED_CHAPTER_NAME
        );

        process_chapter(&mut chapter).unwrap();

        assert_eq!(expected, chapter.content);
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

![]({}_graph_name_0.generated.svg "Graph Name")

"#,
            NORMALIZED_CHAPTER_NAME
        );

        process_chapter(&mut chapter).unwrap();

        assert_eq!(expected, chapter.content);
    }

    fn process_chapter(chapter: &mut Chapter) -> Result<()> {
        let graphviz = Graphviz {
            renderer: Box::new(NoopRenderer),
        };

        graphviz.process_chapter(chapter, &PathBuf::from("./"))
    }

    fn new_chapter(content: String) -> Chapter {
        Chapter::new(CHAPTER_NAME, content.into(), PathBuf::from("./"), vec![])
    }
}
