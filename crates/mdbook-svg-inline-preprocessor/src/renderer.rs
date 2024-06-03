use std::path::Path;

use anyhow::Result;
use lazy_static::lazy_static;
use pulldown_cmark::{Event, LinkType, Tag, TagEnd};
use regex::Regex;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use crate::SvgBlock;

pub(crate) const D2_CONTAINER_CLASS: &str = "svg-container";
const TAB_HEADER_ID_PREFIX: &str = "svg-tabs";
pub(crate) const TAB_CONTENT_CLASS: &str = "svg-content";

pub trait SvgRenderer {
    fn info_string(&self) -> &str;

    fn renderer(&self) -> &str;

    fn copy_js(&self) -> Option<&Path>;

    fn copy_css(&self) -> Option<&Path>;

    fn output_to_file(&self) -> bool;

    fn link_to_file(&self) -> bool;

    #[allow(async_fn_in_trait)]
    async fn render(&self, block: SvgBlock) -> Result<Vec<Event<'_>>> {
        if self.renderer() == "html" {
            // assume that only the HTML renderer can handle the js/css rendering
            self.render_html(block).await
        } else {
            self.render_md(block).await
        }
    }

    #[allow(async_fn_in_trait)]
    async fn render_html(&self, block: SvgBlock) -> Result<Vec<Event<'_>>> {
        let graph_uid = block.uid_for_chapter();
        let svg_contents = self.render_svgs(&block).await?;

        // Set up our header nodes to only exist if we have more than one diagram
        let mut tab_header_nodes = (svg_contents.len() > 1).then(String::new);
        tab_header_nodes.iter_mut().for_each(|h| {
            h.push_str(&format!(
                "<ul id=\"{}\">",
                sanitize_html_id(&format!("{TAB_HEADER_ID_PREFIX}-{graph_uid}",))
            ));
        });

        let mut tab_content_nodes = String::new();

        let mut first = true;
        for SvgOutput {
            relative_id,
            title,
            source,
        } in svg_contents
        {
            let html_id = sanitize_html_id(&format!(
                "{TAB_CONTENT_CLASS}-{graph_uid}{}",
                relative_id
                    .as_deref()
                    .map(|id| format!("-{id}"))
                    .unwrap_or_default()
            ));

            tab_header_nodes.iter_mut().for_each(|h| {
                h.push_str(&format!(
                    "<li><a {} href=\"#{html_id}\">{}</a></li>",
                    if first { "data-tabby-default" } else { "" },
                    title
                ));

                first = false;
            });

            if self.output_to_file() {
                let file_name = block.svg_file_name(relative_id.as_deref());
                let graph_name = block.graph_name().unwrap_or_default();
                let output_path = block.chapter_path().join(&file_name);

                let mut file = File::create(output_path).await?;
                file.write_all(source.as_bytes()).await?;

                if self.link_to_file() {
                    tab_content_nodes
                        .push_str(&format!("<a href=\"{file_name}\" title=\"{graph_name}\">"));
                }

                tab_content_nodes.push_str(&format!(
                    "<img src=\"{file_name}\" alt=\"{graph_name}\" title=\"{graph_name}\">"
                ));

                if self.link_to_file() {
                    tab_content_nodes.push_str("</a>");
                }
            } else {
                // TODO support linking to file
                tab_content_nodes.push_str(&format!(
                    r##"<div id="{html_id}" class="{TAB_CONTENT_CLASS} mdbook-graphviz-output">{}</div>"##,
                    format_for_inline(source)
                ));
            }
        }

        tab_header_nodes.iter_mut().for_each(|h| {
            h.push_str("</ul>");
        });

        Ok({
            let mut result = vec![];
            result.push(Event::Text("\n\n".into()));
            result.push(Event::Html(
                format!(
                    r#"<div class="{D2_CONTAINER_CLASS}"><div>{}{tab_content_nodes}</div></div>"#,
                    tab_header_nodes.unwrap_or_default(),
                )
                .into(),
            ));
            result.push(Event::Text("\n\n".into()));

            result
        })
    }

    #[allow(async_fn_in_trait)]
    async fn render_md(&self, block: SvgBlock) -> Result<Vec<Event<'_>>> {
        let svg_contents = self.render_svgs(&block).await?;
        let mut nodes = vec![];

        nodes.push(Event::Text("\n\n".into()));
        for SvgOutput {
            relative_id,
            title,
            source,
        } in svg_contents
        {
            if self.output_to_file() {
                let file_name = block.svg_file_name(relative_id.as_deref());
                let output_path = block.chapter_path().join(&file_name);

                let mut file = File::create(output_path).await?;
                file.write_all(source.as_bytes()).await?;

                if self.link_to_file() {
                    let link_tag = Tag::Link {
                        link_type: LinkType::Inline,
                        dest_url: file_name.clone().into(),
                        title: title.clone().into(),
                        id: "".into(),
                    };
                    nodes.push(Event::Start(link_tag));
                }

                let image_tag = Tag::Image {
                    link_type: LinkType::Inline,
                    dest_url: file_name.into(),
                    title: title.into(),
                    id: "".into(),
                };

                nodes.extend([Event::Start(image_tag), Event::End(TagEnd::Image)]);

                if self.link_to_file() {
                    nodes.push(Event::End(TagEnd::Link));
                }
            } else {
                nodes.push(Event::Html(format_for_inline(source).into()));
            }
        }
        nodes.push(Event::Text("\n\n".into()));

        Ok(nodes)
    }

    #[allow(async_fn_in_trait)]
    async fn render_svgs(&self, block: &SvgBlock) -> Result<Vec<SvgOutput>>;
}

pub struct SvgOutput {
    pub relative_id: Option<String>,
    pub title: String,
    pub source: String,
}

fn format_for_inline(output: String) -> String {
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

    output.to_string()
}

fn sanitize_html_id(id: &str) -> String {
    // only pass through valid chars
    id.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            '.' => '-',
            _ => '_',
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use crate::preprocessor::SvgBlockBuilder;
    use crate::SvgRendererSharedConfig;
    use std::path::PathBuf;

    #[tokio::test]
    async fn html_events() {
        let block = SvgBlockBuilder::new(
            "Name".into(),
            PathBuf::from("book"),
            PathBuf::from("chapter"),
            "svg".into(),
            Some("graph".into()),
            0,
        )
        .build(0);

        let renderer = TestRenderer {
            config: SvgRendererSharedConfig::default(),
        };
        let mut events = renderer.render(block).await.unwrap().into_iter();
        assert_eq!(events.next(), Some(Event::Text("\n\n".into())));
        if let Some(Event::Html(_)) = events.next() {
        } else {
            panic!("Unexpected next event")
        }
        assert_eq!(events.next(), Some(Event::Text("\n\n".into())));
        assert_eq!(events.next(), None);
    }

    // #[tokio::test]
    // async fn file_events() {
    //     let code = r#"digraph Test { a -> b }"#;
    //
    //     let block = GraphvizBlock {
    //         graph_name: "Name".into(),
    //         code: code.into(),
    //         chapter_name: "".into(),
    //         chapter_path: "test-output".into(),
    //         index: 0,
    //     };
    //
    //     let config = GraphvizConfig::default();
    //     let mut events = CLIGraphvizToFile::render_graphviz(block, &config)
    //         .await
    //         .expect("Expect rendering to succeed")
    //         .into_iter();
    //     let next = events.next();
    //     assert!(
    //         matches!(next, Some(Event::Start(Tag::Image { .. }))),
    //         "Expected Image got {next:#?}"
    //     );
    //     let next = events.next();
    //     assert!(
    //         matches!(next, Some(Event::End(TagEnd::Image))),
    //         "Expected End Image got {next:#?}"
    //     );
    //     assert_eq!(events.next(), Some(Event::Text("\n\n".into())));
    //     assert_eq!(events.next(), None);
    // }

    pub struct TestRenderer {
        pub config: SvgRendererSharedConfig,
    }

    impl SvgRenderer for TestRenderer {
        fn info_string(&self) -> &str {
            &self.config.info_string
        }

        fn renderer(&self) -> &str {
            &self.config.renderer
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
            Ok(vec![SvgOutput {
                relative_id: None,
                title: "Test".to_string(),
                source: block.source_code().to_string(),
            }])
        }
    }
}
