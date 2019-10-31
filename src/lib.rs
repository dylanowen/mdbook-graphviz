use mdbook::book::{Book, Chapter};
use mdbook::errors::Error;
use mdbook::errors::ErrorKind;
use mdbook::errors::Result;
use mdbook::preprocess::{Preprocessor, PreprocessorContext};
use mdbook::BookItem;
use pulldown_cmark::{Event, LinkType, Parser, Tag};
use pulldown_cmark_to_cmark::fmt::cmark;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub static PREPROCESSOR_NAME: &str = "mdbook-graphviz";
pub static INFO_STRING_PREFIX: &str = "dot preprocess";

pub struct Graphviz;

impl Graphviz {
    pub fn new() -> Graphviz {
        Graphviz
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

                    error = Graphviz::process_chapter(chapter, &full_path)
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
    fn process_chapter(chapter: &mut Chapter, chapter_path: &PathBuf) -> Result<()> {
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

                            block.compile_graphviz()?;

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
    graph_name: String,
    code: String,
    path: PathBuf,
}

impl GraphvizBlockBuilder {
    fn new<S: Into<String>>(info_string: S, path: PathBuf) -> GraphvizBlockBuilder {
        let info_string: String = info_string.into();
        let mut graph_name = "";

        // check if we can have a name at the end of our info string
        if Some(' ') == info_string.chars().nth(INFO_STRING_PREFIX.len()) {
            graph_name = &info_string[INFO_STRING_PREFIX.len() + 1..];
        }

        GraphvizBlockBuilder {
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

        GraphvizBlock::new(
            self.graph_name.clone(),
            format!("{}.generated", index),
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

    fn compile_graphviz(&self) -> Result<()> {
        let output_path = self.chapter_path.join(self.file_name());
        let output_path_str = output_path.to_str().ok_or_else(|| {
            ErrorKind::Io(io::Error::new(
                io::ErrorKind::NotFound,
                "Couldn't build output path",
            ))
        })?;

        let mut child = Command::new("dot")
            .args(&["-Tsvg", "-o", output_path_str])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(self.code.as_bytes())?;
        }

        if child.wait()?.success() {
            Ok(())
        } else {
            Err(ErrorKind::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                "Error response from Graphviz",
            ))
            .into())
        }
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

#[cfg(test)]
mod test {
    use super::Graphviz;
    use mdbook::book::Chapter;
    use std::path::PathBuf;

    //    // test non processed
    //    ```dot
    //    digraph Test {
    //    a -> b
    //}
    //```
    //
    //// test no name
    //```dot preprocess
    //digraph Test {
    //a -> b
    //}
    //```
    //
    //// test no name
    //```dot preprocess
    //digraph Test {
    //a -> b
    //}
    //```
    //
    //// test with name
    //```dot preprocess Something
    //digraph Test {
    //a -> b
    //}
    //```
    //
    //// test newlines after images

    #[test]
    fn adds_image() {
        let content = r#"# Chapter
```dot
digraph Test {
    a -> b
}
```

```dot preprocess
digraph Test {
    a -> b
}
```
"#;
        let mut chapter = Chapter::new("chapter", content.into(), PathBuf::from("./"), vec![]);

        let expected = r#"# Chapter

  ![](Output.png)"#;

        Graphviz::process_chapter(&mut chapter, &PathBuf::from("./")).unwrap();

        assert_eq!(expected, chapter.content);
    }
}
