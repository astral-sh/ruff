//! `ruff_cpp_codegen` — stage 2 of the C++ -> Rust AST-DLL pipeline.
//!
//! Stage 1 (`ruff_spo_triplet::reassemble`) turns the harvested SPO triples back
//! into a [`ruff_spo_triplet::ModelGraph`]. This crate is stage 2: it
//! [`project`]s that graph's C++ method plane into per-class [`ClassManifest`]s
//! and [`render`]s them as Rust source that targets the OGAR Core
//! (`lance_graph_contract::codegen_manifest::MethodSig`).
//!
//! ```text
//!   C++ corpus --(ruff_cpp_spo, libclang)--> triples (ndjson)
//!     --(ruff_spo_triplet::from_ndjson + reassemble)--> ModelGraph
//!     --(THIS CRATE: project)--> Vec<ClassManifest>
//!     --(THIS CRATE: render)--> Rust source (MethodSig manifests)
//! ```
//!
//! # Placement + dependency boundary
//!
//! This crate depends on `ruff_spo_triplet` ONLY. The rendered text *names*
//! `lance_graph_contract` types as strings; the crate never compiles against
//! lance-graph (the forbidden `ruff -> lance-graph` edge). The generated source
//! is type-checked downstream, in the consumer repo (tesseract-rs), after the
//! `MethodSig` EXTEND-CORE lands and against a leptonica build env this checkout
//! lacks.
//!
//! # The gate (teeth, not a self-golden)
//!
//! [`decompile`] regenerates the signature-plane triples a manifest encodes;
//! the round-trip `decompile(project(g))` must equal `expand(g)` restricted to
//! the signature plane. That is the `codegen_spine::roundtrip_eq` pattern over
//! the *live harvested* triples (see `manifest::tests`): a manifest that drops a
//! method or mangles a parameter fails it. A green render run is NOT byte-parity
//! with libtesseract (that is the operator-gated `PROBE-OGAR-ADAPTER-UNICHARSET`);
//! every emitted file says so via its `PARITY: UNRUN` marker.

mod manifest;
mod render;

pub use manifest::{ClassManifest, MethodSig, decompile, is_signature_plane, project};
pub use render::render;

#[cfg(test)]
mod integration_tests {
    use super::*;
    use ruff_spo_triplet::{CppMethod, Model, ModelGraph, Triple, expand};
    use std::collections::BTreeSet;

    fn graph() -> ModelGraph {
        let mut m = Model::new("tesseract::UNICHARSET");
        m.methods.push(CppMethod {
            name: "unichar_to_id".to_string(),
            return_type: Some("UNICHAR_ID".to_string()),
            param_types: vec!["const char *".to_string()],
            is_const: true,
            ..Default::default()
        });
        m.methods.push(CppMethod {
            name: "size".to_string(),
            return_type: Some("int".to_string()),
            param_types: Vec::new(),
            is_const: true,
            ..Default::default()
        });
        ModelGraph {
            namespace: "cpp".to_string(),
            models: vec![m],
        }
    }

    /// End-to-end: harvest graph -> project -> render produces a manifest naming
    /// the Core type, carrying the PARITY marker, one literal per method.
    #[test]
    fn project_then_render_emits_a_core_manifest() {
        let g = graph();
        let manifests = project(&g);
        let src = render(&manifests);
        assert!(src.contains("use lance_graph_contract::codegen_manifest::MethodSig;"));
        assert!(src.contains("PARITY: UNRUN"));
        assert_eq!(src.matches("MethodSig {").count(), 2);
    }

    /// End-to-end gate: the projected manifest's signature plane round-trips
    /// against the live harvested triples.
    #[test]
    fn project_then_decompile_roundtrips_against_expand() {
        let g = graph();
        let spo = |ts: &[Triple]| -> BTreeSet<(String, String, String)> {
            ts.iter()
                .map(|t| (t.s.clone(), t.p.clone(), t.o.clone()))
                .collect()
        };
        let from_manifest = spo(&decompile(&project(&g), &g.namespace));
        let from_expand: BTreeSet<_> = expand(&g)
            .into_iter()
            .filter(is_signature_plane)
            .map(|t| (t.s, t.p, t.o))
            .collect();
        assert_eq!(from_manifest, from_expand);
    }
}
