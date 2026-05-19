//! Bundle data model — serialized JSON shape matches
//! `AdaWorldAPI/woa-rs:rfcs/v02-005-bundle-schema.md` v1.

use serde::Serialize;

use crate::{HARVESTER_VERSION, SCHEMA_VERSION};

#[derive(Debug, Serialize)]
pub struct Bundle {
    pub schema_version: u32,
    pub harvester: Harvester,

    pub endpoint: String,
    pub path: String,
    pub methods: Vec<String>,
    pub function: String,
    pub family: String,
    pub action: String,
    pub source: Source,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub complexity_score: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_loc: Option<u32>,

    pub body: String,
    pub body_sha256: String,

    pub decorators: Vec<Decorator>,
}

#[derive(Debug, Serialize)]
pub struct Harvester {
    pub name: &'static str,
    pub version: &'static str,
    pub schema_version: u32,
}

impl Harvester {
    pub fn new() -> Self {
        Self {
            name: "ruff_python_dto_check",
            version: HARVESTER_VERSION,
            schema_version: SCHEMA_VERSION,
        }
    }
}

impl Default for Harvester {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Serialize)]
pub struct Source {
    pub file: String,
    pub line_start: u32,
    pub line_end: u32,
    pub blueprint: String,
}

#[derive(Debug, Serialize)]
pub struct Decorator {
    pub raw: String,
    pub kind: DecoratorKind,
}

#[derive(Debug, Serialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum DecoratorKind {
    Route,
    Auth,
    Scope,
    ModuleRequired,
    Other,
}
