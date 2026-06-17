//! Reassembly — the inverse of the C++ machine-plane projection of [`expand`].
//!
//! [`crate::expand`] flattens a [`ModelGraph`] into a sorted, de-duplicated
//! `Vec<Triple>`. This module walks that triple set back into a
//! [`ModelGraph`], recovering the per-class structure (members, methods,
//! bases, templates, friends, macro uses, static asserts) that the AST-DLL
//! codegen consumes as its stage-1 input.
//!
//! It is the first half of the `ruff_cpp_spo` → Rust-adapter generator: the
//! harvester emits ndjson, [`crate::from_ndjson`] parses it back to
//! [`Triple`]s, and [`reassemble`] groups those triples into the typed
//! per-class surface the emitter walks.
//!
//! # Scope
//!
//! This recovers the **C++ machine-plane projection** only — the seven
//! `Cpp*` sibling collections on [`Model`] plus the class identity. The
//! core-7 (`fields` / `functions`) and the `OpenProject` AR-shape collections
//! are intentionally not reconstructed (the codegen target is the C++ adapter
//! surface). Feed it the output of [`crate::expand`] over a C++-plane graph.
//!
//! # The round-trip property (the falsifier)
//!
//! `reassemble(expand(g))` equals `g` *projected to its emitted form* — i.e.
//! with the three fields [`crate::expand`] deliberately drops blanked to
//! their defaults: [`CppField::type_name`] (→ empty), [`CppBase::access`] (→
//! [`CppAccess::Public`]) and [`CppBase::virtual_base`] (→ `false`). The test
//! module asserts this on a fixture exercising every C++ arm, plus the two
//! adversarial cases that make the property a real measurement rather than a
//! tautology: two classes sharing a method name (no cross-attribution) and a
//! single class with overloaded methods (no overload collapse). Because the
//! check compares against the live `g` and not a frozen golden file, it is
//! immune to harvester-vocabulary drift — only a genuine reassembly bug
//! turns it red.
//!
//! # Why method identity comes from `has_param_type`, not the IRI suffix
//!
//! The per-overload method IRI carries a `(<comma-joined-types>)` suffix for
//! disambiguation, but a templated parameter type contains commas
//! (`std::map<int, int>`), so the suffix has no clean `,`-split inverse.
//! Reassembly therefore recovers the ordered parameter list from the
//! index-prefixed `has_param_type` triples (`0:int`, `1:const Image &`),
//! reconstructs the exact suffix from them, and strips that suffix off the
//! IRI to recover the method name. No delimiter inside the suffix is ever
//! guessed.

use std::collections::BTreeMap;

use crate::ir::{
    ConstexprKind, CppAccess, CppBase, CppField, CppFriend, CppMacroUse, CppMethod,
    CppStaticAssert, CppTemplate, CppTemplateKind, Model, ModelGraph,
};
use crate::triple::Triple;

/// Mutable accumulator for one method's properties while the triple set is
/// scanned. Finalised into a [`CppMethod`] once every property triple has
/// been routed to it.
#[derive(Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "accumulator mirroring CppMethod's independent C++ qualifiers \
              (pure-virtual / noexcept / const / static) — any combination is valid"
)]
struct MethodAcc {
    /// `index → parameter type`, from the `<index>:<type>` `has_param_type`
    /// objects. A `BTreeMap` keeps the parameters in signature order.
    params: BTreeMap<usize, String>,
    return_type: Option<String>,
    is_pure_virtual: bool,
    constexpr_kind: Option<ConstexprKind>,
    is_noexcept: bool,
    overrides: Option<String>,
    operator_kind: Option<String>,
    requires_clause: Option<String>,
    is_const: bool,
    is_static: bool,
}

/// Reassemble the C++ machine-plane projection of a triple set into a
/// [`ModelGraph`].
///
/// See the module docs for the scope, the round-trip property, and why
/// method identity is recovered from `has_param_type` rather than the IRI
/// suffix. The returned graph is canonicalised (collections sorted, the
/// three never-emitted fields blanked to their defaults) so it compares
/// equal to `expand`'s emitted projection of the source graph.
#[must_use]
pub fn reassemble(triples: &[Triple]) -> ModelGraph {
    // The namespace is the prefix of any class anchor's subject IRI.
    let namespace = triples
        .iter()
        .find(|t| t.p == "rdf:type" && t.o == "ogit:ObjectType")
        .and_then(|t| t.s.split_once(':'))
        .map_or_else(String::new, |(ns, _)| ns.to_string());
    let ns_prefix = format!("{namespace}:");

    // Pass 1 — class anchors. Identity comes from the explicit
    // `(class, rdf:type, ogit:ObjectType)` triple, never from string-
    // splitting a member IRI (anchor-first attribution).
    let mut classes: BTreeMap<String, Model> = BTreeMap::new();
    for t in triples {
        if t.p == "rdf:type" && t.o == "ogit:ObjectType" {
            let name = t.s.strip_prefix(&ns_prefix).unwrap_or(&t.s).to_string();
            classes
                .entry(t.s.clone())
                .or_insert_with(|| Model::new(name));
        }
    }

    // Pass 2 — the `has_function` edges are the explicit class → method
    // links; they seed the method accumulators and the owner map so every
    // method-property triple routes to the right class without prefix
    // matching.
    let mut method_owner: BTreeMap<String, String> = BTreeMap::new();
    let mut method_acc: BTreeMap<String, MethodAcc> = BTreeMap::new();
    for t in triples {
        if t.p == "has_function" && classes.contains_key(&t.s) {
            method_owner.insert(t.o.clone(), t.s.clone());
            method_acc.entry(t.o.clone()).or_default();
        }
    }

    // Pass 3 — route every remaining triple by its subject: method-property
    // predicates land in the method accumulator; class-level facts land on
    // the owning model.
    for t in triples {
        match t.p.as_str() {
            "has_param_type" => {
                if let Some(acc) = method_acc.get_mut(&t.s)
                    && let Some((idx, ty)) = t.o.split_once(':')
                    && let Ok(i) = idx.parse::<usize>()
                {
                    acc.params.insert(i, ty.to_string());
                }
            }
            "returns_type" => {
                if let Some(acc) = method_acc.get_mut(&t.s) {
                    acc.return_type = Some(t.o.clone());
                }
            }
            "is_pure_virtual" => {
                if let Some(acc) = method_acc.get_mut(&t.s) {
                    acc.is_pure_virtual = true;
                }
            }
            "is_constexpr" => {
                if let Some(acc) = method_acc.get_mut(&t.s) {
                    acc.constexpr_kind = Some(match t.o.as_str() {
                        "consteval" => ConstexprKind::Consteval,
                        _ => ConstexprKind::Constexpr,
                    });
                }
            }
            "is_noexcept" => {
                if let Some(acc) = method_acc.get_mut(&t.s) {
                    acc.is_noexcept = true;
                }
            }
            "virtually_overrides" => {
                if let Some(acc) = method_acc.get_mut(&t.s) {
                    let base = t.o.strip_prefix(&ns_prefix).unwrap_or(&t.o).to_string();
                    acc.overrides = Some(base);
                }
            }
            "defines_operator" => {
                if let Some(acc) = method_acc.get_mut(&t.s) {
                    acc.operator_kind = Some(t.o.clone());
                }
            }
            "requires_concept" => {
                if let Some(acc) = method_acc.get_mut(&t.s) {
                    acc.requires_clause = Some(t.o.clone());
                }
            }
            "is_const" => {
                if let Some(acc) = method_acc.get_mut(&t.s) {
                    acc.is_const = true;
                }
            }
            "is_static" => {
                if let Some(acc) = method_acc.get_mut(&t.s) {
                    acc.is_static = true;
                }
            }
            "has_field" => {
                if let Some(model) = classes.get_mut(&t.s) {
                    let prefix = format!("{}.", t.s);
                    let name = t.o.strip_prefix(&prefix).unwrap_or(&t.o).to_string();
                    model.member_fields.push(CppField {
                        name,
                        type_name: String::new(),
                    });
                }
            }
            "inherits_from" => {
                if let Some(model) = classes.get_mut(&t.s) {
                    let name = t.o.strip_prefix(&ns_prefix).unwrap_or(&t.o).to_string();
                    model.bases.push(CppBase {
                        name,
                        access: CppAccess::Public,
                        virtual_base: false,
                    });
                }
            }
            "template_specialises" => {
                if let Some(model) = classes.get_mut(&t.s) {
                    model.templates.push(CppTemplate {
                        kind: CppTemplateKind::Specialisation,
                        name: t.o.clone(),
                    });
                }
            }
            "template_instantiates" => {
                if let Some(model) = classes.get_mut(&t.s) {
                    model.templates.push(CppTemplate {
                        kind: CppTemplateKind::Instantiation,
                        name: t.o.clone(),
                    });
                }
            }
            "is_friend_of" => {
                if let Some(model) = classes.get_mut(&t.s) {
                    model.friends.push(CppFriend { name: t.o.clone() });
                }
            }
            "uses_macro_expansion" => {
                if let Some(model) = classes.get_mut(&t.s)
                    && let Some((ident, macro_name)) = t.o.split_once("<=")
                {
                    model.macro_uses.push(CppMacroUse {
                        identifier: ident.to_string(),
                        macro_name: macro_name.to_string(),
                    });
                }
            }
            "static_asserts" => {
                if let Some(model) = classes.get_mut(&t.s) {
                    model.static_asserts.push(CppStaticAssert {
                        condition: t.o.clone(),
                    });
                }
            }
            _ => {}
        }
    }

    // Finalise methods. The ordered parameter list comes from the
    // accumulator; the suffix is reconstructed from it and stripped off the
    // IRI to recover the name — never split.
    for (method_iri, acc) in method_acc {
        let Some(class_iri) = method_owner.get(&method_iri) else {
            continue;
        };
        let Some(model) = classes.get_mut(class_iri) else {
            continue;
        };
        let param_types: Vec<String> = acc.params.into_values().collect();
        // Reconstruct the exact suffix `expand` built — including the ` const`
        // cv-qualifier when the method is const — so the prefix/suffix strip
        // recovers the bare name. `is_const` was collected from the property
        // triple in pass 3, so it is available here at finalize.
        let suffix = format!(
            "({}){}",
            param_types.join(","),
            if acc.is_const { " const" } else { "" }
        );
        let class_prefix = format!("{class_iri}.");
        let name = method_iri
            .strip_prefix(&class_prefix)
            .and_then(|rest| rest.strip_suffix(&suffix))
            .unwrap_or(&method_iri)
            .to_string();
        model.methods.push(CppMethod {
            name,
            is_pure_virtual: acc.is_pure_virtual,
            constexpr_kind: acc.constexpr_kind,
            is_noexcept: acc.is_noexcept,
            overrides: acc.overrides,
            operator_kind: acc.operator_kind,
            requires_clause: acc.requires_clause,
            return_type: acc.return_type,
            param_types,
            is_const: acc.is_const,
            is_static: acc.is_static,
        });
    }

    let mut graph = ModelGraph {
        namespace,
        models: classes.into_values().collect(),
    };
    canonicalize_cpp(&mut graph);
    graph
}

/// The *emitted projection* of a graph: the form `reassemble(expand(g))`
/// recovers. Clones `graph`, blanks the three never-emitted C++ fields
/// ([`CppField::type_name`], [`CppBase::access`], [`CppBase::virtual_base`]),
/// and canonically sorts every C++ collection.
///
/// For a C++-plane graph whose method IRIs are all distinct,
/// `reassemble(expand(&g)) == cpp_projection(&g)`. The method IRI is cv-aware
/// (it carries the ` const` qualifier), so a const/non-const overload pair —
/// which shares name AND parameter types — stays on distinct nodes rather than
/// collapsing under the `(s, p, o)` dedup. This is the round-trip
/// identity the AST-DLL generator relies on; it is exposed so a consumer can
/// assert it against its own harvested graph.
#[must_use]
pub fn cpp_projection(graph: &ModelGraph) -> ModelGraph {
    let mut projected = graph.clone();
    canonicalize_cpp(&mut projected);
    projected
}

/// Sort and de-duplicate every C++ collection, and blank the three fields
/// [`crate::expand`] never emits, so a reassembled graph and a source graph's
/// emitted projection compare equal regardless of source declaration order.
///
/// De-duplication mirrors `expand`'s `(s, p, o)` dedup: a source graph can
/// carry the same fact twice (e.g. a template-id instantiated in several method
/// signatures yields several identical `template_instantiates`, or a member
/// harvested twice), but `expand` collapses identical triples, so `reassemble`
/// recovers each fact once. The projection must therefore collapse exact
/// duplicates too — otherwise a benign duplicate would read as a round-trip
/// difference. Real collisions (two entries sharing a sort key but differing in
/// content — the genuine overload-collision residual) are NOT equal, so dedup
/// keeps them and they still surface.
fn canonicalize_cpp(graph: &mut ModelGraph) {
    for model in &mut graph.models {
        for field in &mut model.member_fields {
            field.type_name = String::new();
        }
        for base in &mut model.bases {
            base.access = CppAccess::Public;
            base.virtual_base = false;
        }
        model.member_fields.sort_by(|a, b| a.name.cmp(&b.name));
        model.member_fields.dedup();
        model.bases.sort_by(|a, b| a.name.cmp(&b.name));
        model.bases.dedup();
        // Sort key includes `is_const` — the cv-qualifier is part of the method
        // IRI's identity, so a const/non-const overload pair (same name + params)
        // must sort deterministically (non-const before const) on both the
        // reassembled and the projected side; without it the stable sort
        // preserves two different pre-sort orders and the pair compares unequal.
        model.methods.sort_by(|a, b| {
            (a.name.as_str(), &a.param_types, a.is_const).cmp(&(
                b.name.as_str(),
                &b.param_types,
                b.is_const,
            ))
        });
        model.methods.dedup();
        model.templates.sort_by(|a, b| {
            (a.name.as_str(), tkind_ord(a.kind)).cmp(&(b.name.as_str(), tkind_ord(b.kind)))
        });
        model.templates.dedup();
        model.friends.sort_by(|a, b| a.name.cmp(&b.name));
        model.friends.dedup();
        model.macro_uses.sort_by(|a, b| {
            (a.identifier.as_str(), a.macro_name.as_str())
                .cmp(&(b.identifier.as_str(), b.macro_name.as_str()))
        });
        model.macro_uses.dedup();
        model
            .static_asserts
            .sort_by(|a, b| a.condition.cmp(&b.condition));
        model.static_asserts.dedup();
    }
    graph.models.sort_by(|a, b| a.name.cmp(&b.name));
}

/// Stable ordering key for a template's kind (specialisations before
/// instantiations of the same name).
fn tkind_ord(kind: CppTemplateKind) -> u8 {
    match kind {
        CppTemplateKind::Specialisation => 0,
        CppTemplateKind::Instantiation => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expand::expand;
    use crate::ir::{CppTemplate, CppTemplateKind};

    /// Project a source graph to the form `expand` actually emits — the
    /// public [`cpp_projection`], i.e. the definition of the round-trip
    /// target.
    fn projected(graph: &ModelGraph) -> ModelGraph {
        cpp_projection(graph)
    }

    /// A `Tesseract::Recognizer`-shaped graph exercising every C++ arm —
    /// mirrors the `expand` test fixture so the round-trip covers the whole
    /// C++ surface (bases, fields, every method-property flag, overload-free
    /// methods, templates, friends, macro uses, static asserts).
    fn cpp_fixture() -> ModelGraph {
        let mut rec = Model::new("Tesseract::Recognizer");
        rec.bases.push(CppBase {
            name: "Tesseract::Classify".to_string(),
            access: CppAccess::Public,
            virtual_base: false,
        });
        rec.member_fields.push(CppField {
            name: "recognizer_".to_string(),
            type_name: "std::unique_ptr<LSTMRecognizer>".to_string(),
        });
        rec.methods.push(CppMethod {
            name: "Recognize".to_string(),
            is_pure_virtual: false,
            constexpr_kind: None,
            is_noexcept: true,
            overrides: Some("Tesseract::Classify.Recognize(int,const Image &) const".to_string()),
            operator_kind: None,
            requires_clause: None,
            return_type: Some("int".to_string()),
            param_types: vec!["int".to_string(), "const Image &".to_string()],
            is_const: true,
            is_static: false,
        });
        rec.methods.push(CppMethod {
            name: "Clear".to_string(),
            is_pure_virtual: true,
            constexpr_kind: None,
            is_noexcept: false,
            overrides: None,
            operator_kind: None,
            requires_clause: None,
            return_type: None,
            param_types: Vec::new(),
            is_const: false,
            is_static: false,
        });
        rec.methods.push(CppMethod {
            name: "kMaxRating".to_string(),
            is_pure_virtual: false,
            constexpr_kind: Some(ConstexprKind::Constexpr),
            is_noexcept: false,
            overrides: None,
            operator_kind: None,
            requires_clause: None,
            return_type: None,
            param_types: Vec::new(),
            is_const: false,
            is_static: true,
        });
        rec.methods.push(CppMethod {
            name: "operator==".to_string(),
            is_pure_virtual: false,
            constexpr_kind: None,
            is_noexcept: false,
            overrides: None,
            operator_kind: Some("operator==".to_string()),
            requires_clause: Some("std::equality_comparable<T>".to_string()),
            return_type: None,
            param_types: Vec::new(),
            is_const: false,
            is_static: false,
        });
        rec.templates.push(CppTemplate {
            kind: CppTemplateKind::Specialisation,
            name: "GenericVector<int>".to_string(),
        });
        rec.templates.push(CppTemplate {
            kind: CppTemplateKind::Instantiation,
            name: "GenericVector<float>".to_string(),
        });
        rec.friends.push(CppFriend {
            name: "TessdataManager".to_string(),
        });
        rec.macro_uses.push(CppMacroUse {
            identifier: "BOOL_MEMBER".to_string(),
            macro_name: "INT_MEMBER".to_string(),
        });
        rec.static_asserts.push(CppStaticAssert {
            condition: "sizeof(int) == 4".to_string(),
        });
        ModelGraph {
            namespace: "cpp".to_string(),
            models: vec![rec],
        }
    }

    /// The core falsifier: a full round-trip recovers the emitted projection
    /// of the source graph exactly. Would fail on any mis-attribution,
    /// overload collapse, parameter loss, or name mis-parse.
    #[test]
    fn round_trip_recovers_cpp_emitted_projection() {
        let g = cpp_fixture();
        let got = reassemble(&expand(&g));
        assert_eq!(got, projected(&g));
    }

    /// Two classes that declare an identically-named, identically-signatured
    /// method (the real `UNICHARMAP::unichar_to_id` /
    /// `UNICHARSET::unichar_to_id` collision) must each keep their own
    /// method — no cross-attribution. This is the adversarial case that
    /// proves anchor-first attribution works.
    #[test]
    fn two_classes_same_method_name_stay_distinct() {
        let mut g = ModelGraph::new("cpp");
        for class in ["UNICHARMAP", "UNICHARSET"] {
            let mut m = Model::new(class);
            m.methods.push(CppMethod {
                name: "unichar_to_id".to_string(),
                return_type: Some("UNICHAR_ID".to_string()),
                param_types: vec!["const char *".to_string()],
                is_const: true,
                ..Default::default()
            });
            g.models.push(m);
        }
        let got = reassemble(&expand(&g));
        assert_eq!(got, projected(&g));

        let map = got.models.iter().find(|m| m.name == "UNICHARMAP").unwrap();
        let set = got.models.iter().find(|m| m.name == "UNICHARSET").unwrap();
        assert_eq!(map.methods.len(), 1, "UNICHARMAP keeps its own method");
        assert_eq!(set.methods.len(), 1, "UNICHARSET keeps its own method");
        assert_eq!(map.methods[0].name, "unichar_to_id");
        assert_eq!(set.methods[0].name, "unichar_to_id");
    }

    /// Overloads on one class (the two `UNICHARSET::unichar_to_id` arities)
    /// must reassemble into two distinct methods, never one merged node.
    #[test]
    fn overloads_split_into_distinct_methods() {
        let mut g = ModelGraph::new("cpp");
        let mut m = Model::new("UNICHARSET");
        m.methods.push(CppMethod {
            name: "unichar_to_id".to_string(),
            return_type: Some("UNICHAR_ID".to_string()),
            param_types: vec!["const char *".to_string()],
            is_const: true,
            ..Default::default()
        });
        m.methods.push(CppMethod {
            name: "unichar_to_id".to_string(),
            return_type: Some("UNICHAR_ID".to_string()),
            param_types: vec!["const char *".to_string(), "int".to_string()],
            is_const: true,
            ..Default::default()
        });
        g.models.push(m);

        let got = reassemble(&expand(&g));
        assert_eq!(got, projected(&g));

        let set = &got.models[0];
        assert_eq!(set.methods.len(), 2, "two overloads stay distinct");
        // Sorted by (name, param_types): the 1-arg overload sorts first.
        assert_eq!(set.methods[0].param_types, vec!["const char *".to_string()]);
        assert_eq!(
            set.methods[1].param_types,
            vec!["const char *".to_string(), "int".to_string()]
        );
    }

    /// The GAP-CONST-OVERLOAD fix (D): a const/non-const overload pair sharing
    /// name AND parameter types (`T& at(i)` vs `const T& at(i) const`) must
    /// reassemble into TWO distinct methods. Before the cv-aware method IRI they
    /// collapsed under the `(s, p, o)` dedup (19/67 ccutil classes); the
    /// ` const` qualifier in the IRI keeps them apart, and `reassemble`
    /// reconstructs it from the recovered `is_const`.
    #[test]
    fn const_and_nonconst_overload_stay_distinct() {
        let mut g = ModelGraph::new("cpp");
        let mut m = Model::new("GenericVector");
        m.methods.push(CppMethod {
            name: "at".to_string(),
            return_type: Some("T &".to_string()),
            param_types: vec!["int".to_string()],
            is_const: false,
            ..Default::default()
        });
        m.methods.push(CppMethod {
            name: "at".to_string(),
            return_type: Some("const T &".to_string()),
            param_types: vec!["int".to_string()],
            is_const: true,
            ..Default::default()
        });
        g.models.push(m);

        let got = reassemble(&expand(&g));
        assert_eq!(got, projected(&g));

        let v = &got.models[0];
        assert_eq!(
            v.methods.len(),
            2,
            "const and non-const `at(int)` must stay distinct, not collapse"
        );
        assert!(v.methods.iter().all(|method| method.name == "at"));
        assert!(
            v.methods.iter().any(|method| method.is_const),
            "the const overload survives"
        );
        assert!(
            v.methods.iter().any(|method| !method.is_const),
            "the non-const overload survives"
        );
    }

    /// Parameter types containing internal commas (`std::map<int, int>`)
    /// must round-trip exactly. This is the baton-auditor's P1(a) guard: the
    /// name and params are recovered from the index-prefixed
    /// `has_param_type` triples and the reconstructed suffix, never by
    /// splitting the IRI's `(params)` text on `,`.
    #[test]
    fn params_with_internal_commas_recover_exactly() {
        let mut g = ModelGraph::new("cpp");
        let mut m = Model::new("Cache");
        m.methods.push(CppMethod {
            name: "merge".to_string(),
            return_type: Some("void".to_string()),
            param_types: vec!["std::map<int, int>".to_string(), "int".to_string()],
            ..Default::default()
        });
        g.models.push(m);

        let got = reassemble(&expand(&g));
        assert_eq!(got, projected(&g));

        let method = &got.models[0].methods[0];
        assert_eq!(method.name, "merge", "name survives comma-bearing params");
        assert_eq!(
            method.param_types,
            vec!["std::map<int, int>".to_string(), "int".to_string()],
            "ordered params recover from has_param_type, not a comma split"
        );
    }

    #[test]
    fn empty_triples_yield_empty_graph() {
        let got = reassemble(&[]);
        assert!(got.models.is_empty());
        assert!(got.namespace.is_empty());
    }

    #[test]
    fn reassembly_is_deterministic() {
        let triples = expand(&cpp_fixture());
        assert_eq!(reassemble(&triples), reassemble(&triples));
    }
}
