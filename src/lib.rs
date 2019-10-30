use mdbook::book::{Book, Chapter};
use mdbook::errors::Error;
use mdbook::errors::Result;
use mdbook::preprocess::{Preprocessor, PreprocessorContext};
use mdbook::BookItem;
use pulldown_cmark::{Event, LinkType, Parser, Tag};
use pulldown_cmark_to_cmark::fmt::cmark;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub static PREPROCESSOR_NAME: &str = "mdbook-graphviz";

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
        let mut src_dir = ctx.root.clone();
        src_dir.push(&ctx.config.book.src);
        let mut error = Ok(());

        book.for_each_mut(|item: &mut BookItem| {
            // only continue editing the book if we don't have any errors
            if error.is_ok() {
                if let BookItem::Chapter(ref mut chapter) = item {
                    let mut full_path = src_dir.clone();
                    full_path.push(&chapter.path);
                    //full_path.push("./bad/bad");
                    full_path.pop();

                    error = Graphviz::process_chapter(chapter, &full_path)
                    //                        .map(
                    //                        |processed_chapter| {
                    //                            chapter.content = processed_chapter;
                    //                            ()
                    //                        },
                    //                    );
                }
            }
        });

        // In testing we want to tell the preprocessor to blow up by setting a
        // particular config value
        //        if let Some(nop_cfg) = ctx.config.get_preprocessor(self.name()) {
        //            if nop_cfg.contains_key("blow-up") {
        //                return Err("Boom!!1!".into());
        //            }
        //        }
        //        let events = Parser::new(content).map(|e| e);
        //
        //        cmark(events, &mut buf, None)
        //            .map(|_| buf)
        //            .map_err(|err| Error::from(format!("Markdown serialization failed: {}", err)))

        //        // we *are* a no-op preprocessor after all
        //        Ok(book)
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

        let events = Parser::new(&chapter.content).flat_map(|e| {
            if let Some(ref mut builder) = graphviz_block_builder {
                match e {
                    Event::Text(ref text) => {
                        builder.append_code(&**text);

                        vec![]
                    }
                    Event::End(Tag::CodeBlock(ref code_type)) => {
                        assert_eq!("dot", &**code_type, "We must close our graphviz block");

                        // finish our digraph
                        let block = builder.build(image_index);
                        image_index += 1;
                        graphviz_block_builder = None;

                        block.compile_graphviz().expect("succ");

                        block.tag_events()
                    }
                    _ => vec![e],
                }
            } else {
                match e {
                    Event::Start(Tag::CodeBlock(ref code_type)) if &**code_type == "dot" => {
                        graphviz_block_builder =
                            Some(GraphvizBlockBuilder::new(chapter_path.clone()));

                        vec![]
                    }
                    _ => vec![e],
                }
            }
        });
        //
        //        Command::new("graphviz")
        //            .args(&["src/hello.c", "-c", "-fPIC", "-o"])
        //            .arg(&format!("{}/hello.o", out_dir))
        //            .status()
        //            .unwrap();

        cmark(events, &mut buf, None)
            .map_err(|err| Error::from(format!("Markdown serialization failed: {}", err)))?;

        chapter.content = buf;

        Ok(())
    }
}

struct GraphvizBlockBuilder {
    code: String,
    path: PathBuf,
}

impl GraphvizBlockBuilder {
    fn new(path: PathBuf) -> GraphvizBlockBuilder {
        GraphvizBlockBuilder {
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
            "".into(),
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
        vec![Event::Start(self.image_tag()), Event::End(self.image_tag())]
    }

    fn compile_graphviz(&self) -> Result<()> {
        //println!("gong {} {} {}", "-Tsvg", "-o", &self.file_name());
        let mut output_path = self.chapter_path.clone();
        output_path.push(self.file_name());
        let output_path_str = output_path.to_str().unwrap();
        // TODO ok_or

        let mut child = Command::new("dot")
            .args(&["-Tsvg", "-o", output_path_str])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        //println!("{:?}", child);

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(self.code.as_bytes())?;
        }

        let output = child.wait()?;

        //.chain_err(|| "Error waiting for the preprocessor to complete")?;

        //.map_err(|e| Error::Subprocess::new())

        Ok(())
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

//#[cfg(test)]
//mod test {
//    use super::Graphviz;
//
//    #[test]
//    fn adds_image() {
//        let content = r#"# Chapter
//```dot
//digraph Test {
//    a -> b
//}
//```"#;
//
//        let expected = r#"# Chapter
//
//  ![](Output.png)"#;
//
//        assert_eq!(
//            expected,
//            Graphviz::process_chapter(content).unwrap()
//        );
//    }
//}
