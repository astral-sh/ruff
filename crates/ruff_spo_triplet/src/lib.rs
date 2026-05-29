//! `ruff_spo_triplet` — language-agnostic SPO triplet expansion.
//!
//! # What this crate is
//!
//! A tiny, zero-logic-duplication core that turns a neutral
//! [`ModelGraph`] intermediate representation into deterministic
//! NARS-weighted SPO triples ([`Triple`]), serialised as ndjson that loads
//! directly into the `lance_graph` SPO store.
//!
//! It exists so that **business logic extracted from different source
//! languages produces the same ontology graph**. The Python/Odoo frontend
//! (`ruff_python_dto_check`) and a future Ruby/Rails frontend (OpenProject)
//! both:
//!
//! 1. parse their own AST,
//! 2. fill a [`ModelGraph`] (the only language-specific work), and
//! 3. call [`expand`] + [`ndjson::to_ndjson`].
//!
//! The triple vocabulary, the provenance/truth calibration, and the IRI
//! shape live here once. A new language is a new frontend, not a new
//! ontology.
//!
//! ```text
//!   Python AST ─┐
//!               ├─► ModelGraph (IR) ─► expand() ─► Vec<Triple> ─► ndjson ─► SPO store
//!   Ruby AST  ──┘        ▲                  ▲            ▲
//!                   language-specific   THIS CRATE   THIS CRATE
//! ```
//!
//! # The triple schema
//!
//! | predicate            | subject            | object             | provenance     |
//! | ---                  | ---                | ---                | ---            |
//! | `rdf:type`           | `ns:model`         | `ogit:ObjectType`  | Structural     |
//! | `rdf:type`           | `ns:model.field`   | `ogit:Property`    | Structural     |
//! | `rdf:type`           | `ns:model.fn`      | `ogit:Function`    | Structural     |
//! | `has_function`       | `ns:model`         | `ns:model.fn`      | Structural     |
//! | `emitted_by`         | `ns:model.field`   | `ns:model.fn`      | Authoritative  |
//! | `depends_on`         | `ns:model.field`   | `ns:model.<dep>`   | Authoritative  |
//! | `reads_field`        | `ns:model.fn`      | `ns:model.field`   | Inferred       |
//! | `raises`             | `ns:model.fn`      | `exc:<Type>`       | Authoritative  |
//! | `traverses_relation` | `ns:model.fn`      | `ns:model.<rel>`   | Inferred       |
//!
//! See `SPO_TRIPLET_EXTRACTION.md` (this crate's root) for the full
//! methodology, the "a + b → c through d" query it enables, and the
//! step-by-step guide to writing a new language frontend (incl. the
//! OpenProject Ruby/Rails mapping).
//!
//! # Why these and not RDF libraries
//!
//! The vocabulary is closed and tiny. A full RDF/OWL stack would add
//! hundreds of dependencies to express seven predicates. This crate is
//! `serde` + `serde_json` only — the same zero-dep ethos as
//! `lance_graph_contract`.

mod expand;
mod ir;
mod ndjson;
mod triple;

pub use expand::expand;
pub use ir::{Field, Function, Model, ModelGraph};
pub use ndjson::{ParseError, from_ndjson, to_ndjson};
pub use triple::{EntityKind, Predicate, Provenance, Triple};

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// End-to-end: build a two-model graph, expand, serialise, parse back.
    #[test]
    fn two_model_graph_round_trips_through_ndjson() {
        let mut graph = ModelGraph::new("openproject");
        graph.models.push(Model {
            name: "WorkPackage".to_string(),
            fields: vec![Field {
                name: "total_hours".to_string(),
                depends_on: vec!["time_entries.hours".to_string()],
                emitted_by: Some("compute_total_hours".to_string()),
            }],
            functions: vec![Function {
                name: "compute_total_hours".to_string(),
                reads: vec!["status".to_string()],
                raises: vec!["ActiveRecord::RecordInvalid".to_string()],
                traverses: vec!["time_entries".to_string()],
            }],
        });
        graph.models.push(Model::new("Project"));

        let triples = expand(&graph);
        let text = to_ndjson(&triples);
        let parsed = from_ndjson(&text).expect("round-trips");
        assert_eq!(parsed, triples);

        // The Ruby exception namespacing survives.
        assert!(
            triples
                .iter()
                .any(|t| t.p == "raises" && t.o == "exc:ActiveRecord::RecordInvalid")
        );
        // Both models classified.
        assert_eq!(
            triples
                .iter()
                .filter(|t| t.p == "rdf:type" && t.o == "ogit:ObjectType")
                .count(),
            2
        );
    }
}
