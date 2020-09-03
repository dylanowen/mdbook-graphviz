use std::io;
use std::io::Write;
use std::process::{Child, Command, Stdio};

use mdbook::errors::Result;
use pulldown_cmark::{Event, LinkType, Tag};
use regex::Regex;

use crate::preprocessor::GraphvizBlock;

pub trait GraphvizRenderer {
    fn render_graphviz<'a>(block: GraphvizBlock) -> Result<Vec<Event<'a>>>;
}

pub struct CLIGraphviz;

impl GraphvizRenderer for CLIGraphviz {
    fn render_graphviz<'a>(GraphvizBlock { code, .. }: GraphvizBlock) -> Result<Vec<Event<'a>>> {
        let output = call_graphviz(&["-Tsvg"], &code)?.wait_with_output()?;
        if output.status.success() {
            let graph_svg = String::from_utf8(output.stdout)?;

            Ok(vec![
                Event::Start(Tag::HtmlBlock),
                Event::Text(format_output(graph_svg).into()),
                Event::End(Tag::HtmlBlock),
                Event::Text("\n\n".into()),
            ])
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Error response from Graphviz",
            )
            .into())
        }
    }
}

pub struct CLIGraphvizToFile;

impl GraphvizRenderer for CLIGraphvizToFile {
    fn render_graphviz<'a>(block: GraphvizBlock) -> Result<Vec<Event<'a>>> {
        let file_name = block.file_name();
        let output_path = block.output_path();
        let GraphvizBlock {
            graph_name, code, ..
        } = block;

        let output_path_str = output_path.to_str().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Couldn't build output path",
            )
        })?;

        if call_graphviz(&["-Tsvg", "-o", output_path_str], &code)?
            .wait()?
            .success()
        {
            let image_tag = Tag::Image(LinkType::Inline, file_name.into(), graph_name.into());

            Ok(vec![
                Event::Start(image_tag.clone()),
                Event::End(image_tag),
                Event::Text("\n\n".into()),
            ])
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Error response from Graphviz",
            )
            .into())
        }
    }
}

fn call_graphviz(args: &[&str], code: &str) -> Result<Child> {
    let mut child = Command::new("dot")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(code.as_bytes())?;
    }

    Ok(child)
}

fn format_output(output: String) -> String {
    lazy_static! {
        static ref DOCTYPE_RE: Regex = Regex::new(r"<!DOCTYPE [^>]+>").unwrap();
        static ref XML_TAG_RE: Regex = Regex::new(r"<\?xml [^>]+\?>").unwrap();
        static ref NEW_LINE_TAGS_RE: Regex = Regex::new(r">\s+<").unwrap();
    }

    // yes yes: https://stackoverflow.com/a/1732454 ZA̡͊͠͝LGΌ and such
    let output = DOCTYPE_RE.replace(&output, "");
    let output = XML_TAG_RE.replace(&output, "");
    // remove newlines between our tags to help commonmark determine the full set of HTML
    let output = NEW_LINE_TAGS_RE.replace_all(&output, "><");
    let output = output.trim();

    format!("<div>{}</div>", output)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn inline_events() {
        let code = r#"digraph Test { a -> b }"#;

        let block = GraphvizBlock {
            graph_name: "Name".into(),
            code: code.into(),
            chapter_name: "".into(),
            chapter_path: "".into(),
            index: 0,
        };

        let mut events = CLIGraphviz::render_graphviz(block).unwrap().into_iter();
        assert_eq!(events.next(), Some(Event::Start(Tag::HtmlBlock)));
        if let Some(Event::Text(_)) = events.next() {
        } else {
            panic!("Unexpected next event")
        }
        assert_eq!(events.next(), Some(Event::End(Tag::HtmlBlock)));
        assert_eq!(events.next(), None);
    }
}
