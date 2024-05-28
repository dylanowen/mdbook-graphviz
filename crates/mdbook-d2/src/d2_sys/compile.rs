use std::cmp::Ordering;
use std::ffi::c_char;
use std::fmt::{Debug, Formatter};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::d2_sys::{null_to_default, unwrap_result, D2Error, GoString};

extern "C" {
    fn Compile(content: GoString) -> *const c_char;
}

#[allow(dead_code)]
pub fn compile(content: &str) -> Result<Graph, D2Error> {
    let raw_graph = unwrap_result(unsafe { Compile(content.into()) })?;

    Ok(serde_json::from_str(&raw_graph).with_context(|| "Failed to parse Graph")?)
}

#[derive(Deserialize, Eq, PartialEq, Debug)]
pub struct Graph {
    pub name: String,

    #[serde(alias = "isFolderOnly")]
    pub is_folder_only: bool,

    pub root: Option<Object>,
    #[serde(deserialize_with = "null_to_default")]
    pub edges: Vec<Edge>,
    #[serde(deserialize_with = "null_to_default")]
    pub objects: Vec<Object>,

    pub layers: Option<Vec<Graph>>,
    pub scenarios: Option<Vec<Graph>>,
    pub steps: Option<Vec<Graph>>,

    #[serde(alias = "rootLevel", default)]
    pub root_level: usize,
}

#[derive(Deserialize, Eq, PartialEq, Debug)]
pub struct Object {
    pub id: String,
    pub id_val: String,

    pub attributes: Attributes,
}

#[derive(Deserialize, Eq, PartialEq, Debug)]
pub struct Attributes {
    pub label: Scalar,
}

#[derive(Deserialize, Eq, PartialEq, Debug)]
pub struct Scalar {
    pub value: String,
}

#[derive(Deserialize, Eq, PartialEq, Debug)]
pub struct Edge {
    pub index: usize,
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct GraphPath(Vec<GraphPathComponent>);

impl Debug for GraphPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.0.iter().fold(String::new(), |acc, c| {
                format!(
                    "{acc}{}{c:?}",
                    if acc.is_empty() || matches!(c, GraphPathComponent::Index { .. }) {
                        ""
                    } else {
                        "."
                    },
                )
            })
        )
    }
}

impl From<Vec<GraphPathComponent>> for GraphPath {
    fn from(components: Vec<GraphPathComponent>) -> Self {
        GraphPath(components)
    }
}

#[derive(Clone, Eq, PartialEq, Hash)]
pub enum GraphPathComponent {
    Index { index: usize },
    Layers,
    Scenarios,
    Steps,
}

impl GraphPathComponent {
    fn enum_index(&self) -> usize {
        match self {
            GraphPathComponent::Index { .. } => 0,
            GraphPathComponent::Layers { .. } => 1,
            GraphPathComponent::Scenarios { .. } => 2,
            GraphPathComponent::Steps { .. } => 3,
        }
    }
}

impl Ord for GraphPathComponent {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (
                GraphPathComponent::Index {
                    index: self_index, ..
                },
                GraphPathComponent::Index {
                    index: other_index, ..
                },
            ) => self_index.cmp(other_index),
            (_, _) => self.enum_index().cmp(&other.enum_index()),
        }
    }
}

impl PartialOrd for GraphPathComponent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Debug for GraphPathComponent {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphPathComponent::Index { index } => write!(f, "[{index}]"),
            GraphPathComponent::Layers => write!(f, "layers"),
            GraphPathComponent::Scenarios => write!(f, "scenarios"),
            GraphPathComponent::Steps => write!(f, "steps"),
        }
    }
}
