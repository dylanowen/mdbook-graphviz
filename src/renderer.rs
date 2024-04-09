use std::io;
use std::process::Stdio;
use tokio::process::{Child, Command};

use async_trait::async_trait;
use mdbook::errors::Result;
use pulldown_cmark::{Event, LinkType, Tag, TagEnd};
use regex::Regex;

use crate::preprocessor::GraphvizBlock;
use tokio::io::AsyncWriteExt;

#[async_trait]
pub trait GraphvizRenderer {
    async fn render_graphviz<'a>(block: GraphvizBlock) -> Result<Vec<Event<'a>>>;
}

pub struct CLIGraphviz;

#[async_trait]
impl GraphvizRenderer for CLIGraphviz {
    async fn render_graphviz<'a>(
        GraphvizBlock { code, .. }: GraphvizBlock,
    ) -> Result<Vec<Event<'a>>> {
        let output = call_graphviz(&["-Tsvg"], &code)
            .await?
            .wait_with_output()
            .await?;
        if output.status.success() {
            let graph_svg = String::from_utf8(output.stdout)?;

            Ok(vec![
                Event::Html(format_output(graph_svg).into()),
                Event::Text("\n\n".into()),
            ])
        } else {
            Err(io::Error::new(io::ErrorKind::InvalidData, "Error response from Graphviz").into())
        }
    }
}

pub struct CLIGraphvizToFile;

#[async_trait]
impl GraphvizRenderer for CLIGraphvizToFile {
    async fn render_graphviz<'a>(block: GraphvizBlock) -> Result<Vec<Event<'a>>> {
        let file_name = block.file_name();
        let output_path = block.output_path();
        let GraphvizBlock {
            graph_name, code, ..
        } = block;

        let output_path_str = output_path
            .to_str()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Couldn't build output path"))?;

        if call_graphviz(&["-Tsvg", "-o", output_path_str], &code)
            .await?
            .wait()
            .await?
            .success()
        {
            let image_tag = Tag::Image {
                link_type: LinkType::Inline,
                dest_url: file_name.into(),
                title: graph_name.into(),
                id: "".into(),
            };

            Ok(vec![
                Event::Start(image_tag),
                Event::End(TagEnd::Image),
                Event::Text("\n\n".into()),
            ])
        } else {
            Err(io::Error::new(io::ErrorKind::InvalidData, "Error response from Graphviz").into())
        }
    }
}

async fn call_graphviz(args: &[&str], code: &str) -> Result<Child> {
    let mut child = Command::new("dot")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(code.as_bytes()).await?;
    }

    Ok(child)
}

fn format_output(output: String) -> String {
    lazy_static! {
        static ref DOCTYPE_RE: Regex = Regex::new(r"<!DOCTYPE [^>]+>").unwrap();
        static ref XML_TAG_RE: Regex = Regex::new(r"<\?xml [^>]+\?>").unwrap();
        static ref NEW_LINE_TAGS_RE: Regex = Regex::new(r">\s+<").unwrap();
        static ref NEWLINES_RE: Regex = Regex::new(r"\n").unwrap();
    }

    // yes yes: https://stackoverflow.com/a/1732454 ZA̡͊͠͝LGΌ and such
    let output = DOCTYPE_RE.replace(&output, "");
    let output = XML_TAG_RE.replace(&output, "");
    // remove newlines between our tags to help commonmark determine the full set of HTML
    let output = NEW_LINE_TAGS_RE.replace_all(&output, "><");
    // remove explicit newlines as they won't be preserved and break commonmark parsing
    let output = NEWLINES_RE.replace_all(&output, "");
    let output = output.trim();

    format!("<div>{output}</div>")
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn inline_events() {
        let code = r#"digraph Test { a -> b }"#;

        let block = GraphvizBlock {
            graph_name: "Name".into(),
            code: code.into(),
            chapter_name: "".into(),
            chapter_path: "".into(),
            index: 0,
        };

        let mut events = CLIGraphviz::render_graphviz(block)
            .await
            .unwrap()
            .into_iter();
        if let Some(Event::Html(_)) = events.next() {
        } else {
            panic!("Unexpected next event")
        }
        assert_eq!(events.next(), Some(Event::Text("\n\n".into())));
        assert_eq!(events.next(), None);
    }
}
