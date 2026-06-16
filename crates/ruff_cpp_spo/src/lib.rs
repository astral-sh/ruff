//! `ruff_cpp_spo` — C++ machine-plane frontend for the shared SPO triplet
//! core.
//!
//! Walks a C++ corpus (Tesseract first; LLVM / Boost / `OpenCV` next) via
//! libclang and produces a [`ModelGraph`] populated with the C++ machine-
//! plane `Declaration` siblings the shared `ruff_spo_triplet` crate expands
//! into the 13 C++ predicates (`inherits_from`, `template_specialises`,
//! `virtually_overrides`, `is_pure_virtual`, …).
//!
//! # The harvester family
//!
//! `ruff_python_dto_check` parses the Python control plane;
//! `ruff_ruby_spo` parses the Ruby class plane; this crate parses the C++
//! machine plane. All three fill the SAME `ruff_spo_triplet::ModelGraph`
//! and call the SAME [`ruff_spo_triplet::expand`], so the downstream SPO
//! graph is identical regardless of source language. A new language is a
//! new frontend, not a new ontology.
//!
//! ```text
//!   C++ corpus (UPSTREAM) ─(libclang)─► CppClass.declarations
//!        ─► ModelGraph (shared IR) ─► expand() ─► Vec<Triple> ─► ndjson
//!        ─► lance-graph SPO store / tesseract-rs-ast-dll-codegen-v1
//! ```
//!
//! # Architecture (mirrors `ruff_ruby_spo`)
//!
//! - [`CppClass`] is the frontend-local discriminated union the parser
//!   emits — one [`Declaration`] per class-body member, in source order.
//! - [`model_from_class`] unpacks each [`Declaration`] into the typed
//!   `Model::{bases, member_fields, methods, templates, friends, …}`
//!   sibling slots the shared IR consumes. **Pure unpacking** — no
//!   semantic transform, no re-parsing.
//! - [`extract`] is the top-level corpus walker. It is a `todo!()` stub
//!   today (see its docs for the libclang wiring contract); the target
//!   triple shape is already locked by [`tests::locked_shape_expands_to_expected_triples`].
//!
//! # Iron rules this frontend respects
//!
//! - **`ruff_spo_triplet` stays serde-only.** The libclang dependency lives
//!   here (behind a `libclang` feature, when wired), never in the shared
//!   core.
//! - **No C++ source vendored into a `*-rs` target.** The corpus stays
//!   upstream; `extract` walks it from a configurable path.
//! - **Closed-vocab gate.** The C++ predicates are in
//!   `ruff_spo_triplet::Predicate` under the `predicate_count_locked_at_47`
//!   gate. A new C++ predicate is a deliberate ontology change there.

use std::path::Path;

use ruff_spo_triplet::{
    CppBase, CppField, CppFriend, CppMacroUse, CppMethod, CppStaticAssert, CppTemplate, Model,
    ModelGraph,
};

/// The namespace prefix for C++ machine-plane subjects/objects.
///
/// `cpp` (the language), not the corpus name — the C++ machine plane is one
/// graph spanning every C++ corpus (Tesseract, LLVM, Boost, …); a class is
/// identified by its fully-qualified name (`Tesseract::Recognizer`), not by
/// the namespace prefix. (This differs deliberately from `ruff_ruby_spo`'s
/// corpus-named `"openproject"`, because that crate is OpenProject-specific
/// whereas this one is the reusable C++ frontend.)
pub const NAMESPACE: &str = "cpp";

/// A minimally-parsed C++ class / struct — what the libclang walker should
/// produce before the IR mapping.
///
/// **Frontend-local IR.** The shared `ruff_spo_triplet::Model` already
/// carries the C++ sibling-shape `Vec<…>` fields per category; this struct
/// is just the in-source-order shape the parser emits *before*
/// [`model_from_class`] unpacks them. It is NOT exposed in any triple — it
/// disappears at the IR boundary.
#[derive(Debug, Clone, Default)]
pub struct CppClass {
    /// Enclosing namespace components, outermost first
    /// (`["Tesseract"]` for `Tesseract::Recognizer`). Empty for a class at
    /// global scope. libclang exposes these as separate cursors; the
    /// qualified name is computed by [`Self::qualified_name`].
    pub namespace: Vec<String>,
    /// Class name as written (`Recognizer`), without namespace qualifiers.
    pub name: String,
    /// Every class-body declaration, captured in source order. The
    /// [`model_from_class`] fn unpacks this into the typed
    /// `Model::{bases, member_fields, methods, …}` sibling fields the
    /// shared IR consumes.
    pub declarations: Vec<Declaration>,
}

impl CppClass {
    /// The fully-qualified name (`Tesseract::Recognizer`) used as the
    /// [`Model::name`]. Joins [`Self::namespace`] components with `::` and
    /// appends [`Self::name`]; returns the bare name at global scope.
    #[must_use]
    pub fn qualified_name(&self) -> String {
        if self.namespace.is_empty() {
            self.name.clone()
        } else {
            format!("{}::{}", self.namespace.join("::"), self.name)
        }
    }
}

/// One class-body declaration, discriminated by category.
///
/// **Frontend-local IR** (mirrors `ruff_ruby_spo::Declaration`): the shared
/// `ruff_spo_triplet::Model` carries the C++ sibling-shape `Vec<…>` fields;
/// this enum is the source-order shape the parser emits before
/// [`model_from_class`] unpacks it. It disappears at the IR boundary —
/// nothing here is serialized into a triple directly.
#[derive(Debug, Clone)]
pub enum Declaration {
    /// `class Derived : public Base` — a base-class declaration.
    Base(CppBase),
    /// A data-member declaration.
    Field(CppField),
    /// A method declaration carrying its C++ property flags
    /// (virtual / override / pure-virtual / constexpr / noexcept /
    /// operator / requires).
    Method(CppMethod),
    /// A template specialisation or instantiation.
    Template(CppTemplate),
    /// A `friend class` / `friend fn` declaration.
    Friend(CppFriend),
    /// An identifier originating from a preprocessor macro expansion.
    MacroUse(CppMacroUse),
    /// A `static_assert` in class scope.
    StaticAssert(CppStaticAssert),
}

/// Top-level entry: walk a C++ corpus and produce the IR.
///
/// **`todo!()` stub — the libclang walker is the next deliverable.** The
/// wiring contract for the session that picks this up:
///
/// 1. Add the `clang` crate under a non-default `libclang` feature (see
///    `Cargo.toml`). The system `libclang.so` is the cost of admission for
///    semantic C++ resolution (templates, preprocessor, ADL) — the only
///    parser family that can satisfy the C++ predicates faithfully.
/// 2. Walk each translation unit (start with `tesseract/src/api/baseapi.h`).
///    For each `EntityKind::ClassDecl` / `StructDecl` cursor, build a
///    [`CppClass`]: its `namespace` from the enclosing namespace cursors,
///    its `name` from the cursor spelling, and one [`Declaration`] per
///    child cursor (base specifier → [`Declaration::Base`], field →
///    [`Declaration::Field`], method → [`Declaration::Method`] with its
///    `virtual`/`override`/`= 0`/`constexpr`/`noexcept`/operator/`requires`
///    flags read from the cursor, etc.). For an `override`, set
///    [`ruff_spo_triplet::CppMethod::overrides`] to the **fully-qualified**
///    base method (clang's `get_overridden_cursors()` spelling,
///    `Namespace::Base.method`), so the `virtually_overrides` edge joins the
///    base class's own method node (codex P2, PR #8).
/// 3. Call [`model_from_class`] per class and push into the [`ModelGraph`].
/// 4. The output MUST match the locked shape asserted in
///    [`tests::locked_shape_expands_to_expected_triples`] for the
///    `Tesseract::Recognizer` representative class.
///
/// Until step 1 lands, this panics with a pointer to the contract. The
/// pure unpacking ([`model_from_class`]) and the target triple shape are
/// already testable without libclang.
#[must_use]
pub fn extract(source_tree: &Path) -> ModelGraph {
    let _ = source_tree;
    todo!(
        "wire the `clang` crate libclang walker per the `extract` doc \
         contract — emit one CppClass per class cursor, then model_from_class"
    )
}

/// The pure unpacking: build a [`Model`] from a parsed [`CppClass`] by
/// routing each [`Declaration`] into its typed `Model::*` sibling slot.
///
/// No semantic transform here — this is the seam between source-order
/// parsing and category-grouped IR. Once [`extract`]'s libclang walker
/// lands, it calls this per class.
#[must_use]
pub fn model_from_class(class: &CppClass) -> Model {
    let mut model = Model::new(class.qualified_name());
    for decl in &class.declarations {
        unpack_declaration(&mut model, decl);
    }
    model
}

/// Route each [`Declaration`] into its typed `Model::*` Vec slot. Guards the
/// frontend → IR seam against drift if a new [`Declaration`] variant is
/// added without a routing arm.
fn unpack_declaration(model: &mut Model, decl: &Declaration) {
    match decl {
        Declaration::Base(b) => model.bases.push(b.clone()),
        Declaration::Field(f) => model.member_fields.push(f.clone()),
        Declaration::Method(m) => model.methods.push(m.clone()),
        Declaration::Template(t) => model.templates.push(t.clone()),
        Declaration::Friend(fr) => model.friends.push(fr.clone()),
        Declaration::MacroUse(mu) => model.macro_uses.push(mu.clone()),
        Declaration::StaticAssert(sa) => model.static_asserts.push(sa.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_spo_triplet::{ConstexprKind, CppAccess, CppTemplateKind, expand};

    /// Locked target shape: a hand-built [`ModelGraph`] matching what a
    /// finished [`extract`] MUST produce for the `Tesseract::Recognizer`
    /// representative class. This test passes today (it does not call the
    /// `todo!()` walker); it tells the frontend author what "done" looks
    /// like. Mirrors `ruff_ruby_spo::tests::locked_shape_expands_to_expected_triples`.
    fn locked_recognizer_graph() -> ModelGraph {
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
            overrides: Some("Tesseract::Classify.Recognize".to_string()),
            operator_kind: None,
            requires_clause: None,
        });
        rec.methods.push(CppMethod {
            name: "Clear".to_string(),
            is_pure_virtual: true,
            constexpr_kind: None,
            is_noexcept: false,
            overrides: None,
            operator_kind: None,
            requires_clause: None,
        });
        rec.templates.push(CppTemplate {
            kind: CppTemplateKind::Specialisation,
            name: "GenericVector<int>".to_string(),
        });
        rec.friends.push(CppFriend {
            name: "TessdataManager".to_string(),
        });
        rec.static_asserts.push(CppStaticAssert {
            condition: "sizeof(float) == 4".to_string(),
        });
        ModelGraph {
            namespace: NAMESPACE.to_string(),
            models: vec![rec],
        }
    }

    #[test]
    fn locked_shape_expands_to_expected_triples() {
        let triples = expand(&locked_recognizer_graph());
        let has =
            |s: &str, p: &str, o: &str| triples.iter().any(|t| t.s == s && t.p == p && t.o == o);

        // ObjectType / Property / Function classification.
        assert!(has(
            "cpp:Tesseract::Recognizer",
            "rdf:type",
            "ogit:ObjectType"
        ));
        assert!(has(
            "cpp:Tesseract::Recognizer.recognizer_",
            "rdf:type",
            "ogit:Property"
        ));
        assert!(has(
            "cpp:Tesseract::Recognizer.Recognize",
            "rdf:type",
            "ogit:Function"
        ));
        // C++ machine-plane edges.
        assert!(has(
            "cpp:Tesseract::Recognizer",
            "inherits_from",
            "cpp:Tesseract::Classify"
        ));
        assert!(has(
            "cpp:Tesseract::Recognizer",
            "has_field",
            "cpp:Tesseract::Recognizer.recognizer_"
        ));
        assert!(has(
            "cpp:Tesseract::Recognizer.Recognize",
            "is_noexcept",
            "true"
        ));
        assert!(has(
            "cpp:Tesseract::Recognizer.Recognize",
            "virtually_overrides",
            "cpp:Tesseract::Classify.Recognize"
        ));
        assert!(has(
            "cpp:Tesseract::Recognizer.Clear",
            "is_pure_virtual",
            "true"
        ));
        assert!(has(
            "cpp:Tesseract::Recognizer",
            "template_specialises",
            "GenericVector<int>"
        ));
        assert!(has(
            "cpp:Tesseract::Recognizer",
            "is_friend_of",
            "TessdataManager"
        ));
        assert!(has(
            "cpp:Tesseract::Recognizer",
            "static_asserts",
            "sizeof(float) == 4"
        ));
    }

    #[test]
    fn namespace_is_cpp() {
        let triples = expand(&locked_recognizer_graph());
        assert!(triples.iter().all(|t| t.s.starts_with("cpp:")));
    }

    #[test]
    fn qualified_name_joins_namespace() {
        let cls = CppClass {
            namespace: vec!["Tesseract".to_string()],
            name: "Recognizer".to_string(),
            declarations: vec![],
        };
        assert_eq!(cls.qualified_name(), "Tesseract::Recognizer");

        let nested = CppClass {
            namespace: vec!["tesseract".to_string(), "lstm".to_string()],
            name: "Network".to_string(),
            declarations: vec![],
        };
        assert_eq!(nested.qualified_name(), "tesseract::lstm::Network");

        let global = CppClass {
            namespace: vec![],
            name: "TBLOB".to_string(),
            declarations: vec![],
        };
        assert_eq!(global.qualified_name(), "TBLOB");
    }

    /// Unpacking lock: a fully-populated `CppClass.declarations` list must
    /// end up in the corresponding `Model::*` Vec slots after
    /// [`model_from_class`] runs across every variant. Guards the
    /// frontend → IR seam against drift if a new [`Declaration`] variant is
    /// added without a routing arm. Mirrors
    /// `ruff_ruby_spo::tests::declarations_unpack_into_typed_model_slots`.
    #[test]
    fn declarations_unpack_into_typed_model_slots() {
        let class = CppClass {
            namespace: vec!["Tesseract".to_string()],
            name: "Recognizer".to_string(),
            declarations: vec![
                Declaration::Base(CppBase {
                    name: "Classify".to_string(),
                    access: CppAccess::Public,
                    virtual_base: false,
                }),
                Declaration::Field(CppField {
                    name: "recognizer_".to_string(),
                    type_name: "LSTMRecognizer*".to_string(),
                }),
                Declaration::Method(CppMethod {
                    name: "Recognize".to_string(),
                    is_pure_virtual: false,
                    constexpr_kind: Some(ConstexprKind::Constexpr),
                    is_noexcept: true,
                    overrides: None,
                    operator_kind: None,
                    requires_clause: None,
                }),
                Declaration::Template(CppTemplate {
                    kind: CppTemplateKind::Instantiation,
                    name: "GenericVector<int>".to_string(),
                }),
                Declaration::Friend(CppFriend {
                    name: "TessdataManager".to_string(),
                }),
                Declaration::MacroUse(CppMacroUse {
                    identifier: "BOOL_VAR_H".to_string(),
                    macro_name: "BOOL_VAR".to_string(),
                }),
                Declaration::StaticAssert(CppStaticAssert {
                    condition: "sizeof(int) == 4".to_string(),
                }),
            ],
        };
        let model = model_from_class(&class);
        assert_eq!(model.name, "Tesseract::Recognizer");
        assert_eq!(model.bases.len(), 1);
        assert_eq!(model.member_fields.len(), 1);
        assert_eq!(model.methods.len(), 1);
        assert_eq!(model.templates.len(), 1);
        assert_eq!(model.friends.len(), 1);
        assert_eq!(model.macro_uses.len(), 1);
        assert_eq!(model.static_asserts.len(), 1);
        // The Ruby/Python sibling slots stay empty — no cross-language bleed.
        assert!(model.associations.is_empty());
        assert!(model.functions.is_empty());
        assert!(model.sti.is_none());
    }

    /// `model_from_class` → `expand` round-trip: the unpacking path produces
    /// the same triples as the hand-built locked graph for a single class.
    #[test]
    fn model_from_class_matches_locked_shape() {
        let class = CppClass {
            namespace: vec!["Tesseract".to_string()],
            name: "Recognizer".to_string(),
            declarations: vec![Declaration::Base(CppBase {
                name: "Tesseract::Classify".to_string(),
                access: CppAccess::Public,
                virtual_base: false,
            })],
        };
        let mut graph = ModelGraph::new(NAMESPACE);
        graph.models.push(model_from_class(&class));
        let triples = expand(&graph);
        assert!(triples.iter().any(|t| {
            t.s == "cpp:Tesseract::Recognizer"
                && t.p == "inherits_from"
                && t.o == "cpp:Tesseract::Classify"
        }));
    }
}
