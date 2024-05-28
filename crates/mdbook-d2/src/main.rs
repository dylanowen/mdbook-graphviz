use mdbook::errors::Result;
use mdbook::preprocess::PreprocessorContext;

use mdbook_svg_inline_preprocessor::{run_preprocessor, SvgPreprocessor, SvgRendererSharedConfig};

use crate::renderer::D2Renderer;

mod d2_sys;
mod renderer;

const PREPROCESSOR_NAME: &str = "d2-interactive";
const DEFAULT_INFO_STRING_PREFIX: &str = "d2";

fn main() {
    run_preprocessor(&D2Preprocessor);
}

pub struct D2Preprocessor;

impl SvgPreprocessor for D2Preprocessor {
    type Renderer = D2Renderer;

    fn name(&self) -> &str {
        PREPROCESSOR_NAME
    }

    fn default_info_string(&self) -> &str {
        DEFAULT_INFO_STRING_PREFIX
    }

    fn build_renderer(
        &self,
        _ctx: &PreprocessorContext,
        shared_config: SvgRendererSharedConfig,
    ) -> Result<Self::Renderer> {
        Ok(D2Renderer::new(shared_config))
    }
}
