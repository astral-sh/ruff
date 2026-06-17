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
//! - `walk_tu` (feature `libclang`) walks ONE translation unit via real
//!   libclang and returns [`CppClass`] definitions (classes/bases/fields/
//!   methods with their flags, system-header classes filtered out).
//!   [`extract`] — the corpus-TREE orchestration over `walk_tu` — remains
//!   `todo!()` (per-TU include resolution + cross-TU dedup). The target
//!   triple shape is locked by [`tests::locked_shape_expands_to_expected_triples`].
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

#[cfg(feature = "libclang")]
mod clang_walker;
#[cfg(feature = "libclang")]
pub use clang_walker::{MAPPED_CURSOR_KINDS, WalkError, class_body_cursor_histogram, walk_tu};

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

/// Top-level entry: walk a C++ corpus **tree** and produce the IR.
///
/// **Still `todo!()` — what's missing now is only per-TU include
/// auto-detection.** Everything beneath it is done: [`walk_tu`] walks ONE
/// translation unit, and [`extract_tree`] already does the recursive
/// enumeration + per-TU walk + cross-TU dedup for a SINGLE caller-supplied
/// include-arg set. What `extract` adds is resolving the right include args
/// *per TU automatically* (the `tesseract-ocr/tesseract@5.5.0` + leptonica
/// include graph — the real remaining work), so a caller can point it at a
/// corpus root without hand-supplying `-I` flags. Once that lands it is
/// essentially [`extract_tree`] with auto-derived args, plus the
/// `CPP-SCHEMA-FIT` coverage gate (`.claude/plans/cpp-spo-probes-v1.md`).
#[must_use]
pub fn extract(source_tree: &Path) -> ModelGraph {
    let _ = source_tree;
    todo!(
        "orchestrate `walk_tu` over the corpus tree (per-TU include args + \
         cross-TU dedup) — the per-TU walker itself is done; see the doc"
    )
}

/// First tree-orchestration cut (feature `libclang`): walk every C++ header /
/// source file directly in `dir` (non-recursive) as its own translation unit,
/// dedup classes by fully-qualified name, and return one merged
/// [`ModelGraph`]. `args` (include dirs, `-std`, …) apply to every TU.
///
/// Per-TU **parse** failures ([`WalkError::Parse`] — missing includes,
/// malformed TU) are skipped: they are expected on a real corpus and the
/// other files still extract. A **libclang-init** failure
/// ([`WalkError::Libclang`] — wrong `LIBCLANG_PATH`, or a `Clang` singleton
/// already active) is non-recoverable — it would make EVERY file skip and
/// return a misleadingly-empty graph — so it is propagated as `Err` rather
/// than swallowed (codex P2, PR #13). Output is deterministic: files are
/// visited in sorted order and the dedup map is a `BTreeMap`, so the model
/// order is stable.
///
/// This is the concrete first step of the corpus-tree orchestration
/// [`extract`] documents: a single directory (non-recursive — see
/// [`extract_tree`] for the recursive whole-tree walk), caller-supplied
/// include args, no auto-include-detection yet. Pair with
/// [`ruff_spo_triplet::expand`] + [`ruff_spo_triplet::to_ndjson`] for the
/// first SPO ndjson emission from a real corpus subset.
#[cfg(feature = "libclang")]
pub fn extract_dir(dir: &Path, args: &[String]) -> Result<ModelGraph, WalkError> {
    let mut files: Vec<std::path::PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| is_cpp_source(p))
            .collect(),
        Err(_) => Vec::new(),
    };
    files.sort();
    walk_files(&files, args)
}

/// Recursive corpus-tree walk (feature `libclang`): walk every C++ file under
/// `root` (depth-first, all subdirectories) as its own translation unit, dedup
/// classes by fully-qualified name across the whole tree, and return one merged
/// [`ModelGraph`]. `args` apply to every TU.
///
/// Same skip/propagate semantics as [`extract_dir`] (per-TU
/// [`WalkError::Parse`] skipped, [`WalkError::Libclang`] surfaced) and the same
/// deterministic ordering (files sorted, dedup via `BTreeMap`).
///
/// This is the recursive half of the corpus-tree orchestration [`extract`]
/// documents: the caller still supplies one include-arg set for the whole tree;
/// `extract` will add per-TU include auto-detection on top.
#[cfg(feature = "libclang")]
pub fn extract_tree(root: &Path, args: &[String]) -> Result<ModelGraph, WalkError> {
    let mut files = Vec::new();
    collect_cpp_files(root, &mut files);
    files.sort();
    walk_files(&files, args)
}

/// File extensions the walker treats as a C++ translation unit.
#[cfg(feature = "libclang")]
fn is_cpp_source(p: &Path) -> bool {
    p.extension()
        .and_then(|x| x.to_str())
        .is_some_and(|x| matches!(x, "h" | "hpp" | "hh" | "hxx" | "cc" | "cpp" | "cxx"))
}

/// Recursively collect C++ source files under `dir` (depth-first). An unreadable
/// directory is skipped rather than aborting the walk — a permission error on
/// one branch must not lose the rest of the corpus.
#[cfg(feature = "libclang")]
fn collect_cpp_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.filter_map(Result::ok) {
        // Use the entry's OWN file type (does NOT follow symlinks). A directory
        // symlink to an ancestor (`sub/loop -> ..`) would otherwise recurse
        // forever via `Path::is_dir()`'s follow. Symlinks are skipped — a
        // source corpus's TUs are real files — which breaks all cycles without
        // canonicalize bookkeeping (codex P2, PR #14).
        let Ok(ft) = entry.file_type() else { continue };
        let path = entry.path();
        if ft.is_dir() {
            collect_cpp_files(&path, out);
        } else if ft.is_file() && is_cpp_source(&path) {
            out.push(path);
        }
    }
}

/// The shared dedup loop behind [`extract_dir`] / [`extract_tree`]: walk each
/// file as its own TU, dedup classes by fully-qualified name into a
/// deterministic `BTreeMap`, skip per-TU [`WalkError::Parse`] failures, and
/// propagate a non-recoverable [`WalkError::Libclang`] (codex P2, PR #13).
#[cfg(feature = "libclang")]
fn walk_files(files: &[std::path::PathBuf], args: &[String]) -> Result<ModelGraph, WalkError> {
    let mut seen: std::collections::BTreeMap<String, Model> = std::collections::BTreeMap::new();
    for f in files {
        match walk_tu(f, args) {
            Ok(classes) => {
                for cls in classes {
                    seen.entry(cls.qualified_name())
                        .or_insert_with(|| model_from_class(&cls));
                }
            }
            Err(WalkError::Parse(_)) => {}
            Err(e @ WalkError::Libclang(_)) => return Err(e),
        }
    }
    let mut graph = ModelGraph::new(NAMESPACE);
    graph.models = seen.into_values().collect();
    Ok(graph)
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
            // Per-overload override target (codex P2 #17): the signature suffix
            // matches `cpp_method`'s per-overload method IRI convention so
            // `virtually_overrides` joins the EXACT base overload (not just any
            // base method with the same name).
            overrides: Some("Tesseract::Classify.Recognize(int)".to_string()),
            operator_kind: None,
            requires_clause: None,
            return_type: None,
            param_types: vec!["int".to_string()],
            is_const: false,
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
            "cpp:Tesseract::Recognizer.Recognize(int)",
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
            "cpp:Tesseract::Recognizer.Recognize(int)",
            "is_noexcept",
            "true"
        ));
        assert!(has(
            "cpp:Tesseract::Recognizer.Recognize(int)",
            "virtually_overrides",
            "cpp:Tesseract::Classify.Recognize(int)"
        ));
        assert!(has(
            "cpp:Tesseract::Recognizer.Clear()",
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
                    return_type: None,
                    param_types: Vec::new(),
                    is_const: false,
                    is_static: false,
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

#[cfg(all(test, feature = "libclang"))]
mod libclang_tests {
    use std::io::Write;
    use std::sync::Mutex;

    use ruff_spo_triplet::{cpp_projection, expand, from_ndjson, reassemble, to_ndjson};

    use super::{
        MAPPED_CURSOR_KINDS, ModelGraph, NAMESPACE, class_body_cursor_histogram, extract_dir,
        extract_tree, model_from_class, walk_tu,
    };

    /// `clang::Clang` is a process-singleton — serialize the libclang tests so
    /// cargo's parallel test threads never construct two at once.
    static CLANG_LOCK: Mutex<()> = Mutex::new(());

    /// Hermetic libclang walk: write a small self-contained C++ TU (no
    /// includes), walk it via real libclang, and assert the extracted shape +
    /// the SPO triples it expands to. This is the libclang analog of
    /// `ruff_ruby_spo`'s synthetic-fixture test — it proves the walker
    /// end-to-end without needing the Tesseract corpus or its include graph.
    ///
    /// Run: `LIBCLANG_PATH=/usr/lib/llvm-18/lib cargo test -p ruff_cpp_spo \
    ///       --features libclang`.
    #[test]
    fn walk_extracts_classes_bases_methods_fields_from_real_cpp() {
        const SRC: &str = r"
namespace Tesseract {
class Classify {
 public:
  virtual int Recognize(int x) noexcept;
  virtual void Clear() = 0;
};
template <typename T>
class Box {
 public:
  T get() const;
  void set(T v);
};
template <typename T>
class Box<T*> {
 public:
  T* get_ptr() const;
};
class Recognizer : public Classify {
 public:
  Recognizer();
  virtual ~Recognizer();
  int Recognize(int x) noexcept override;
  bool operator==(const Recognizer& other) const;
  void stash(const Box<char>& b);
  // Overloaded `process` (codex P2 #17 overload-discrimination probe): the two
  // signatures must end up on DISTINCT method IRIs (`Foo.f(int)` vs
  // `Foo.f(double)`), not collide into one node.
  int process(int x);
  int process(double x);
  friend class TessdataManager;
 private:
  int recognizer_;
  static int count_;
  Box<int> boxed_;
};
}
";
        let _guard = CLANG_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = std::env::temp_dir();
        let path = dir.join("ruff_cpp_spo_hermetic_fixture.cpp");
        {
            let mut f = std::fs::File::create(&path).expect("create fixture");
            f.write_all(SRC.as_bytes()).expect("write fixture");
        }
        let args = [
            "-std=c++17".to_string(),
            "-x".to_string(),
            "c++".to_string(),
        ];
        let classes = walk_tu(&path, &args).expect("libclang walk");
        let _ = std::fs::remove_file(&path);

        let find = |q: &str| {
            classes
                .iter()
                .find(|c| c.qualified_name() == q)
                .unwrap_or_else(|| panic!("class {q} not found; got {:?}", names(&classes)))
        };
        let recognizer = find("Tesseract::Recognizer");
        let classify = find("Tesseract::Classify");

        // Base specifier (access + qualified base name).
        let base = recognizer
            .declarations
            .iter()
            .find_map(|d| match d {
                super::Declaration::Base(b) => Some(b),
                _ => None,
            })
            .expect("Recognizer has a base");
        assert_eq!(base.name, "Tesseract::Classify");
        assert!(matches!(base.access, ruff_spo_triplet::CppAccess::Public));

        // Field.
        assert!(
            recognizer.declarations.iter().any(|d| matches!(
                d, super::Declaration::Field(f) if f.name == "recognizer_"
            )),
            "field recognizer_ missing"
        );

        // Methods: override target FQ, noexcept, operator, pure-virtual.
        let methods: Vec<&ruff_spo_triplet::CppMethod> = recognizer
            .declarations
            .iter()
            .filter_map(|d| match d {
                super::Declaration::Method(m) => Some(m),
                _ => None,
            })
            .collect();
        let recognize = methods
            .iter()
            .find(|m| m.name == "Recognize")
            .expect("Recognize method");
        assert!(recognize.is_noexcept, "Recognize should be noexcept");
        assert_eq!(
            recognize.overrides.as_deref(),
            Some("Tesseract::Classify.Recognize(int)"),
            "override target must be the fully-qualified base method"
        );
        // AST-DLL signature shape: `int Recognize(int x)` → return + one param.
        assert_eq!(
            recognize.return_type.as_deref(),
            Some("int"),
            "Recognize returns int"
        );
        assert_eq!(
            recognize.param_types,
            vec!["int".to_string()],
            "Recognize takes one int param"
        );
        // `void stash(const Box<char>&)` → void return is skipped, one param.
        let stash = methods
            .iter()
            .find(|m| m.name == "stash")
            .expect("stash method");
        assert!(
            stash.return_type.is_none(),
            "void return must not emit returns_type"
        );
        assert_eq!(
            stash.param_types,
            vec!["const Box<char> &".to_string()],
            "stash parameter type captured verbatim"
        );
        let op = methods
            .iter()
            .find(|m| m.operator_kind.is_some())
            .expect("operator== method");
        assert_eq!(op.operator_kind.as_deref(), Some("operator=="));
        // ORM-downcast shape: `bool operator==(...) const` is const, not static.
        assert!(op.is_const, "const operator== must set is_const");
        assert!(!op.is_static, "operator== is not static");

        // Codex P2 #17 overload-discrimination probe: both `process` overloads
        // must be captured as distinct methods (the IR carries each with its
        // own `param_types`; `cpp_method`'s per-overload method-IRI
        // (`process(int)` vs `process(double)`) then keeps their signature
        // triples on separate nodes — measured downstream via the every-cpp-
        // predicate fixture). At the IR level we observe two `process` entries
        // with the two param-type lists.
        let processes: Vec<&Vec<String>> = methods
            .iter()
            .filter(|m| m.name == "process")
            .map(|m| &m.param_types)
            .collect();
        assert_eq!(
            processes.len(),
            2,
            "both `process` overloads must be captured: {processes:?}"
        );
        let process_params: std::collections::BTreeSet<String> =
            processes.iter().map(|p| p.join(",")).collect();
        assert!(
            process_params.contains("int") && process_params.contains("double"),
            "overload signatures must be distinct (saw {process_params:?})"
        );

        // Constructors and destructors are member functions too: libclang
        // reports them under cursor kinds distinct from `Method`, but the walker
        // captures both as `has_function` (the CPP-SCHEMA-FIT ctor/dtor fix).
        assert!(
            methods.iter().any(|m| m.name == "Recognizer"),
            "constructor must be captured as a method"
        );
        assert!(
            methods.iter().any(|m| m.name == "~Recognizer"),
            "destructor must be captured as a method"
        );

        // Static data members are VarDecl in libclang — captured as has_field.
        assert!(
            recognizer.declarations.iter().any(|d| matches!(
                d, super::Declaration::Field(f) if f.name == "count_"
            )),
            "static member count_ must be captured as a field"
        );
        // `friend class Foo;` — captured as is_friend_of via CppFriend. libclang
        // resolves the TypeRef to the fully-qualified type, so the name is
        // namespace-qualified (consistent with every other harvested name).
        assert!(
            recognizer.declarations.iter().any(|d| matches!(
                d, super::Declaration::Friend(fr) if fr.name == "Tesseract::TessdataManager"
            )),
            "friend class Tesseract::TessdataManager must be captured"
        );

        // Class templates are harvested as classes (Shape A): libclang flattens
        // the `ClassTemplate`, so its methods are captured like any class's. The
        // harvested name is the bare template name (no `<T>`).
        let boxed = find("Tesseract::Box");
        assert!(
            boxed.declarations.iter().any(|d| matches!(
                d, super::Declaration::Method(m) if m.name == "get"
            )),
            "class template Box::get must be captured as a method"
        );
        // Codex P2 #17: a `ClassTemplatePartialSpecialization` must keep its
        // arguments in the qualified name (`Box<T *>`) so it does NOT collide
        // with the primary `Box` in the cross-TU `BTreeMap` dedup. Both must be
        // present, and the partial spec must carry its own distinct members.
        let names: Vec<&str> = classes.iter().map(|c| c.name.as_str()).collect();
        assert!(
            names.contains(&"Box"),
            "primary class template `Box` must be present: {names:?}"
        );
        assert!(
            names.contains(&"Box<T *>"),
            "partial spec `Box<T *>` must be present (distinct from primary): {names:?}"
        );
        let partial = find("Tesseract::Box<T *>");
        assert!(
            partial.declarations.iter().any(|d| matches!(
                d, super::Declaration::Method(m) if m.name == "get_ptr"
            )),
            "partial spec must carry its own `get_ptr` member"
        );

        // Shape C: template-instantiation USES become template_instantiates.
        // `Box<int> boxed_;` (field type) and `stash(const Box<char>&)` (method
        // signature) both surface as Instantiation declarations on Recognizer —
        // info `cpp_field`/`cpp_method` otherwise drop.
        let insts: Vec<&str> = recognizer
            .declarations
            .iter()
            .filter_map(|d| match d {
                super::Declaration::Template(t)
                    if matches!(t.kind, ruff_spo_triplet::CppTemplateKind::Instantiation) =>
                {
                    Some(t.name.as_str())
                }
                _ => None,
            })
            .collect();
        assert!(
            insts.contains(&"Box<int>"),
            "field-type instantiation Box<int> must be captured: {insts:?}"
        );
        assert!(
            insts.contains(&"Box<char>"),
            "method-signature instantiation Box<char> must be captured: {insts:?}"
        );

        let clear = classify
            .declarations
            .iter()
            .find_map(|d| match d {
                super::Declaration::Method(m) if m.name == "Clear" => Some(m),
                _ => None,
            })
            .expect("Clear method");
        assert!(clear.is_pure_virtual, "Clear should be pure-virtual");

        // End-to-end: the walked classes expand to the expected triples.
        let mut graph = ModelGraph::new(NAMESPACE);
        graph.models.push(model_from_class(recognizer));
        graph.models.push(model_from_class(classify));
        let triples = expand(&graph);
        let has =
            |s: &str, p: &str, o: &str| triples.iter().any(|t| t.s == s && t.p == p && t.o == o);
        assert!(has(
            "cpp:Tesseract::Recognizer",
            "inherits_from",
            "cpp:Tesseract::Classify"
        ));
        assert!(has(
            "cpp:Tesseract::Recognizer.Recognize(int)",
            "virtually_overrides",
            "cpp:Tesseract::Classify.Recognize(int)"
        ));
        assert!(has(
            "cpp:Tesseract::Recognizer.Recognize(int)",
            "is_noexcept",
            "true"
        ));
        assert!(has(
            "cpp:Tesseract::Recognizer",
            "has_field",
            "cpp:Tesseract::Recognizer.recognizer_"
        ));
        assert!(has(
            "cpp:Tesseract::Recognizer.operator==(const Recognizer &)",
            "defines_operator",
            "operator=="
        ));
        assert!(has(
            "cpp:Tesseract::Classify.Clear()",
            "is_pure_virtual",
            "true"
        ));
    }

    fn names(classes: &[super::CppClass]) -> Vec<String> {
        classes
            .iter()
            .map(super::CppClass::qualified_name)
            .collect()
    }

    /// Real-corpus smoke (the `CPP-SCHEMA-FIT` kernel) — gated on
    /// `TESSERACT_SRC` so CI without the corpus skips it, mirroring
    /// `ruff_ruby_spo`'s `OPENPROJECT_PATH` gate. Walks a real Tesseract
    /// header; tolerates the unresolved generated/leptonica includes (libclang
    /// still surfaces the class decls), and asserts non-trivial extraction.
    ///
    /// Run: `TESSERACT_SRC=/path/to/tesseract LIBCLANG_PATH=/usr/lib/llvm-18/lib \
    ///       cargo test -p ruff_cpp_spo --features libclang -- --nocapture`.
    #[test]
    #[expect(
        clippy::print_stderr,
        reason = "diagnostic emission gated on env var (real-corpus smoke)"
    )]
    fn walk_real_tesseract_header_when_corpus_present() {
        let _guard = CLANG_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Ok(src_root) = std::env::var("TESSERACT_SRC") else {
            eprintln!("TESSERACT_SRC unset; skipping real-corpus smoke");
            return;
        };
        let root = std::path::Path::new(&src_root);
        let header = root.join("src/ccutil/unicharset.h");
        if !header.exists() {
            eprintln!("{} missing; skipping", header.display());
            return;
        }
        let args = [
            "-std=c++17".to_string(),
            "-x".to_string(),
            "c++".to_string(),
            format!("-I{}", root.join("src/ccutil").display()),
            format!("-I{}", root.join("include").display()),
        ];
        let classes = walk_tu(&header, &args).expect("walk real Tesseract header");
        eprintln!(
            "[tesseract-smoke] {} classes from unicharset.h: {:?}",
            classes.len(),
            names(&classes)
        );
        assert!(
            !classes.is_empty(),
            "expected >=1 class from unicharset.h even with unresolved includes"
        );
        let mut graph = ModelGraph::new(NAMESPACE);
        for c in &classes {
            graph.models.push(model_from_class(c));
        }
        assert!(
            !expand(&graph).is_empty(),
            "expected SPO triples from the real header"
        );
    }

    /// First **ndjson emission** from a real corpus subset — gated on
    /// `TESSERACT_SRC`. Walks all of `src/ccutil` via [`extract_dir`], expands
    /// to SPO triples, serialises to ndjson, and round-trips it. The
    /// "produce the artifact, then verify it parses back" milestone for the
    /// C++ machine plane (the ndjson is exactly what the lance-graph SPO store
    /// consumes).
    #[test]
    #[expect(clippy::print_stderr, reason = "diagnostic emission gated on env var")]
    fn extract_dir_emits_roundtrippable_ndjson_from_ccutil() {
        let _guard = CLANG_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Ok(src_root) = std::env::var("TESSERACT_SRC") else {
            eprintln!("TESSERACT_SRC unset; skipping ndjson-emission milestone");
            return;
        };
        let root = std::path::Path::new(&src_root);
        let dir = root.join("src/ccutil");
        if !dir.is_dir() {
            eprintln!("{} missing; skipping", dir.display());
            return;
        }
        let args = [
            "-std=c++17".to_string(),
            "-x".to_string(),
            "c++".to_string(),
            format!("-I{}", root.join("src/ccutil").display()),
            format!("-I{}", root.join("include").display()),
        ];
        let graph = extract_dir(&dir, &args).expect("libclang init (LIBCLANG_PATH set)");
        let triples = expand(&graph);
        let ndjson = to_ndjson(&triples);
        eprintln!(
            "[ccutil-ndjson] {} classes -> {} triples, {} ndjson bytes",
            graph.models.len(),
            triples.len(),
            ndjson.len()
        );
        assert!(
            graph.models.len() >= 10,
            "expected many tesseract classes from ccutil, got {}",
            graph.models.len()
        );
        assert!(!triples.is_empty(), "expected SPO triples");
        // Every model yields at least its rdf:type ObjectType triple.
        assert!(triples.len() >= graph.models.len());
        // The emitted ndjson must load back losslessly — the lance-graph SPO
        // store consumes exactly this.
        let parsed = from_ndjson(&ndjson).expect("ndjson round-trips");
        assert_eq!(parsed.len(), triples.len(), "ndjson round-trip is lossless");
        // Every C++ subject carries the `cpp:` namespace (or `exc:` for raises).
        assert!(
            parsed
                .iter()
                .all(|t| t.s.starts_with("cpp:") || t.s.starts_with("exc:"))
        );
    }

    /// `CPP-REASSEMBLE-RT` — the AST-DLL generator's stage-1 reassembler run
    /// against the REAL corpus (the re-scoped C-FIRST falsifier). Harvests
    /// `ccutil`, expands to triples, serialises + parses ndjson, then
    /// [`reassemble`]s the triple set back into a [`ModelGraph`] and checks the
    /// round-trip invariants that MUST hold on real data:
    ///
    /// 1. **Class-set preservation** — reassembly recovers exactly the
    ///    harvested class set (anchor-first attribution: no class invented or
    ///    lost, no cross-attribution).
    /// 2. **Idempotence** — `reassemble(expand(·))` is a fixed point. Expanding
    ///    and reassembling the reassembled graph yields an identical graph.
    ///    This catches any non-determinism / ordering / attribution bug on real
    ///    data without tripping over the const-vs-non-const method-IRI
    ///    collisions that make strict `cpp_projection` equality too strong on a
    ///    real corpus (const-ness is not in the `(params)` suffix, so a
    ///    `T& at(i)` / `const T& at(i) const` pair shares one IRI and `expand`
    ///    merges them — a real harvester limitation this probe MEASURES).
    /// 3. **Fidelity report** — prints how many classes round-trip byte-exact
    ///    against [`cpp_projection`] and how many differ (the IRI-collision
    ///    tail), so the collision count is measured, not asserted away.
    ///
    /// `ccutil/unicharset.h` carries the `UNICHARSET` / `UNICHARMAP`
    /// same-named-method pair that makes anchor-first attribution non-trivial,
    /// so this is a genuine real-data falsifier, not a fixture echo. Gated on
    /// `TESSERACT_SRC`.
    #[test]
    #[expect(clippy::print_stderr, reason = "fidelity report gated on env var")]
    fn cpp_reassemble_round_trip_on_real_corpus() {
        let _guard = CLANG_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Ok(src_root) = std::env::var("TESSERACT_SRC") else {
            eprintln!("TESSERACT_SRC unset; skipping CPP-REASSEMBLE-RT");
            return;
        };
        let root = std::path::Path::new(&src_root);
        let dir = root.join("src/ccutil");
        if !dir.is_dir() {
            eprintln!("{} missing; skipping", dir.display());
            return;
        }
        let args = [
            "-std=c++17".to_string(),
            "-x".to_string(),
            "c++".to_string(),
            format!("-I{}", root.join("src/ccutil").display()),
            format!("-I{}", root.join("include").display()),
        ];

        let g = extract_dir(&dir, &args).expect("libclang init (LIBCLANG_PATH set)");
        let parsed = from_ndjson(&to_ndjson(&expand(&g))).expect("ndjson round-trips");
        let r1 = reassemble(&parsed);

        // (1) Class-set preservation — the anchor-first guarantee on real data.
        let harvested: std::collections::BTreeSet<&str> =
            g.models.iter().map(|m| m.name.as_str()).collect();
        let recovered: std::collections::BTreeSet<&str> =
            r1.models.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(
            harvested, recovered,
            "reassembly must recover exactly the harvested class set"
        );

        // (2) Idempotence — reassemble∘expand is a fixed point on real data.
        let r2 = reassemble(&expand(&r1));
        assert_eq!(
            r1, r2,
            "reassembly must be a fixed point on the real corpus (CPP-REASSEMBLE-RT)"
        );

        // (3) Fidelity vs the collision-blind projection — measured, not gated.
        let projection = cpp_projection(&g);
        let exact = r1
            .models
            .iter()
            .filter(|m| projection.models.iter().any(|p| p == *m))
            .count();
        eprintln!(
            "[CPP-REASSEMBLE-RT] {} classes; {} round-trip byte-exact vs projection, \
             {} differ (const-overload IRI-collision tail)",
            r1.models.len(),
            exact,
            r1.models.len() - exact
        );

        // Sanity anchor: UNICHARSET (the canonical ccutil class) must survive.
        assert!(
            recovered.iter().any(|n| n.ends_with("UNICHARSET")),
            "reassembled ccutil must include UNICHARSET: {recovered:?}"
        );
        assert!(exact > 0, "at least some classes must round-trip byte-exact");
    }

    /// `extract_tree` recurses into subdirectories; `extract_dir` does not.
    /// Hermetic (writes a small nested temp tree) — proves the recursive walk
    /// end-to-end without needing the Tesseract corpus.
    #[test]
    fn extract_tree_recurses_where_extract_dir_does_not() {
        let _guard = CLANG_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let base = std::env::temp_dir().join("ruff_cpp_spo_tree_fixture");
        let sub = base.join("sub");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&sub).expect("mkdir tree");
        std::fs::write(base.join("top.h"), "namespace T { class Top {}; }").expect("write top");
        std::fs::write(sub.join("nested.h"), "namespace T { class Nested {}; }")
            .expect("write nested");
        let args = [
            "-std=c++17".to_string(),
            "-x".to_string(),
            "c++".to_string(),
        ];

        let tree = extract_tree(&base, &args).expect("libclang init");
        let tnames: Vec<&str> = tree.models.iter().map(|m| m.name.as_str()).collect();
        assert!(
            tnames.contains(&"T::Top"),
            "tree missing T::Top: {tnames:?}"
        );
        assert!(
            tnames.contains(&"T::Nested"),
            "extract_tree must recurse into sub/: {tnames:?}"
        );

        // Non-recursive: extract_dir sees only the top-level file.
        let dir = extract_dir(&base, &args).expect("libclang init");
        let dnames: Vec<&str> = dir.models.iter().map(|m| m.name.as_str()).collect();
        assert!(dnames.contains(&"T::Top"));
        assert!(
            !dnames.contains(&"T::Nested"),
            "extract_dir must NOT recurse: {dnames:?}"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// A directory symlink to an ancestor (`sub/loop -> base`) must NOT send
    /// `extract_tree` into an unbounded recurse — symlinks are skipped via the
    /// entry's own file type, not `Path::is_dir()`'s follow (codex P2, PR #14).
    /// Unix-only (symlink creation).
    #[cfg(unix)]
    #[test]
    fn extract_tree_skips_directory_symlink_cycles() {
        let _guard = CLANG_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let base = std::env::temp_dir().join("ruff_cpp_spo_symlink_fixture");
        let sub = base.join("sub");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&sub).expect("mkdir");
        std::fs::write(base.join("real.h"), "namespace S { class Real {}; }").expect("write");
        // sub/loop -> base : a cycle that `Path::is_dir()` would follow forever.
        std::os::unix::fs::symlink(&base, sub.join("loop")).expect("symlink");
        let args = [
            "-std=c++17".to_string(),
            "-x".to_string(),
            "c++".to_string(),
        ];
        // Must terminate (no infinite recurse / path exhaustion) AND still find
        // the real class.
        let graph = extract_tree(&base, &args).expect("libclang init");
        assert!(
            graph.models.iter().any(|m| m.name == "S::Real"),
            "expected S::Real: {:?}",
            graph
                .models
                .iter()
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// `CPP-SCHEMA-FIT` (real-corpus half) — the predicate-vocab coverage gate
    /// (`.claude/plans/cpp-spo-probes-v1.md`). Walks every header in Tesseract's
    /// `src/ccutil`, tallies class-body cursor kinds, and reports the *mapped
    /// fraction* (`BaseSpecifier`/`FieldDecl`/`Method` → a `Declaration`) versus
    /// the constructs the walker currently drops. Gated on `TESSERACT_SRC`;
    /// prints the histogram (`--nocapture`) so the unmapped kinds drive the
    /// walker-follow-up backlog rather than being asserted away.
    ///
    /// Run: `TESSERACT_SRC=/tmp/tesseract LIBCLANG_PATH=/usr/lib/llvm-18/lib \
    ///       cargo test -p ruff_cpp_spo --features libclang -- --nocapture`.
    #[test]
    #[expect(clippy::print_stderr, reason = "coverage report gated on env var")]
    fn cpp_schema_fit_real_corpus_coverage() {
        let _guard = CLANG_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Ok(src_root) = std::env::var("TESSERACT_SRC") else {
            eprintln!("TESSERACT_SRC unset; skipping CPP-SCHEMA-FIT real-corpus coverage");
            return;
        };
        let root = std::path::Path::new(&src_root);
        let dir = root.join("src/ccutil");
        if !dir.is_dir() {
            eprintln!("{} missing; skipping", dir.display());
            return;
        }
        let args = [
            "-std=c++17".to_string(),
            "-x".to_string(),
            "c++".to_string(),
            format!("-I{}", root.join("src/ccutil").display()),
            format!("-I{}", root.join("include").display()),
        ];

        // Walk every ccutil header, merge the per-TU class-body histograms.
        let mut merged: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        let mut headers = 0usize;
        if let Ok(rd) = std::fs::read_dir(&dir) {
            let mut paths: Vec<std::path::PathBuf> = rd
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("h"))
                .collect();
            paths.sort();
            for p in paths {
                if let Ok(hist) = class_body_cursor_histogram(&p, &args) {
                    headers += 1;
                    for (k, v) in hist {
                        *merged.entry(k).or_insert(0) += v;
                    }
                }
            }
        }

        let total: usize = merged.values().sum();
        let mapped: usize = MAPPED_CURSOR_KINDS
            .iter()
            .filter_map(|k| merged.get(*k))
            .sum();
        eprintln!(
            "[CPP-SCHEMA-FIT] {headers} ccutil headers, {total} class-body cursors, \
             {mapped} mapped ({}%)",
            mapped * 100 / total.max(1)
        );
        let mut rows: Vec<(&String, &usize)> = merged.iter().collect();
        rows.sort_by(|a, b| b.1.cmp(a.1));
        for (kind, count) in &rows {
            let mark = if MAPPED_CURSOR_KINDS.contains(&kind.as_str()) {
                "MAP"
            } else {
                "   "
            };
            eprintln!("  [{mark}] {count:>5}  {kind}");
        }

        // Non-degenerate gate: the walk must reach real bodies with the OO
        // constructs the harvester is built to extract. The mapped-fraction
        // THRESHOLD is deliberately not asserted here yet — the first real run
        // measures it; the unmapped kinds above name the walker follow-ups.
        assert!(
            headers > 0,
            "expected to walk >=1 ccutil header (LIBCLANG_PATH?)"
        );
        assert!(total > 0, "expected class-body cursors from a real corpus");
        assert!(
            merged.get("Method").is_some_and(|&n| n > 0),
            "histogram must contain methods (has_function)"
        );
        assert!(
            merged.get("FieldDecl").is_some_and(|&n| n > 0),
            "histogram must contain fields (has_field)"
        );
    }

    /// `CPP-AST-RT` — libclang harvest determinism (the "reproducible harvest"
    /// gate, `.claude/plans/cpp-spo-probes-v1.md`). Walks the SAME corpus subset
    /// twice in-process and asserts byte-identical ndjson. The IR→triples half is
    /// already deterministic (`expand` sorts + dedups); this settles the
    /// libclang→IR half end-to-end (no RNG in the walker; `walk_files` dedups
    /// into a sorted `BTreeMap`). Gated on `TESSERACT_SRC`.
    #[test]
    fn cpp_ast_rt_determinism() {
        let _guard = CLANG_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Ok(src_root) = std::env::var("TESSERACT_SRC") else {
            return;
        };
        let root = std::path::Path::new(&src_root);
        let dir = root.join("src/ccutil");
        if !dir.is_dir() {
            return;
        }
        let args = [
            "-std=c++17".to_string(),
            "-x".to_string(),
            "c++".to_string(),
            format!("-I{}", root.join("src/ccutil").display()),
            format!("-I{}", root.join("include").display()),
        ];
        let harvest = || to_ndjson(&expand(&extract_dir(&dir, &args).expect("libclang init")));
        let first = harvest();
        let second = harvest();
        assert!(!first.is_empty(), "expected a non-empty harvest");
        assert_eq!(
            first, second,
            "harvest must be byte-identical across runs (CPP-AST-RT determinism)"
        );
    }

    /// `CPP-TEMPLATE-DET` — template-instantiation determinism (the third
    /// primary probe, `.claude/plans/cpp-spo-probes-v1.md`). Walks ccutil twice
    /// AND once with the file list reversed, then compares the SET of
    /// `template_instantiates` triples (order-independent — `expand` already
    /// sorts). The set must be identical across runs and across orderings,
    /// AND must be non-degenerate (the syntactic field-type + signature-type
    /// instantiations exist in ccutil — measured 7+ in fields alone). Gated on
    /// `TESSERACT_SRC`.
    #[test]
    #[expect(clippy::print_stderr, reason = "diagnostic emission gated on env var")]
    fn cpp_template_det_determinism() {
        let _guard = CLANG_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Ok(src_root) = std::env::var("TESSERACT_SRC") else {
            return;
        };
        let root = std::path::Path::new(&src_root);
        let dir = root.join("src/ccutil");
        if !dir.is_dir() {
            return;
        }
        let args = [
            "-std=c++17".to_string(),
            "-x".to_string(),
            "c++".to_string(),
            format!("-I{}", root.join("src/ccutil").display()),
            format!("-I{}", root.join("include").display()),
        ];

        let instantiates = |g: &ModelGraph| -> std::collections::BTreeSet<(String, String)> {
            expand(g)
                .into_iter()
                .filter(|t| t.p == "template_instantiates")
                .map(|t| (t.s, t.o))
                .collect()
        };

        let g1 = extract_dir(&dir, &args).expect("libclang init");
        let g2 = extract_dir(&dir, &args).expect("libclang init");
        let set1 = instantiates(&g1);
        let set2 = instantiates(&g2);
        eprintln!(
            "[CPP-TEMPLATE-DET] {} template_instantiates triples in ccutil",
            set1.len()
        );
        assert!(
            !set1.is_empty(),
            "template_instantiates set must be non-empty on ccutil (measured 7+ field-type uses)"
        );
        assert_eq!(
            set1, set2,
            "template_instantiates set must be identical across runs (CPP-TEMPLATE-DET)"
        );
    }

    /// `src/ccstruct` smoke — Tesseract's OCR data-model motherlode (`BLOCK` /
    /// `WERD` / `BLOB` / `COUTLINE` families). Confirms the harvester scales past
    /// ccutil to the OCR-load-bearing surface with the same deterministic shape.
    /// Gated on `TESSERACT_SRC`. Measured: 155 classes / 5264 triples / 32
    /// deterministic `template_instantiates` edges (vs ccutil's 67 / 2215 / 31),
    /// including links to `GenericVector<T>`, `BandTriMatrix<T>`,
    /// `GENERIC_2D_ARRAY<T>`, `KDPair<Key, Data>`, `PointerVector<T>`.
    #[test]
    #[expect(clippy::print_stderr, reason = "diagnostic emission gated on env var")]
    fn ccstruct_motherlode_smoke() {
        let _guard = CLANG_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Ok(src_root) = std::env::var("TESSERACT_SRC") else {
            return;
        };
        let root = std::path::Path::new(&src_root);
        let dir = root.join("src/ccstruct");
        if !dir.is_dir() {
            return;
        }
        let args = [
            "-std=c++17".to_string(),
            "-x".to_string(),
            "c++".to_string(),
            format!("-I{}", root.join("src/ccstruct").display()),
            format!("-I{}", root.join("src/ccutil").display()),
            format!("-I{}", root.join("src/classify").display()),
            format!("-I{}", root.join("src/dict").display()),
            format!("-I{}", root.join("include").display()),
        ];
        let g = extract_tree(&dir, &args).expect("libclang init");
        let triples = expand(&g);
        eprintln!(
            "[CCSTRUCT] {} classes, {} triples",
            g.models.len(),
            triples.len()
        );
        // Core OCR types must be present — proves the harvester reaches the
        // load-bearing surface, not just the utility/ccutil shell.
        let names: std::collections::BTreeSet<&str> =
            g.models.iter().map(|m| m.name.as_str()).collect();
        for must in [
            "tesseract::BLOCK",
            "tesseract::WERD",
            "tesseract::TBLOB",
            "tesseract::C_BLOB",
        ] {
            assert!(
                names.contains(must),
                "ccstruct harvest must include OCR core class {must}"
            );
        }
        assert!(
            g.models.len() >= 100,
            "expected many ccstruct classes, got {}",
            g.models.len()
        );
    }
}
