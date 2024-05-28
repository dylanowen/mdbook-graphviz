use anyhow::{Context, Result};
use serde::Deserialize;
use std::ffi::c_char;

use crate::d2_sys::{null_to_default, unwrap_result, D2Error, GoString, Object};

extern "C" {
    fn Render(content: GoString) -> *const c_char;
}

pub fn render(content: &str) -> Result<RenderResult, D2Error> {
    let raw_result = unwrap_result(unsafe { Render(content.into()) })?;

    Ok(serde_json::from_str(&raw_result).with_context(|| "Failed to parse Graph")?)
}

#[derive(Deserialize, Eq, PartialEq, Debug)]
pub struct RenderResult {
    pub name: String,

    #[serde(alias = "isFolderOnly")]
    pub is_folder_only: bool,
    pub content: String,

    pub root: Option<Object>,

    #[serde(deserialize_with = "null_to_default")]
    pub layers: Vec<RenderResult>,
    #[serde(deserialize_with = "null_to_default")]
    pub scenarios: Vec<RenderResult>,
    #[serde(deserialize_with = "null_to_default")]
    pub steps: Vec<RenderResult>,
}

impl RenderResult {
    pub fn title(&self) -> String {
        self.root
            .as_ref()
            // Try to grab our root attribute as a title
            .map(|root| &root.attributes.label.value)
            .filter(|s| !s.is_empty())
            // Fallback on our graph name
            .or(Some(&self.name))
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            // Fallback on our path
            .unwrap_or_else(|| "index".to_string())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_render() {
        render(
            r#"Chicken's plan: {
  style.font-size: 35
  near: top-center
  shape: text
}

steps: {
  1: {
    Approach road
  }
  2: {
    Approach road -> Cross road
  }
  3: {
    Cross road -> Make you wonder why
  }
}"#,
        )
        .unwrap();
    }
}
