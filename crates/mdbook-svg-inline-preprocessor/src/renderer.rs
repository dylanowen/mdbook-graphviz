use std::path::Path;

use anyhow::Result;
use lazy_static::lazy_static;
use pulldown_cmark::Event;
use regex::Regex;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use crate::SvgBlock;

const D2_CONTAINER_CLASS: &str = "svg-container";
const TAB_HEADER_ID_PREFIX: &str = "svg-tabs";
const TAB_CONTENT_CLASS: &str = "svg-content";

pub trait SvgRenderer {
    fn info_string(&self) -> &str;

    fn copy_js(&self) -> Option<&Path>;

    fn copy_css(&self) -> Option<&Path>;

    fn output_to_file(&self) -> bool;

    fn link_to_file(&self) -> bool;

    #[allow(async_fn_in_trait)]
    async fn render(&self, block: SvgBlock) -> Result<Vec<Event<'_>>> {
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
                    "<div id=\"{html_id}\" class=\"{TAB_CONTENT_CLASS} mdbook-graphviz-output\">{}</div>",
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
                    r#"<div class="{D2_CONTAINER_CLASS}"><div>
                         {}{tab_content_nodes}
                       </div></div>"#,
                    tab_header_nodes.unwrap_or_default(),
                )
                .into(),
            ));
            result.push(Event::Text("\n\n".into()));

            result
        })
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
