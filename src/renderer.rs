use regex::RegexBuilder;
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::io;
use std::iter;
use std::process::Stdio;
use tokio::process::{Child, Command};

use async_trait::async_trait;
use mdbook::errors::Result;
use pulldown_cmark::{Event, LinkType, Tag, TagEnd};
use regex::Regex;

use crate::preprocessor::{GraphvizBlock, GraphvizConfig};
use tokio::io::AsyncWriteExt;

type RegexResult<T> = std::result::Result<T, regex::Error>;

#[async_trait]
pub trait GraphvizRenderer {
    async fn render_graphviz<'a>(
        block: GraphvizBlock,
        config: &GraphvizConfig,
    ) -> Result<Vec<Event<'a>>>;
}

pub struct CLIGraphviz;

#[async_trait]
impl GraphvizRenderer for CLIGraphviz {
    async fn render_graphviz<'a>(
        GraphvizBlock { code, .. }: GraphvizBlock,
        config: &GraphvizConfig,
    ) -> Result<Vec<Event<'a>>> {
        let reserved_color = config
            .respect_theme
            .then(|| reserve_color_code(&code))
            .transpose()?;

        let append_arguments = reserved_color.map(respect_theme_args);
        let output = call_graphviz(
            config
                .arguments
                .iter()
                .chain(append_arguments.iter().flatten()),
            &code,
        )
        .await?
        .wait_with_output()
        .await?;

        if !output.status.success() {
            return Err(
                io::Error::new(io::ErrorKind::InvalidData, "Error response from Graphviz").into(),
            );
        }

        let mut graph_svg = String::from_utf8(output.stdout)?;
        if let Some(reserved) = reserved_color {
            replace_color_with_fg(&mut graph_svg, reserved)?;
        }

        Ok(vec![
            Event::Html(format_output(&graph_svg).into()),
            Event::Text("\n\n".into()),
        ])
    }
}

pub struct CLIGraphvizToFile;

#[async_trait]
impl GraphvizRenderer for CLIGraphvizToFile {
    async fn render_graphviz<'a>(
        block: GraphvizBlock,
        config: &GraphvizConfig,
    ) -> Result<Vec<Event<'a>>> {
        // For some reason files cannot depend on CSS variables, so ignore `config.respect_theme`

        let file_name = block.file_name();
        let output_path = block.output_path();
        let GraphvizBlock {
            graph_name, code, ..
        } = block;

        let output_path_str = output_path
            .to_str()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Couldn't build output path"))?;

        let mut args_with_output = config.arguments.clone();
        args_with_output.extend(["-o", output_path_str].iter().map(|s| s.to_string()));

        if call_graphviz(&args_with_output, &code)
            .await?
            .wait()
            .await?
            .success()
        {
            let mut nodes = vec![];

            if config.link_to_file {
                let link_tag = Tag::Link {
                    link_type: LinkType::Inline,
                    dest_url: file_name.clone().into(),
                    title: graph_name.clone().into(),
                    id: "".into(),
                };
                nodes.push(Event::Start(link_tag));
            }

            let image_tag = Tag::Image {
                link_type: LinkType::Inline,
                dest_url: file_name.into(),
                title: graph_name.into(),
                id: "".into(),
            };

            nodes.extend([Event::Start(image_tag), Event::End(TagEnd::Image)]);

            if config.link_to_file {
                nodes.push(Event::End(TagEnd::Link));
            }
            nodes.push(Event::Text("\n\n".into()));

            Ok(nodes)
        } else {
            Err(io::Error::new(io::ErrorKind::InvalidData, "Error response from Graphviz").into())
        }
    }
}

async fn call_graphviz<I, S>(arguments: I, code: &str) -> Result<Child>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut child = Command::new("dot")
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(code.as_bytes()).await?;
    }

    Ok(child)
}

/// Reserve unused color code
fn reserve_color_code(source: &str) -> io::Result<u32> {
    lazy_static! {
        static ref COLOR_CODES: Regex = Regex::new(r##""#[0-9a-fA-F]{6}""##).unwrap();
    }

    // Reserve one free hexadecimal color code
    let color_codes: BTreeSet<u32> = COLOR_CODES
        .find_iter(source)
        .map(|m| u32::from_str_radix(m.as_str().trim_matches(['"', '#']), 16))
        .chain(iter::once(Ok(0))) // add plain black in case no color codes are found
        .collect::<Result<_, _>>()
        .unwrap();
    (0..=0xffffff)
        .rev()
        .zip(color_codes.iter().rev())
        .find_map(|(candidate, found)| (candidate != *found).then_some(candidate))
        .ok_or(io::Error::new(
            io::ErrorKind::FileTooLarge,
            "Out of hexadecimal color literals",
        ))
}

/// Required CLI arguments to respect mdbook theme
fn respect_theme_args(reserved_color: u32) -> [String; 5] {
    [
        format!("-Nfontcolor=#{reserved_color:x}"),
        format!("-Ncolor=#{reserved_color:x}"),
        format!("-Ecolor=#{reserved_color:x}"),
        format!("-Efontcolor=#{reserved_color:x}"),
        "-Gbgcolor=transparent".to_owned(),
    ]
}

/// Replace color code with "var(--fg)"
fn replace_color_with_fg(text: &mut String, color_code: u32) -> RegexResult<()> {
    let reserved_color_code = RegexBuilder::new(&format!(r##""#{color_code:x}""##))
        .case_insensitive(true)
        .build()?;
    let processed = reserved_color_code.replace_all(text, "\"var(--fg)\"");
    // `Regex::replace_all` would return `Cow::Borrowed` if no replacements were made
    if let Cow::Owned(processed) = processed {
        *text = processed;
    }
    Ok(())
}

fn format_output(output: &str) -> String {
    lazy_static! {
        static ref DOCTYPE_RE: Regex = Regex::new(r"<!DOCTYPE [^>]+>").unwrap();
        static ref XML_TAG_RE: Regex = Regex::new(r"<\?xml [^>]+\?>").unwrap();
        static ref NEW_LINE_TAGS_RE: Regex = Regex::new(r">\s+<").unwrap();
        static ref NEWLINES_RE: Regex = Regex::new(r"\n").unwrap();
    }

    // yes yes: https://stackoverflow.com/a/1732454 ZA̡͊͠͝LGΌ and such
    let output = DOCTYPE_RE.replace(output, "");
    let output = XML_TAG_RE.replace(&output, "");
    // remove newlines between our tags to help commonmark determine the full set of HTML
    let output = NEW_LINE_TAGS_RE.replace_all(&output, "><");
    // remove explicit newlines as they won't be preserved and break commonmark parsing
    let output = NEWLINES_RE.replace_all(&output, "");
    let output = output.trim();

    format!("<div class=\"mdbook-graphviz-output\">{output}</div>")
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

        let config = GraphvizConfig::default();
        let mut events = CLIGraphviz::render_graphviz(block, &config)
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

    #[tokio::test]
    async fn file_events() {
        let code = r#"digraph Test { a -> b }"#;

        let block = GraphvizBlock {
            graph_name: "Name".into(),
            code: code.into(),
            chapter_name: "".into(),
            chapter_path: "test-output".into(),
            index: 0,
        };

        let config = GraphvizConfig::default();
        let mut events = CLIGraphvizToFile::render_graphviz(block, &config)
            .await
            .expect("Expect rendering to succeed")
            .into_iter();
        let next = events.next();
        assert!(
            matches!(next, Some(Event::Start(Tag::Image { .. }))),
            "Expected Image got {next:#?}"
        );
        let next = events.next();
        assert!(
            matches!(next, Some(Event::End(TagEnd::Image))),
            "Expected End Image got {next:#?}"
        );
        assert_eq!(events.next(), Some(Event::Text("\n\n".into())));
        assert_eq!(events.next(), None);
    }

    #[tokio::test]
    async fn file_events_with_link() {
        let code = r#"digraph Test { a -> b }"#;

        let block = GraphvizBlock {
            graph_name: "Name".into(),
            code: code.into(),
            chapter_name: "".into(),
            chapter_path: "test-output".into(),
            index: 0,
        };

        let config = GraphvizConfig {
            link_to_file: true,
            ..GraphvizConfig::default()
        };
        let mut events = CLIGraphvizToFile::render_graphviz(block, &config)
            .await
            .expect("Expect rendering to succeed")
            .into_iter();
        let next = events.next();
        assert!(
            matches!(next, Some(Event::Start(Tag::Link { .. }))),
            "Expected Link got {next:#?}"
        );
        let next = events.next();
        assert!(
            matches!(next, Some(Event::Start(Tag::Image { .. }))),
            "Expected Image got {next:#?}"
        );
        let next = events.next();
        assert!(
            matches!(next, Some(Event::End(TagEnd::Image))),
            "Expected End Image got {next:#?}"
        );
        let next = events.next();
        assert!(
            matches!(next, Some(Event::End(TagEnd::Link))),
            "Expected End Link got {next:#?}"
        );
        assert_eq!(events.next(), Some(Event::Text("\n\n".into())));
        assert_eq!(events.next(), None);
    }
}
