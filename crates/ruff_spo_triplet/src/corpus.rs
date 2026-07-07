//! The ndjson grouper — turns a flat `Vec<`[`Triple`]`>` stream back into
//! per-method [`Function`] fact-sets, the shape [`crate::recipe::classify`]
//! consumes.
//!
//! A harvester emits one triple per fact, subject-keyed by the method IRI
//! (`ns:Class.method`). This module is the inverse projection: it walks the
//! flat stream once and buckets `writes_field` / `reads_field` /
//! `writes_if_blank` / `calls` / `raises` triples by subject, so a corpus
//! runner can classify every method the harvest saw — including methods
//! whose body carries no facts at all (read via `has_function`), so the
//! Observe/Empty tail is counted rather than silently dropped.

use std::collections::BTreeMap;

use crate::ir::{Field, Function, Model, ModelGraph};
use crate::triple::{Predicate, Triple};

/// Group a flat SPO triple stream into per-method [`Function`] fact-sets,
/// keyed by the triple's subject IRI (the method).
///
/// A subject is included when it has at least one body fact (`writes_field`
/// / `reads_field` / `writes_if_blank` / `calls` / `raises` as subject) OR
/// appears as the **object** of a `has_function` triple — i.e. every method
/// the corpus declares, even ones whose body carries no facts at all. That
/// keeps the denominator honest: a read-only or empty method still shows up
/// (as `Observe`/`Empty`) instead of vanishing from the corpus.
///
/// `writes_if_blank` objects are folded into **both** `guarded_writes` and
/// `writes` — a harvester may emit the guard fact without also emitting a
/// paired `writes_field` for the same field, and [`crate::recipe::classify`]
/// relies on the invariant `guarded_writes ⊆ writes`.
///
/// The return order is sorted by method name (a [`BTreeMap`] walk), so a
/// corpus run is deterministic across invocations.
#[must_use]
pub fn group_functions(triples: &[Triple]) -> Vec<Function> {
    let mut methods: BTreeMap<String, Function> = BTreeMap::new();

    for t in triples {
        let Some(pred) = Predicate::from_str(&t.p) else {
            // Unknown predicates cannot happen once a triple has passed
            // `from_ndjson`'s closed-vocab gate, but a caller may hand this
            // function a triple set assembled some other way.
            continue;
        };
        match pred {
            Predicate::HasFunction => {
                // Existence-only: register the method even if no body
                // fact ever names it as subject.
                method_entry(&mut methods, &t.o);
            }
            Predicate::WritesField => {
                push_unique(&mut method_entry(&mut methods, &t.s).writes, &t.o);
            }
            Predicate::ReadsField => {
                push_unique(&mut method_entry(&mut methods, &t.s).reads, &t.o);
            }
            Predicate::WritesIfBlank => {
                let f = method_entry(&mut methods, &t.s);
                push_unique(&mut f.guarded_writes, &t.o);
                push_unique(&mut f.writes, &t.o);
            }
            Predicate::Calls => {
                push_unique(&mut method_entry(&mut methods, &t.s).calls, &t.o);
            }
            Predicate::Raises => {
                push_unique(&mut method_entry(&mut methods, &t.s).raises, &t.o);
            }
            Predicate::EmittedBy => {
                // Odoo's declarative write signal, INVERTED: the triple is
                // `(field, emitted_by, method)` — field as subject, method as
                // object — because the fact comes from the field declaration
                // (`compute='_compute_x'`), not from a body walk. Fold it
                // into `writes` exactly like a body-observed store: without
                // this arm, an Odoo corpus (whose harvest predates the
                // ruff #51 body-write DTO arm, e.g. odoo-rs slice_2 with 388
                // `emitted_by` rows and ZERO `writes_field`) regroups with
                // `Function.writes == []` and `recipe::classify` misreads
                // every compute as Guard/Observe/Empty.
                push_unique(&mut method_entry(&mut methods, &t.o).writes, &t.s);
            }
            _ => {}
        }
    }

    methods.into_values().collect()
}

/// Fetch (or create) the [`Function`] entry for `name`, seeding its `name`
/// field on first insertion.
fn method_entry<'a>(methods: &'a mut BTreeMap<String, Function>, name: &str) -> &'a mut Function {
    methods.entry(name.to_string()).or_insert_with(|| Function {
        name: name.to_string(),
        ..Function::default()
    })
}

/// Append `item` to `v` unless it is already present.
fn push_unique(v: &mut Vec<String>, item: &str) {
    if !v.iter().any(|existing| existing == item) {
        v.push(item.to_string());
    }
}

/// Reassemble a flat SPO triple stream into a [`ModelGraph`] on the
/// **core-7 plane** (`Model::fields` / `Model::functions`) — the shape a
/// "class + typed fields + methods" frontend (C#/Roslyn, Java, …) emits,
/// and the shape [`crate::mint`]-adjacent consumers
/// (`ogar-from-ruff::compile_graph_csharp`) walk.
///
/// Distinct from [`crate::reassemble::reassemble`], which recovers the
/// **C++ sibling-collection plane** (`bases` / `member_fields` / `methods` /
/// …) instead. The two reassemblies are complementary projections of the
/// same triple stream, not exclusive — a caller that needs the C++-style
/// method-signature facts (`is_static` / `has_param_type` / `has_visibility`
/// / `returns_type` / `is_const`) should reach for
/// [`crate::reassemble::reassemble`] on the same triples; this function does
/// not recover them, because [`Field`] / [`Function`] carry no slot for a
/// method's signature — only its body facts.
///
/// # Predicates consumed
///
/// - `(class, rdf:type, ogit:ObjectType)` — class anchor. Seeds one
///   [`Model`] per distinct subject (first-wins: a duplicate anchor triple
///   never resets an already-seeded model).
/// - `(class, has_field, class.field)` — field ownership; seeds a
///   bare-named [`Field`] on the owning [`Model`] (class-prefix stripped).
/// - `(class.field, field_type, "<type>")` — fills in the already-seeded
///   field's [`Field::field_type`].
/// - `(class, has_function, class.method)` — method ownership; used to
///   attribute [`group_functions`]'s output (keyed by the full method IRI)
///   onto the right [`Model`], with the class-prefix stripped to recover the
///   bare method name.
/// - the five body-fact predicates `group_functions` already groups
///   (`writes_field` / `reads_field` / `writes_if_blank` / `calls` /
///   `raises`) — delegated, never re-implemented here.
/// - `(class, inherits_from, base)` — appended to [`Model::inherits`]
///   (the frontend-agnostic parent list), namespace-stripped.
///
/// A method or field with no `has_function` / `has_field` owner (should not
/// happen for a well-formed harvest — every body-fact subject is expected to
/// also appear as a `has_function` object) is silently dropped rather than
/// attributed to the wrong class or attached to a phantom model.
#[must_use]
pub fn reassemble_model_graph(triples: &[Triple], namespace: &str) -> ModelGraph {
    let ns_prefix = format!("{namespace}:");

    // Pass 1 — class anchors. First-wins so a duplicate anchor triple
    // doesn't reset an already-populated accumulator.
    let mut classes: BTreeMap<String, Model> = BTreeMap::new();
    for t in triples {
        if t.p == "rdf:type" && t.o == "ogit:ObjectType" {
            let name = t.s.strip_prefix(&ns_prefix).unwrap_or(&t.s).to_string();
            classes
                .entry(t.s.clone())
                .or_insert_with(|| Model::new(name));
        }
    }

    // Pass 2 — has_field seeds a bare-named Field on the owning class and
    // records field IRI -> owning class IRI (field_type's subject is the
    // field, not the class, so it needs the reverse lookup). field_type then
    // fills in the field it names.
    //
    // First-wins on the field IRI (`field_owner`'s key): a harvester may
    // legitimately repeat a `(class, has_field, class.field)` line (measured
    // on a real corpus — 121 duplicate pairs in one harvest), and pushing
    // one `Field` per triple would silently inflate the field count past
    // the true distinct-field one. Mirrors the class-anchor dedup above
    // and `group_functions`' de-duplication of body facts.
    let mut field_owner: BTreeMap<String, String> = BTreeMap::new();
    for t in triples {
        if t.p == "has_field" && classes.contains_key(&t.s) && !field_owner.contains_key(&t.o) {
            field_owner.insert(t.o.clone(), t.s.clone());
            if let Some(model) = classes.get_mut(&t.s) {
                let prefix = format!("{}.", t.s);
                let name = t.o.strip_prefix(&prefix).unwrap_or(&t.o).to_string();
                model.fields.push(Field {
                    name,
                    ..Field::default()
                });
            }
        }
    }
    for t in triples {
        if t.p == "field_type"
            && let Some(class_iri) = field_owner.get(&t.s)
            && let Some(model) = classes.get_mut(class_iri)
        {
            let prefix = format!("{class_iri}.");
            let name = t.s.strip_prefix(&prefix).unwrap_or(&t.s).to_string();
            if let Some(field) = model.fields.iter_mut().find(|f| f.name == name) {
                field.field_type = Some(t.o.clone());
            }
        }
    }

    // Pass 3 — has_function seeds method ownership; group_functions does the
    // body-fact grouping (never duplicated here). Each grouped Function is
    // keyed by the full method IRI; route it onto its owning Model with the
    // class-prefix stripped to the bare method name.
    let mut method_owner: BTreeMap<String, String> = BTreeMap::new();
    for t in triples {
        if t.p == "has_function" && classes.contains_key(&t.s) {
            method_owner.insert(t.o.clone(), t.s.clone());
        }
    }
    for mut f in group_functions(triples) {
        if let Some(class_iri) = method_owner.get(&f.name)
            && let Some(model) = classes.get_mut(class_iri)
        {
            let prefix = format!("{class_iri}.");
            f.name = f.name.strip_prefix(&prefix).unwrap_or(&f.name).to_string();
            model.functions.push(f);
        }
    }

    // Pass 4 — inherits_from, namespace-stripped (mirrors the raw
    // parent-name convention `Model::inherits` already uses for Odoo
    // `_inherit`). `push_unique` guards the same duplicate-triple pattern
    // as Pass 2 (6 duplicate pairs measured on the same real corpus).
    for t in triples {
        if t.p == "inherits_from"
            && let Some(model) = classes.get_mut(&t.s)
        {
            let name = t.o.strip_prefix(&ns_prefix).unwrap_or(&t.o).to_string();
            push_unique(&mut model.inherits, &name);
        }
    }

    ModelGraph {
        namespace: namespace.to_string(),
        models: classes.into_values().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str, p: Predicate, o: &str) -> Triple {
        Triple::new(s, p, o, p.default_provenance())
    }

    /// The Odoo declarative-write arm: `(field, emitted_by, method)` is
    /// INVERTED (field subject, method object) and must fold into the
    /// method's `writes` — without it, an emitted_by-style corpus (e.g.
    /// odoo-rs slice_2: 388 emitted_by rows, zero writes_field) regroups
    /// write-blind and `recipe::classify` misreads every compute as
    /// Guard/Observe/Empty.
    #[test]
    fn emitted_by_folds_inverted_into_writes_and_classifies_compute() {
        let triples = vec![
            t(
                "odoo:account_move",
                Predicate::HasFunction,
                "odoo:account_move._compute_amount",
            ),
            t(
                "odoo:account_move.amount_total",
                Predicate::EmittedBy,
                "odoo:account_move._compute_amount",
            ),
            t(
                "odoo:account_move._compute_amount",
                Predicate::ReadsField,
                "odoo:account_move.line_ids",
            ),
        ];
        let fns = group_functions(&triples);
        assert_eq!(fns.len(), 1);
        let f = &fns[0];
        assert_eq!(f.name, "odoo:account_move._compute_amount");
        assert_eq!(f.writes, vec!["odoo:account_move.amount_total"]);
        assert!(f.guarded_writes.is_empty(), "declarative write is not a J1 guard");
        // The whole point: the fresh write (W ⊄ R) classifies as Compute.
        assert_eq!(crate::recipe::classify(f), crate::recipe::RecipeCentroid::Compute);
    }

    #[test]
    fn groups_body_facts_by_subject() {
        let triples = vec![
            t(
                "csharp:Cipher",
                Predicate::HasFunction,
                "csharp:Cipher.ValueToDB",
            ),
            t(
                "csharp:Cipher.ValueToDB",
                Predicate::WritesField,
                "csharp:Cipher.result",
            ),
            t(
                "csharp:Cipher.ValueToDB",
                Predicate::ReadsField,
                "csharp:Cipher.input",
            ),
        ];
        let functions = group_functions(&triples);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "csharp:Cipher.ValueToDB");
        assert_eq!(functions[0].writes, vec!["csharp:Cipher.result"]);
        assert_eq!(functions[0].reads, vec!["csharp:Cipher.input"]);
    }

    /// A method declared via `has_function` but with no body facts at all
    /// still shows up (as `Empty` once classified) — the honest
    /// denominator.
    #[test]
    fn declared_only_method_is_included() {
        let triples = vec![t(
            "csharp:Cipher",
            Predicate::HasFunction,
            "csharp:Cipher.Sum",
        )];
        let functions = group_functions(&triples);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "csharp:Cipher.Sum");
        assert!(functions[0].writes.is_empty());
        assert!(functions[0].reads.is_empty());
    }

    /// `writes_if_blank` alone (no paired `writes_field` line) still
    /// populates `writes`, preserving the `guarded_writes ⊆ writes`
    /// invariant `classify` relies on.
    #[test]
    fn writes_if_blank_backfills_writes() {
        let triples = vec![t(
            "csharp:Widget.SetDefaults",
            Predicate::WritesIfBlank,
            "csharp:Widget.Name",
        )];
        let functions = group_functions(&triples);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].guarded_writes, vec!["csharp:Widget.Name"]);
        assert_eq!(functions[0].writes, vec!["csharp:Widget.Name"]);
    }

    /// Duplicate triples for the same fact don't duplicate the fact-set
    /// entry (a harvester or store may legitimately repeat a line).
    #[test]
    fn duplicate_facts_are_deduplicated() {
        let triples = vec![
            t(
                "csharp:Cipher.ValueToDB",
                Predicate::WritesField,
                "csharp:Cipher.result",
            ),
            t(
                "csharp:Cipher.ValueToDB",
                Predicate::WritesField,
                "csharp:Cipher.result",
            ),
        ];
        let functions = group_functions(&triples);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].writes, vec!["csharp:Cipher.result"]);
    }

    /// Predicates outside the body-fact set (e.g. `rdf:type`) don't create
    /// a spurious method entry.
    #[test]
    fn structural_predicates_are_ignored() {
        let triples = vec![t("csharp:Cipher", Predicate::RdfType, "ogit:ObjectType")];
        assert!(group_functions(&triples).is_empty());
    }

    // ───── reassemble_model_graph ─────
    //
    // Agnostic Invoice/Order business shapes — mirroring the closed C#/Roslyn
    // harvester predicate set (`rdf:type` / `has_field` / `field_type` /
    // `has_function` / `inherits_from` + the five body-fact predicates),
    // never a real consumer's model names.

    /// A one-class harvest exercising every predicate `reassemble_model_graph`
    /// consumes: a typed field, a base class, and a method with read/write/
    /// call body facts.
    #[test]
    fn reassembles_core7_fields_and_functions_from_a_csharp_shaped_harvest() {
        let triples = vec![
            t("biz:Invoice", Predicate::RdfType, "ogit:ObjectType"),
            t("biz:Invoice", Predicate::InheritsFrom, "biz:DbBase"),
            t("biz:Invoice", Predicate::HasField, "biz:Invoice.Number"),
            t("biz:Invoice.Number", Predicate::FieldType, "string"),
            t(
                "biz:Invoice",
                Predicate::HasFunction,
                "biz:Invoice.MarkPaid",
            ),
            t(
                "biz:Invoice.MarkPaid",
                Predicate::WritesField,
                "biz:Invoice.Status",
            ),
            t(
                "biz:Invoice.MarkPaid",
                Predicate::ReadsField,
                "biz:Invoice.Total",
            ),
            t("biz:Invoice.MarkPaid", Predicate::Calls, "this.Save"),
        ];
        let graph = reassemble_model_graph(&triples, "biz");
        assert_eq!(graph.namespace, "biz");
        assert_eq!(graph.models.len(), 1);
        let invoice = &graph.models[0];
        assert_eq!(invoice.name, "Invoice");
        assert_eq!(invoice.inherits, vec!["DbBase".to_string()]);

        assert_eq!(invoice.fields.len(), 1);
        assert_eq!(invoice.fields[0].name, "Number");
        assert_eq!(invoice.fields[0].field_type.as_deref(), Some("string"));

        assert_eq!(invoice.functions.len(), 1);
        let mark_paid = &invoice.functions[0];
        assert_eq!(
            mark_paid.name, "MarkPaid",
            "bare method name, class-prefix stripped"
        );
        assert_eq!(mark_paid.writes, vec!["biz:Invoice.Status".to_string()]);
        assert_eq!(mark_paid.reads, vec!["biz:Invoice.Total".to_string()]);
        assert_eq!(mark_paid.calls, vec!["this.Save".to_string()]);
    }

    /// A method declared via `has_function` with no body facts at all still
    /// reassembles as an (empty) `Function` on its owning `Model` — the
    /// same "honest denominator" `group_functions` already guarantees.
    #[test]
    fn reassembles_declared_only_method_as_empty_function() {
        let triples = vec![
            t("biz:Order", Predicate::RdfType, "ogit:ObjectType"),
            t("biz:Order", Predicate::HasFunction, "biz:Order.Sum"),
        ];
        let graph = reassemble_model_graph(&triples, "biz");
        assert_eq!(graph.models.len(), 1);
        assert_eq!(graph.models[0].functions.len(), 1);
        let sum = &graph.models[0].functions[0];
        assert_eq!(sum.name, "Sum");
        assert!(sum.reads.is_empty());
        assert!(sum.writes.is_empty());
    }

    /// Two classes each declaring their own field and method — no
    /// cross-attribution, mirroring the anchor-first attribution
    /// [`crate::reassemble::reassemble`] proves for the C++ plane.
    #[test]
    fn two_classes_stay_distinct_no_cross_attribution() {
        let triples = vec![
            t("biz:Invoice", Predicate::RdfType, "ogit:ObjectType"),
            t("biz:Invoice", Predicate::HasField, "biz:Invoice.Number"),
            t("biz:Invoice.Number", Predicate::FieldType, "string"),
            t(
                "biz:Invoice",
                Predicate::HasFunction,
                "biz:Invoice.ComputeTotal",
            ),
            t(
                "biz:Invoice.ComputeTotal",
                Predicate::WritesField,
                "biz:Invoice.Total",
            ),
            t("biz:Order", Predicate::RdfType, "ogit:ObjectType"),
            t("biz:Order", Predicate::HasField, "biz:Order.Number"),
            t("biz:Order.Number", Predicate::FieldType, "int"),
            t(
                "biz:Order",
                Predicate::HasFunction,
                "biz:Order.ComputeTotal",
            ),
            t(
                "biz:Order.ComputeTotal",
                Predicate::WritesField,
                "biz:Order.GrandTotal",
            ),
        ];
        let graph = reassemble_model_graph(&triples, "biz");
        assert_eq!(graph.models.len(), 2);
        let invoice = graph.models.iter().find(|m| m.name == "Invoice").unwrap();
        let order = graph.models.iter().find(|m| m.name == "Order").unwrap();

        assert_eq!(invoice.fields.len(), 1);
        assert_eq!(invoice.fields[0].field_type.as_deref(), Some("string"));
        assert_eq!(order.fields.len(), 1);
        assert_eq!(order.fields[0].field_type.as_deref(), Some("int"));

        assert_eq!(invoice.functions.len(), 1);
        assert_eq!(invoice.functions[0].name, "ComputeTotal");
        assert_eq!(
            invoice.functions[0].writes,
            vec!["biz:Invoice.Total".to_string()]
        );
        assert_eq!(order.functions.len(), 1);
        assert_eq!(order.functions[0].name, "ComputeTotal");
        assert_eq!(
            order.functions[0].writes,
            vec!["biz:Order.GrandTotal".to_string()]
        );
    }

    /// The C++-machine-plane method-signature predicates
    /// (`is_static` / `has_param_type` / `has_visibility` / `returns_type`)
    /// carry no slot on the core-7 [`Function`] — `reassemble_model_graph`
    /// must silently ignore them rather than fabricate a field or crash.
    #[test]
    fn signature_plane_predicates_are_silently_ignored() {
        let triples = vec![
            t("biz:Invoice", Predicate::RdfType, "ogit:ObjectType"),
            t("biz:Invoice", Predicate::HasFunction, "biz:Invoice.Helper"),
            t("biz:Invoice.Helper", Predicate::IsStatic, "true"),
            t("biz:Invoice.Helper", Predicate::HasParamType, "0:int"),
            t("biz:Invoice.Helper", Predicate::HasVisibility, "private"),
            t("biz:Invoice.Helper", Predicate::ReturnsType, "int"),
        ];
        let graph = reassemble_model_graph(&triples, "biz");
        assert_eq!(graph.models.len(), 1);
        assert_eq!(graph.models[0].functions.len(), 1);
        assert_eq!(graph.models[0].functions[0].name, "Helper");
        assert!(graph.models[0].fields.is_empty());
    }

    #[test]
    fn reassemble_model_graph_empty_triples_yield_empty_graph() {
        let graph = reassemble_model_graph(&[], "biz");
        assert!(graph.models.is_empty());
        assert_eq!(graph.namespace, "biz");
    }

    #[test]
    fn reassemble_model_graph_is_deterministic() {
        let triples = vec![
            t("biz:Invoice", Predicate::RdfType, "ogit:ObjectType"),
            t("biz:Invoice", Predicate::HasField, "biz:Invoice.Number"),
            t("biz:Invoice.Number", Predicate::FieldType, "string"),
        ];
        assert_eq!(
            reassemble_model_graph(&triples, "biz"),
            reassemble_model_graph(&triples, "biz")
        );
    }

    /// A harvester may legitimately repeat a `(class, has_field, field)` or
    /// `(class, inherits_from, base)` line (measured on a real C# harvest:
    /// 121 and 6 duplicate pairs respectively). Neither must inflate the
    /// reassembled counts past the true distinct set — this is the
    /// regression the raw-corpus run surfaced (fields: 6395 raw triples but
    /// only 6272 distinct pairs).
    #[test]
    fn duplicate_has_field_and_inherits_from_triples_do_not_inflate_counts() {
        let triples = vec![
            t("biz:Invoice", Predicate::RdfType, "ogit:ObjectType"),
            t("biz:Invoice", Predicate::HasField, "biz:Invoice.Number"),
            t("biz:Invoice", Predicate::HasField, "biz:Invoice.Number"),
            t("biz:Invoice.Number", Predicate::FieldType, "string"),
            t("biz:Invoice", Predicate::InheritsFrom, "biz:DbBase"),
            t("biz:Invoice", Predicate::InheritsFrom, "biz:DbBase"),
        ];
        let graph = reassemble_model_graph(&triples, "biz");
        assert_eq!(graph.models.len(), 1);
        let invoice = &graph.models[0];
        assert_eq!(
            invoice.fields.len(),
            1,
            "duplicate has_field must not duplicate the Field"
        );
        assert_eq!(invoice.fields[0].field_type.as_deref(), Some("string"));
        assert_eq!(
            invoice.inherits,
            vec!["DbBase".to_string()],
            "duplicate inherits_from must not duplicate the parent"
        );
    }
}
