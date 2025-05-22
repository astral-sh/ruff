//! This module implements the [`JsonSchema`] trait from the `schemars` crate for
//! [`TextSize`] and [`TextRange`] if the `schemars` feature is enabled. This trait
//! exposes meta-information on how a given type is serialized and deserialized
//! using `serde`, and is currently used to generate autocomplete information
//! for the `rome.json` configuration file and TypeScript types for the node.js
//! bindings to the Workspace API

use crate::{TextRange, TextSize};
use schemars::{JsonSchema, r#gen::SchemaGenerator, schema::Schema};

impl JsonSchema for TextSize {
    fn schema_name() -> String {
        String::from("TextSize")
    }

    fn json_schema(r#gen: &mut SchemaGenerator) -> Schema {
        // TextSize is represented as a raw u32, see serde_impls.rs for the
        // actual implementation
        <u32>::json_schema(r#gen)
    }
}

impl JsonSchema for TextRange {
    fn schema_name() -> String {
        String::from("TextRange")
    }

    fn json_schema(r#gen: &mut SchemaGenerator) -> Schema {
        // TextSize is represented as (TextSize, TextSize), see serde_impls.rs
        // for the actual implementation
        <(TextSize, TextSize)>::json_schema(r#gen)
    }
}
