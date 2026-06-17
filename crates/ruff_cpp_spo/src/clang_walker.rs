//! The libclang translation-unit walker (feature `libclang`).
//!
//! Walks ONE C++ translation unit via the `clang` crate (libclang FFI) and
//! produces [`CppClass`] declarations in the frontend-local shape
//! [`crate::model_from_class`] unpacks into the shared `ModelGraph`.
//!
//! # Scope of this walker
//!
//! Extracts the rock-solid core the libclang high-level API exposes directly:
//! classes/structs (with namespace + nested-class qualification), base
//! specifiers (access + virtual), member fields, and methods with their
//! pure-virtual / noexcept / `override` / operator flags. This exercises the
//! `inherits_from`, `has_field`, `has_function`, `rdf:type`,
//! `virtually_overrides`, `defines_operator`, `is_pure_virtual`, and
//! `is_noexcept` predicates from real parsing.
//!
//! **Walker follow-ups** (the IR + predicates already exist from PR #8; only
//! the walker does not populate them yet): `constexpr`/`consteval` and
//! C++20 `requires` clauses (not surfaced by the high-level `clang` API —
//! need a token pass), templates (`template_specialises` /
//! `template_instantiates`), `friend` declarations, macro-expansion
//! provenance, and `static_assert`.
//!
//! # libclang at runtime
//!
//! The `clang` crate is built with `runtime` (dlopen), so no link-time
//! version coupling. If libclang is not on the default search path, set
//! `LIBCLANG_PATH` (e.g. `/usr/lib/llvm-18/lib`). [`Clang`] is a
//! process-singleton — call [`walk_tu`] sequentially, never from parallel
//! threads in the same process.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use clang::{Accessibility, Clang, Entity, EntityKind, ExceptionSpecification, Index};
use ruff_spo_triplet::{CppAccess, CppBase, CppField, CppFriend, CppMethod};

use crate::{CppClass, Declaration};

/// A failure walking a translation unit.
#[derive(Debug)]
pub enum WalkError {
    /// libclang could not be loaded (missing `libclang.so` / `LIBCLANG_PATH`),
    /// or a [`Clang`] instance already exists in this process.
    Libclang(String),
    /// The translation unit failed to parse.
    Parse(String),
}

impl fmt::Display for WalkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Libclang(m) => write!(f, "libclang unavailable: {m}"),
            Self::Parse(m) => write!(f, "translation unit parse failed: {m}"),
        }
    }
}

impl std::error::Error for WalkError {}

/// Walk one C++ translation unit at `path`, returning every class/struct
/// **definition** found (forward declarations are skipped).
///
/// `args` are passed verbatim to clang (e.g. `["-std=c++17", "-x", "c++",
/// "-I/path/to/includes"]`). Function bodies are skipped for speed — only
/// declarations are needed for SPO extraction. Parsing tolerates errors
/// (missing includes still yield a partial AST), matching how libclang is
/// used on large real corpora.
pub fn walk_tu(path: &Path, args: &[String]) -> Result<Vec<CppClass>, WalkError> {
    let clang = Clang::new().map_err(WalkError::Libclang)?;
    let index = Index::new(&clang, false, false);
    let tu = index
        .parser(path)
        .arguments(args)
        .skip_function_bodies(true)
        .parse()
        .map_err(|e| WalkError::Parse(e.to_string()))?;

    let mut out = Vec::new();
    collect_classes(&tu.get_entity(), &mut out);
    Ok(out)
}

/// Coverage instrumentation for `CPP-SCHEMA-FIT`: tally the libclang
/// `EntityKind` of every DIRECT class-body child cursor across all
/// (non-system-header) class/struct definitions in the TU.
///
/// The key is the `EntityKind` `Debug` name (e.g. `"Method"`, `"FieldDecl"`,
/// `"FriendDecl"`); the value is how many times that kind appears as a direct
/// member. The caller computes the *mapped fraction* — `BaseSpecifier` +
/// `FieldDecl` + `Method` are exactly the kinds [`build_class`] turns into a
/// [`Declaration`] today — versus the total, so a real-corpus walk shows which
/// constructs the walker silently drops (the walker-follow-up backlog:
/// `FriendDecl`, `StaticAssert`, templates, …) rather than asserting coverage.
/// Counts only meaningful cursors; access specifiers and comments are reported
/// in the histogram like everything else so the caller can classify them.
pub fn class_body_cursor_histogram(
    path: &Path,
    args: &[String],
) -> Result<BTreeMap<String, usize>, WalkError> {
    let clang = Clang::new().map_err(WalkError::Libclang)?;
    let index = Index::new(&clang, false, false);
    let tu = index
        .parser(path)
        .arguments(args)
        .skip_function_bodies(true)
        .parse()
        .map_err(|e| WalkError::Parse(e.to_string()))?;
    let mut hist = BTreeMap::new();
    tally_class_bodies(&tu.get_entity(), &mut hist);
    Ok(hist)
}

/// The kinds [`build_class`] maps to a [`Declaration`] today — the "covered"
/// set for the `CPP-SCHEMA-FIT` mapped-fraction. Kept beside the walker so the
/// coverage probe and the actual extraction can never drift apart. The
/// function-like kinds (`Method` / `Constructor` / `Destructor` /
/// `ConversionFunction` / `FunctionTemplate`) become a `has_function`;
/// `FieldDecl` + `VarDecl` (static members) become `has_field`; `FriendDecl`
/// becomes `is_friend_of`; `BaseSpecifier` becomes `inherits_from`.
pub const MAPPED_CURSOR_KINDS: [&str; 9] = [
    "BaseSpecifier",
    "FieldDecl",
    "VarDecl",
    "Method",
    "Constructor",
    "Destructor",
    "ConversionFunction",
    "FunctionTemplate",
    "FriendDecl",
];

/// Mirror of [`collect_classes`] that tallies direct class-body child kinds
/// instead of building [`CppClass`]es (same class-selection + system-header
/// filtering, so the histogram counts exactly the bodies the walker extracts).
fn tally_class_bodies(entity: &Entity, hist: &mut BTreeMap<String, usize>) {
    for child in entity.get_children() {
        match child.get_kind() {
            // Kept in lockstep with `collect_classes`: templated classes count
            // too, so the coverage histogram reflects exactly what is harvested.
            EntityKind::ClassDecl
            | EntityKind::StructDecl
            | EntityKind::ClassTemplate
            | EntityKind::ClassTemplatePartialSpecialization => {
                if child.is_definition() && !in_system_header(&child) {
                    for member in child.get_children() {
                        *hist.entry(format!("{:?}", member.get_kind())).or_insert(0) += 1;
                    }
                }
                tally_class_bodies(&child, hist);
            }
            EntityKind::Namespace => tally_class_bodies(&child, hist),
            _ => {}
        }
    }
}

/// Recurse the AST, emitting a [`CppClass`] for every class/struct
/// definition (recursing into namespaces and nested classes).
fn collect_classes(entity: &Entity, out: &mut Vec<CppClass>) {
    for child in entity.get_children() {
        match child.get_kind() {
            // Plain classes/structs AND templated classes. libclang FLATTENS a
            // template cursor — its direct children are the template params
            // (skipped by `build_class`'s `_` arm) + the members — so the same
            // `build_class` handles all four unchanged. The harvested name is the
            // bare template name (`GenericVector`, no `<T>`). Shape A: template
            // classes become classes; the template-relationship predicates
            // (`template_specialises` / `template_instantiates`) are a separate,
            // data-driven follow-up (ccutil measured 0 explicit specialisations).
            EntityKind::ClassDecl
            | EntityKind::StructDecl
            | EntityKind::ClassTemplate
            | EntityKind::ClassTemplatePartialSpecialization => {
                // Skip class definitions originating in system headers (the
                // std:: / __gnu_cxx:: machinery dragged in transitively) — an
                // SPO harvest of a project wants the project's own classes,
                // never the standard library's internals.
                if child.is_definition() && !in_system_header(&child) {
                    if let Some(cls) = build_class(&child) {
                        out.push(cls);
                    }
                }
                // Recurse for nested classes regardless of definition state.
                collect_classes(&child, out);
            }
            EntityKind::Namespace => collect_classes(&child, out),
            _ => {}
        }
    }
}

/// Build a [`CppClass`] from a class/struct definition cursor by reading its
/// DIRECT member children (bases, fields, methods). Nested class decls are
/// ignored here — [`collect_classes`] emits them separately.
fn build_class(e: &Entity) -> Option<CppClass> {
    let name = e.get_name()?;
    let namespace = enclosing_scopes(e);
    let mut declarations = Vec::new();
    for m in e.get_children() {
        match m.get_kind() {
            EntityKind::BaseSpecifier => {
                if let Some(base) = build_base(&m) {
                    declarations.push(Declaration::Base(base));
                }
            }
            // FieldDecl = a non-static data member; a VarDecl in a class body is
            // a STATIC data member (`static T x;`, libclang's distinct kind).
            // Both are data members the class HAS → has_field.
            EntityKind::FieldDecl | EntityKind::VarDecl => {
                declarations.push(Declaration::Field(CppField {
                    name: m.get_name().unwrap_or_default(),
                    type_name: m
                        .get_type()
                        .map(|t| t.get_display_name())
                        .unwrap_or_default(),
                }));
            }
            // Constructors, destructors, conversion operators, and member
            // function templates are all member FUNCTIONS that libclang reports
            // under cursor kinds distinct from `Method`; the harvester captures
            // every one as a `has_function`. CPP-SCHEMA-FIT measured 495 such
            // cursors silently dropped across ccutil when only `Method` matched
            // (the ctor/dtor coverage gap: 82% → ~90%).
            EntityKind::Method
            | EntityKind::Constructor
            | EntityKind::Destructor
            | EntityKind::ConversionFunction
            | EntityKind::FunctionTemplate => {
                declarations.push(Declaration::Method(build_method(&m)));
            }
            // `friend class Foo;` / `friend Ret fn(...);` — the befriended
            // entity. CPP-SCHEMA-FIT measured 79 in ccutil; the `is_friend_of`
            // predicate + `CppFriend` IR already exist (PR #8).
            EntityKind::FriendDecl => {
                if let Some(friend) = build_friend(&m) {
                    declarations.push(Declaration::Friend(friend));
                }
            }
            _ => {}
        }
    }
    Some(CppClass {
        namespace,
        name,
        declarations,
    })
}

/// Extract the befriended entity's name from a `friend` declaration cursor.
///
/// The befriended entity is the `FriendDecl`'s child cursor (the `FriendDecl`
/// itself is anonymous). For `friend class Foo;` the child is a `TypeRef` whose
/// referenced TYPE display is the clean fully-qualified name
/// (`Tesseract::TessdataManager`) — the cursor *spelling* would carry a
/// `class `/`struct ` elaboration, so we read the type, not the spelling. For
/// `friend Ret fn(...);` the child is the friend `FunctionDecl`, whose own name
/// is what `is_friend_of` should point to.
fn build_friend(m: &Entity) -> Option<CppFriend> {
    for child in m.get_children() {
        let name = match child.get_kind() {
            EntityKind::TypeRef => child.get_type().map(|t| t.get_display_name()),
            _ => child.get_name(),
        };
        if let Some(name) = name.filter(|s| !s.is_empty()) {
            return Some(CppFriend { name });
        }
    }
    None
}

/// Whether `e` is defined in a system header (std lib, libc, …). Entities
/// with no location (rare) are treated as project entities (kept).
fn in_system_header(e: &Entity) -> bool {
    e.get_location()
        .is_some_and(|loc| loc.is_in_system_header())
}

/// The enclosing named scopes of `e` (namespaces + outer classes),
/// outermost first — the [`CppClass::namespace`] components. The class's
/// own name is excluded.
fn enclosing_scopes(e: &Entity) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = e.get_semantic_parent();
    while let Some(p) = cur {
        if matches!(
            p.get_kind(),
            EntityKind::Namespace | EntityKind::ClassDecl | EntityKind::StructDecl
        ) {
            if let Some(n) = p.get_name() {
                parts.push(n);
            }
        }
        cur = p.get_semantic_parent();
    }
    parts.reverse();
    parts
}

/// The fully-qualified name of a class-like cursor (`Namespace::Outer::Name`).
fn qualified_name(e: &Entity) -> String {
    let mut parts = enclosing_scopes(e);
    if let Some(n) = e.get_name() {
        parts.push(n);
    }
    parts.join("::")
}

fn build_base(m: &Entity) -> Option<CppBase> {
    let ty = m.get_type()?;
    // Prefer the resolved declaration's qualified name; fall back to the
    // type's display name (e.g. for a dependent base in a template).
    let name = ty
        .get_declaration()
        .map(|d| qualified_name(&d))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| ty.get_display_name());
    let access = match m.get_accessibility() {
        Some(Accessibility::Protected) => CppAccess::Protected,
        Some(Accessibility::Private) => CppAccess::Private,
        // Public, or unreported — default to Public (the common base form).
        _ => CppAccess::Public,
    };
    Some(CppBase {
        name,
        access,
        virtual_base: m.is_virtual_base(),
    })
}

fn build_method(m: &Entity) -> CppMethod {
    let name = m.get_name().unwrap_or_default();
    let is_noexcept = matches!(
        m.get_exception_specification(),
        Some(ExceptionSpecification::BasicNoexcept | ExceptionSpecification::ComputedNoexcept)
    );
    // libclang spells operator methods `operator==`, `operator[]`, etc. Guard
    // against an ordinary method merely named `operatorFoo` by requiring the
    // char after `operator` to not start an identifier.
    let operator_kind = (name.starts_with("operator")
        && name
            .as_bytes()
            .get(8)
            .is_none_or(|b| !(b.is_ascii_alphanumeric() || *b == b'_')))
    .then(|| name.clone());
    // `override` target → the fully-qualified base method (`Base.method`), so
    // `virtually_overrides` joins the base class's own method node (PR #9).
    let overrides = m
        .get_overridden_methods()
        .and_then(|ov| ov.into_iter().next())
        .and_then(|base_m| {
            let mname = base_m.get_name()?;
            let parent = base_m.get_semantic_parent()?;
            Some(format!("{}.{mname}", qualified_name(&parent)))
        });
    CppMethod {
        name,
        is_pure_virtual: m.is_pure_virtual_method(),
        // constexpr/consteval + requires need a token pass — walker follow-up.
        constexpr_kind: None,
        is_noexcept,
        overrides,
        operator_kind,
        requires_clause: None,
    }
}
