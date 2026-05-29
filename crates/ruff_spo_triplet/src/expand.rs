//! The expander — the single deterministic projection from [`ModelGraph`]
//! IR to a sorted, de-duplicated `Vec<Triple>`.
//!
//! This is the whole point of the crate: one function, called by every
//! language frontend, so the SPO graph is identical regardless of source
//! language. Determinism is structural — output is sorted by `(s, p, o)`
//! and de-duplicated, so two runs over the same IR are byte-identical.

use std::collections::BTreeSet;

use crate::ir::ModelGraph;
use crate::triple::{EntityKind, Predicate, Provenance, Triple};

/// Expand a [`ModelGraph`] into canonical SPO triples.
///
/// # Emission rules (per model)
///
/// 1. `(ns:model, rdf:type, ogit:ObjectType)` — Structural.
/// 2. For each field: `(ns:model.field, rdf:type, ogit:Property)` — Structural.
/// 3. For each function:
///    - `(ns:model.fn, rdf:type, ogit:Function)` — Structural.
///    - `(ns:model, has_function, ns:model.fn)` — Structural.
/// 4. For each field with `emitted_by`:
///    `(ns:model.field, emitted_by, ns:model.fn)` — Authoritative.
/// 5. For each field dependency:
///    `(ns:model.field, depends_on, ns:model.<dep>)` — Authoritative.
/// 6. For each function read:
///    `(ns:model.fn, reads_field, ns:model.field)` — Inferred.
/// 7. For each function raise:
///    `(ns:model.fn, raises, exc:<Type>)` — Authoritative.
/// 8. For each function traversal:
///    `(ns:model.fn, traverses_relation, ns:model.<rel>)` — Inferred.
///
/// `depends_on` and `traverses_relation` objects are emitted verbatim as
/// dotted paths under the model IRI — the [`crate`]-downstream link-chain
/// splitter (`lance_graph::graph::spo::link_chain`) decomposes them into
/// per-hop link triples; this crate does NOT pre-split them, keeping the
/// emitter source-faithful.
///
/// # Determinism
///
/// The returned Vec is sorted by `(s, p, o)` and de-duplicated. Truth
/// values do not participate in ordering or de-duplication — if the same
/// `(s, p, o)` is produced twice with different provenance, the
/// first-in-sort-order (which, after sort, is deterministic but provenance-
/// arbitrary) wins. Frontends should not emit conflicting provenance for
/// one identity; [`crate::ndjson`] round-trips assume a clean IR.
#[must_use]
pub fn expand(graph: &ModelGraph) -> Vec<Triple> {
    let ns = &graph.namespace;
    let mut set: BTreeSet<(String, String, String)> = BTreeSet::new();
    let mut triples: Vec<Triple> = Vec::new();

    let push = |triples: &mut Vec<Triple>,
                set: &mut BTreeSet<(String, String, String)>,
                s: String,
                p: Predicate,
                o: String,
                prov: Provenance| {
        let key = (s.clone(), p.as_str().to_string(), o.clone());
        if set.insert(key) {
            triples.push(Triple::new(s, p, o, prov));
        }
    };

    for model in &graph.models {
        let model_iri = format!("{ns}:{}", model.name);

        // 1. model rdf:type ObjectType
        push(
            &mut triples,
            &mut set,
            model_iri.clone(),
            Predicate::RdfType,
            EntityKind::ObjectType.iri().to_string(),
            Provenance::Structural,
        );

        // 2. fields
        for field in &model.fields {
            let field_iri = format!("{model_iri}.{}", field.name);
            push(
                &mut triples,
                &mut set,
                field_iri.clone(),
                Predicate::RdfType,
                EntityKind::Property.iri().to_string(),
                Provenance::Structural,
            );

            // 4. emitted_by
            if let Some(fn_name) = &field.emitted_by {
                push(
                    &mut triples,
                    &mut set,
                    field_iri.clone(),
                    Predicate::EmittedBy,
                    format!("{model_iri}.{fn_name}"),
                    Provenance::Authoritative,
                );
            }

            // 5. depends_on (dotted path, source-faithful)
            for dep in &field.depends_on {
                push(
                    &mut triples,
                    &mut set,
                    field_iri.clone(),
                    Predicate::DependsOn,
                    format!("{model_iri}.{dep}"),
                    Provenance::Authoritative,
                );
            }
        }

        // 3 + 6 + 7 + 8. functions
        for func in &model.functions {
            let fn_iri = format!("{model_iri}.{}", func.name);
            push(
                &mut triples,
                &mut set,
                fn_iri.clone(),
                Predicate::RdfType,
                EntityKind::Function.iri().to_string(),
                Provenance::Structural,
            );
            push(
                &mut triples,
                &mut set,
                model_iri.clone(),
                Predicate::HasFunction,
                fn_iri.clone(),
                Provenance::Structural,
            );

            for read in &func.reads {
                push(
                    &mut triples,
                    &mut set,
                    fn_iri.clone(),
                    Predicate::ReadsField,
                    format!("{model_iri}.{read}"),
                    Provenance::Inferred,
                );
            }
            for exc in &func.raises {
                push(
                    &mut triples,
                    &mut set,
                    fn_iri.clone(),
                    Predicate::Raises,
                    format!("exc:{exc}"),
                    Provenance::Authoritative,
                );
            }
            for rel in &func.traverses {
                push(
                    &mut triples,
                    &mut set,
                    fn_iri.clone(),
                    Predicate::TraversesRelation,
                    format!("{model_iri}.{rel}"),
                    Provenance::Inferred,
                );
            }
        }
    }

    triples.sort_by(|a, b| a.key().cmp(&b.key()));
    triples
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Field, Function, Model};

    /// A minimal account_move-shaped graph mirroring the Odoo fixture used
    /// in `lance_graph::graph::spo::action_emitter`.
    fn fixture() -> ModelGraph {
        ModelGraph {
            namespace: "odoo".to_string(),
            models: vec![Model {
                name: "account_move".to_string(),
                fields: vec![Field {
                    name: "amount_total".to_string(),
                    depends_on: vec!["line_ids.balance".to_string()],
                    emitted_by: Some("_compute_amount".to_string()),
                }],
                functions: vec![Function {
                    name: "_compute_amount".to_string(),
                    reads: vec!["currency_id".to_string()],
                    raises: vec!["UserError".to_string()],
                    traverses: vec!["line_ids".to_string()],
                }],
            }],
        }
    }

    #[test]
    fn expands_all_predicate_classes() {
        let triples = expand(&fixture());
        let has =
            |s: &str, p: &str, o: &str| triples.iter().any(|t| t.s == s && t.p == p && t.o == o);

        assert!(has("odoo:account_move", "rdf:type", "ogit:ObjectType"));
        assert!(has(
            "odoo:account_move.amount_total",
            "rdf:type",
            "ogit:Property"
        ));
        assert!(has(
            "odoo:account_move._compute_amount",
            "rdf:type",
            "ogit:Function"
        ));
        assert!(has(
            "odoo:account_move",
            "has_function",
            "odoo:account_move._compute_amount"
        ));
        assert!(has(
            "odoo:account_move.amount_total",
            "emitted_by",
            "odoo:account_move._compute_amount"
        ));
        assert!(has(
            "odoo:account_move.amount_total",
            "depends_on",
            "odoo:account_move.line_ids.balance"
        ));
        assert!(has(
            "odoo:account_move._compute_amount",
            "reads_field",
            "odoo:account_move.currency_id"
        ));
        assert!(has(
            "odoo:account_move._compute_amount",
            "raises",
            "exc:UserError"
        ));
        assert!(has(
            "odoo:account_move._compute_amount",
            "traverses_relation",
            "odoo:account_move.line_ids"
        ));
    }

    #[test]
    fn output_is_sorted_and_deterministic() {
        let a = expand(&fixture());
        let b = expand(&fixture());
        assert_eq!(a, b, "expansion must be deterministic");
        for w in a.windows(2) {
            assert!(w[0].key() <= w[1].key(), "triples not sorted by (s,p,o)");
        }
    }

    #[test]
    fn duplicate_edges_collapse() {
        let mut g = fixture();
        // Push a duplicate depends_on.
        g.models[0].fields[0]
            .depends_on
            .push("line_ids.balance".to_string());
        let triples = expand(&g);
        let count = triples
            .iter()
            .filter(|t| t.p == "depends_on" && t.o == "odoo:account_move.line_ids.balance")
            .count();
        assert_eq!(count, 1, "duplicate depends_on must collapse");
    }

    #[test]
    fn truth_tiers_are_assigned_per_predicate() {
        let triples = expand(&fixture());
        let truth = |p: &str, o: &str| {
            triples
                .iter()
                .find(|t| t.p == p && t.o == o)
                .map(|t| (t.f, t.c))
        };
        // Structural
        assert_eq!(truth("rdf:type", "ogit:ObjectType"), Some((1.0, 1.0)));
        // Authoritative
        assert_eq!(
            truth("emitted_by", "odoo:account_move._compute_amount"),
            Some((0.95, 0.90))
        );
        // Inferred
        assert_eq!(
            truth("reads_field", "odoo:account_move.currency_id"),
            Some((0.85, 0.75))
        );
    }

    #[test]
    fn empty_graph_yields_no_triples() {
        let g = ModelGraph::new("openproject");
        assert!(expand(&g).is_empty());
    }
}
