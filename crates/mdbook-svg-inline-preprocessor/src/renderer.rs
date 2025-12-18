use std::path::Path;

use anyhow::Result;
use pulldown_cmark::{Event, LinkType, Tag, TagEnd};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use crate::svg_inline::format_for_inline;
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

            let file_name = block.svg_file_name(relative_id.as_deref());
            let graph_name = block.graph_name().unwrap_or_default();
            let output_path = block.chapter_path().join(&file_name);

            // if we need the file, write it out
            if self.link_to_file() || self.output_to_file() {
                let mut file = File::create(output_path).await?;
                file.write_all(source.as_bytes()).await?;
            };

            tab_content_nodes.push_str(&format!(
                r##"<div id="{html_id}" class="{TAB_CONTENT_CLASS} mdbook-graphviz-output">"##
            ));

            if self.output_to_file() {
                //
                //
                //
                //
                //
                //
                //
                //
                //
                // TODO image doesn't have an id, if a href is there
                //
                // also should we move that special class to the graphiz package?
                // mdbook-graphviz-output
                //
                //
                //
                //
                //
                //
                //
                //
                //
                if self.link_to_file() {
                    tab_content_nodes.push_str(&format!(
                        "<a href=\"{file_name}\" title=\"{graph_name}\" target=\"_blank\">"
                    ));
                }

                tab_content_nodes.push_str(&format!(
                    r##"<img id="{html_id}" src="{file_name}" alt="{graph_name}" title="{graph_name}">"##
                ));

                if self.link_to_file() {
                    tab_content_nodes.push_str("</a>");
                }
            } else {
                // wrap our SVG in a div to give us a good shadow dom start point
                tab_content_nodes.push_str("<div>");
                tab_content_nodes.push_str(&format_for_inline(
                    &source,
                    &block.svg_id_prefix(relative_id.as_deref()),
                ));
                tab_content_nodes.push_str("</div>");

                // Append our link at the bottom to not impact interactivity
                if self.link_to_file() {
                    tab_content_nodes.push_str(&format!(
                        "<a href=\"{file_name}\" title=\"{graph_name}\" target=\"_blank\">Download{}</a>",
                        if !graph_name.trim().is_empty() {
                            format!(": {}", graph_name)
                        } else {
                            "".to_string()
                        }
                    ));
                }
            }

            tab_content_nodes.push_str("</div>");
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
                // TODO support linking to file
                nodes.push(Event::Html(
                    format_for_inline(&source, &block.svg_id_prefix(relative_id.as_deref())).into(),
                ));
            }
        }
        nodes.push(Event::Text("\n\n".into()));

        Ok(nodes)
    }

    #[allow(async_fn_in_trait)]
    async fn render_svgs(&self, block: &SvgBlock) -> Result<Vec<SvgOutput>>;
}

#[derive(Clone)]
pub struct SvgOutput {
    pub relative_id: Option<String>,
    pub title: String,
    pub source: String,
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
    use std::path::PathBuf;

    use pulldown_cmark::CowStr;
    use scraper::{Html, Selector};

    use crate::preprocessor::SvgBlockBuilder;
    use crate::SvgRendererSharedConfig;

    use super::*;

    #[tokio::test]
    async fn html_inline_events_single() {
        let renderer = TestRenderer {
            config: SvgRendererSharedConfig {
                renderer: "html".into(),
                ..Default::default()
            },
            num_blocks: 1,
        };

        // check that the html value is as expected
        let html = render_html(test_block(), renderer).await;
        let mut element = html.root_element();
        for expected in [".svg-container", "div > div", "#svg-content-svg_0-0"] {
            element = element
                .select(&Selector::parse(expected).unwrap())
                .next()
                .expect(&format!("Expected \"{expected}\" in {}", element.html()));
        }

        assert!(element.attr("class").unwrap().contains("svg-content"));
        assert!(element
            .attr("class")
            .unwrap()
            .contains("mdbook-graphviz-output"));
    }

    #[tokio::test]
    async fn html_file_events_single() {
        let renderer = TestRenderer {
            config: SvgRendererSharedConfig {
                renderer: "html".into(),
                output_to_file: true,
                ..Default::default()
            },
            num_blocks: 1,
        };

        // check that the html value is as expected
        let html = render_html(test_block(), renderer).await;
        let mut element = html.root_element();
        for expected in [".svg-container", "div > div", "#svg-content-svg_0-0"] {
            element = element
                .select(&Selector::parse(expected).unwrap())
                .next()
                .expect(&format!("Expected \"{expected}\" in {}", element.html()));
        }

        assert!(element.attr("class").unwrap().contains("svg-content"));
        assert!(element
            .attr("class")
            .unwrap()
            .contains("mdbook-graphviz-output"));
    }

    #[tokio::test]
    async fn html_events_multiple() {
        let renderer = TestRenderer {
            config: SvgRendererSharedConfig {
                renderer: "html".into(),
                ..Default::default()
            },
            num_blocks: 2,
        };

        // check that the html value is as expected
        let html = render_html(test_block(), renderer).await;
        let mut element = html.root_element();
        for expected in [".svg-container", "div > div"] {
            element = element
                .select(&Selector::parse(expected).unwrap())
                .next()
                .expect(&format!("Expected \"{expected}\" in {}", element.html()));
        }

        // check that for each graph we have the correct headers
        let headers_selector = Selector::parse("ul > li > a").unwrap();
        let mut headers = element.select(&headers_selector);
        // check that for each graph we have the correct classes
        for block_id in ["#svg-content-svg_0-0", "#svg-content-svg_0-1"] {
            let header = headers.next().unwrap();
            let block = element
                .select(&Selector::parse(block_id).unwrap())
                .next()
                .expect(&format!("Expected \"{block_id}\" in {}", element.html()));

            assert_eq!(header.attr("href").unwrap(), block_id);

            assert!(block.attr("class").unwrap().contains("svg-content"));
            assert!(block
                .attr("class")
                .unwrap()
                .contains("mdbook-graphviz-output"));
        }
    }

    #[tokio::test]
    async fn md_inline_events() {
        let renderer = TestRenderer {
            config: SvgRendererSharedConfig {
                renderer: "other".into(),
                ..Default::default()
            },
            num_blocks: 1,
        };
        let mut events = renderer.render(test_block()).await.unwrap().into_iter();
        assert_eq!(events.next(), Some(Event::Text("\n\n".into())));
        assert_eq!(events.next(), Some(Event::Html("".into())));
        assert_eq!(events.next(), Some(Event::Text("\n\n".into())));
        assert_eq!(events.next(), None);
    }

    #[tokio::test]
    async fn md_file_events() {
        let renderer = TestRenderer {
            config: SvgRendererSharedConfig {
                renderer: "other".into(),
                output_to_file: true,
                ..Default::default()
            },
            num_blocks: 1,
        };
        let mut events = renderer.render(test_block()).await.unwrap().into_iter();
        assert_eq!(events.next(), Some(Event::Text("\n\n".into())));
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
    async fn md_linked_file_events() {
        let renderer = TestRenderer {
            config: SvgRendererSharedConfig {
                renderer: "other".into(),
                output_to_file: true,
                link_to_file: true,
                ..Default::default()
            },
            num_blocks: 1,
        };
        let expected_url = CowStr::from("name_graph_svg_0.generated.svg");
        let mut events = renderer.render(test_block()).await.unwrap().into_iter();
        assert_eq!(events.next(), Some(Event::Text("\n\n".into())));
        let next = events.next();
        assert!(
            matches!(
                next,
                Some(Event::Start(Tag::Link {
                    ref dest_url,
                    ..
                })) if *dest_url == expected_url
            ),
            "Expected Link got {next:#?}"
        );
        let next = events.next();
        assert!(
            matches!(
                next,
                Some(Event::Start(Tag::Image {
                    ref dest_url,
                    ..
                })) if *dest_url == expected_url
            ),
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
            "Expected End Image got {next:#?}"
        );
        assert_eq!(events.next(), Some(Event::Text("\n\n".into())));
        assert_eq!(events.next(), None);
    }

    async fn render_html<R: SvgRenderer>(block: SvgBlock, renderer: R) -> Html {
        let mut events = renderer.render(block).await.unwrap().into_iter();
        assert_eq!(events.next(), Some(Event::Text("\n\n".into())));
        let html = if let Some(Event::Html(html)) = events.next() {
            println!("{html:?}");
            Html::parse_fragment(&html)
        } else {
            panic!("Unexpected next event")
        };
        assert_eq!(events.next(), Some(Event::Text("\n\n".into())));
        assert_eq!(events.next(), None);

        assert_eq!(html.errors.len(), 0);

        html
    }

    fn test_block() -> SvgBlock {
        SvgBlockBuilder::new(
            "Name".into(),
            PathBuf::from("test-output"),
            PathBuf::from("chapter.md"),
            "svg".into(),
            Some("graph".into()),
            0,
        )
        .build(0)
    }

    pub struct TestRenderer {
        pub config: SvgRendererSharedConfig,
        pub num_blocks: usize,
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
            let mut blocks = Vec::with_capacity(self.num_blocks);
            for i in 0..self.num_blocks {
                blocks.push(SvgOutput {
                    relative_id: Some(format!("{i}")),
                    title: format!("Test {i}"),
                    source: block.source_code().to_string(),
                });
            }

            Ok(blocks)
        }
    }
}
