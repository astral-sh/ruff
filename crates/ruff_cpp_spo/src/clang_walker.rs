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

use std::fmt;
use std::path::Path;

use clang::{Accessibility, Clang, Entity, EntityKind, ExceptionSpecification, Index};
use ruff_spo_triplet::{CppAccess, CppBase, CppField, CppMethod};

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

/// Recurse the AST, emitting a [`CppClass`] for every class/struct
/// definition (recursing into namespaces and nested classes).
fn collect_classes(entity: &Entity, out: &mut Vec<CppClass>) {
    for child in entity.get_children() {
        match child.get_kind() {
            EntityKind::ClassDecl | EntityKind::StructDecl => {
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
            EntityKind::FieldDecl => {
                declarations.push(Declaration::Field(CppField {
                    name: m.get_name().unwrap_or_default(),
                    type_name: m
                        .get_type()
                        .map(|t| t.get_display_name())
                        .unwrap_or_default(),
                }));
            }
            EntityKind::Method => {
                declarations.push(Declaration::Method(build_method(&m)));
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
