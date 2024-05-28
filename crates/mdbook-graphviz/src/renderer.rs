use std::path::Path;
use std::process::Stdio;

use anyhow::{anyhow, Result};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};

use mdbook_svg_inline_preprocessor::{SvgBlock, SvgOutput, SvgRenderer, SvgRendererSharedConfig};

pub struct GraphvizRenderer {
    config: SvgRendererSharedConfig,
    pub arguments: Vec<String>,
}

impl GraphvizRenderer {
    pub fn new(config: SvgRendererSharedConfig) -> Self {
        Self {
            config,
            arguments: vec![String::from("-Tsvg")],
        }
    }
}

impl SvgRenderer for GraphvizRenderer {
    fn info_string(&self) -> &str {
        &self.config.info_string
    }

    fn copy_js(&self) -> Option<&Path> {
        self.config.copy_js.as_deref()
    }

    fn copy_css(&self) -> Option<&Path> {
        self.config.copy_css.as_deref()
    }

    fn output_to_file(&self) -> bool {
        self.config.output_to_file
    }

    fn link_to_file(&self) -> bool {
        self.config.link_to_file
    }

    async fn render_svgs(&self, block: &SvgBlock) -> Result<Vec<SvgOutput>> {
        let output = call_graphviz(&self.arguments, block.source_code())
            .await?
            .wait_with_output()
            .await?;

        if output.status.success() {
            let source = String::from_utf8(output.stdout)?;

            Ok(vec![SvgOutput {
                relative_id: None,
                title: block.graph_name().clone().unwrap_or_default(),
                source,
            }])
        } else {
            Err(anyhow!(
                "{}: Error response from Graphviz",
                block.location_string(None, None)
            ))
        }
    }
}

async fn call_graphviz(arguments: &Vec<String>, code: &str) -> Result<Child> {
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

// #[cfg(test)]
// mod test {
//     use super::*;
//
//     #[tokio::test]
//     async fn inline_events() {
//         let code = r#"digraph Test { a -> b }"#;
//
//         let block = GraphvizBlock {
//             graph_name: "Name".into(),
//             code: code.into(),
//             chapter_name: "".into(),
//             chapter_path: "".into(),
//             index: 0,
//         };
//
//         let config = GraphvizConfig::default();
//         let mut events = CLIGraphviz::render_graphviz(block, &config)
//             .await
//             .unwrap()
//             .into_iter();
//         if let Some(Event::Html(_)) = events.next() {
//         } else {
//             panic!("Unexpected next event")
//         }
//         assert_eq!(events.next(), Some(Event::Text("\n\n".into())));
//         assert_eq!(events.next(), None);
//     }
//
//     #[tokio::test]
//     async fn file_events() {
//         let code = r#"digraph Test { a -> b }"#;
//
//         let block = GraphvizBlock {
//             graph_name: "Name".into(),
//             code: code.into(),
//             chapter_name: "".into(),
//             chapter_path: "test-output".into(),
//             index: 0,
//         };
//
//         let config = GraphvizConfig::default();
//         let mut events = CLIGraphvizToFile::render_graphviz(block, &config)
//             .await
//             .expect("Expect rendering to succeed")
//             .into_iter();
//         let next = events.next();
//         assert!(
//             matches!(next, Some(Event::Start(Tag::Image { .. }))),
//             "Expected Image got {next:#?}"
//         );
//         let next = events.next();
//         assert!(
//             matches!(next, Some(Event::End(TagEnd::Image))),
//             "Expected End Image got {next:#?}"
//         );
//         assert_eq!(events.next(), Some(Event::Text("\n\n".into())));
//         assert_eq!(events.next(), None);
//     }
//
//     #[tokio::test]
//     async fn file_events_with_link() {
//         let code = r#"digraph Test { a -> b }"#;
//
//         let block = GraphvizBlock {
//             graph_name: "Name".into(),
//             code: code.into(),
//             chapter_name: "".into(),
//             chapter_path: "test-output".into(),
//             index: 0,
//         };
//
//         let config = GraphvizConfig {
//             link_to_file: true,
//             ..GraphvizConfig::default()
//         };
//         let mut events = CLIGraphvizToFile::render_graphviz(block, &config)
//             .await
//             .expect("Expect rendering to succeed")
//             .into_iter();
//         let next = events.next();
//         assert!(
//             matches!(next, Some(Event::Start(Tag::Link { .. }))),
//             "Expected Link got {next:#?}"
//         );
//         let next = events.next();
//         assert!(
//             matches!(next, Some(Event::Start(Tag::Image { .. }))),
//             "Expected Image got {next:#?}"
//         );
//         let next = events.next();
//         assert!(
//             matches!(next, Some(Event::End(TagEnd::Image))),
//             "Expected End Image got {next:#?}"
//         );
//         let next = events.next();
//         assert!(
//             matches!(next, Some(Event::End(TagEnd::Link))),
//             "Expected End Link got {next:#?}"
//         );
//         assert_eq!(events.next(), Some(Event::Text("\n\n".into())));
//         assert_eq!(events.next(), None);
//     }
// }
