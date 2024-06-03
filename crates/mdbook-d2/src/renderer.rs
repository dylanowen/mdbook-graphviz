use std::cmp::Ordering;
use std::path::Path;

use anyhow::anyhow;
use mdbook::errors::Result;

use mdbook_svg_inline_preprocessor::{SvgBlock, SvgOutput, SvgRenderer, SvgRendererSharedConfig};

use crate::d2_sys;
use crate::d2_sys::{D2Error, GraphPath, GraphPathComponent, RenderResult};

pub struct D2Renderer {
    config: SvgRendererSharedConfig,
}

impl D2Renderer {
    pub fn new(config: SvgRendererSharedConfig) -> Self {
        Self { config }
    }
}

impl SvgRenderer for D2Renderer {
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
        let diagram_result =
            d2_sys::render(block.source_code()).map_err(|render_error| match render_error {
                D2Error::Parse(parse_error) => {
                    let parse_errors = parse_error
                        .errors
                        .into_iter()
                        .map(|error| {
                            format!(
                                "{}: D2 {}",
                                // D2 errors are 0 indexed
                                block.location_string(error.start_line(), error.end_line()),
                                error.message
                            )
                        })
                        .fold(String::new(), |acc, e| format!("{}\n{}", acc, e));

                    anyhow!("Parse Error{parse_errors}")
                }
                e => e.into(),
            })?;

        let mut diagrams = D2Result::from_render(&diagram_result);
        diagrams.sort();

        Ok(diagrams
            .into_iter()
            .map(|diagram| SvgOutput {
                relative_id: Some(diagram.relative_id()),
                title: diagram.title(),
                source: diagram.content().to_string(),
            })
            .collect())
    }
}

#[derive(Eq)]
struct D2Result<'a> {
    result: &'a RenderResult,
    path: GraphPath,
}

impl<'a> D2Result<'a> {
    fn from_render(result: &'a RenderResult) -> Vec<Self> {
        fn inner<'i>(
            result: &'i RenderResult,
            path: &mut Vec<GraphPathComponent>,
        ) -> Vec<D2Result<'i>> {
            let mut results = vec![];

            if !result.layers.is_empty() {
                path.push(GraphPathComponent::Layers);
                for (i, layer) in result.layers.iter().enumerate() {
                    path.push(GraphPathComponent::Index { index: i });
                    results.extend(inner(layer, path));
                    path.pop();
                }
                path.pop();
            }
            if !result.scenarios.is_empty() {
                path.push(GraphPathComponent::Scenarios);
                for (i, scenario) in result.scenarios.iter().enumerate() {
                    path.push(GraphPathComponent::Index { index: i });
                    results.extend(inner(scenario, path));
                    path.pop();
                }
                path.pop();
            }
            if !result.steps.is_empty() {
                path.push(GraphPathComponent::Steps);
                for (i, step) in result.steps.iter().enumerate() {
                    path.push(GraphPathComponent::Index { index: i });
                    results.extend(inner(step, path));
                    path.pop();
                }
                path.pop();
            }

            results.push(D2Result {
                result,
                path: path.clone().into(),
            });
            results
        }

        inner(result, &mut vec![])
    }

    fn title(&self) -> String {
        self.result.title()
    }

    fn relative_id(&self) -> String {
        format!("{:?}", self.path)
    }

    fn content(&self) -> &str {
        &self.result.content
    }
}

impl<'a> PartialEq<Self> for D2Result<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.path.eq(&other.path)
    }
}

impl<'a> Ord for D2Result<'a> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.path.cmp(&other.path)
    }
}

impl<'a> PartialOrd for D2Result<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
