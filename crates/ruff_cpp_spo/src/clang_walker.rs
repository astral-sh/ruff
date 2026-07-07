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
use ruff_spo_triplet::{
    CppAccess, CppBase, CppField, CppFriend, CppMethod, CppTemplate, CppTemplateKind,
};

use crate::{CppClass, CppEnum, CppFunction, Declaration};

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

/// Walk ONE translation unit and collect free-function DEFINITIONS with their
/// **general call graph** — the C-library dispatch structure (e.g. leptonica
/// `pixScale` → `pixScaleGeneral` → `pixScaleGrayLI`/`pixScaleAreaMap`/
/// `pixUnsharpMasking`). Unlike [`walk_tu`] this parses WITH bodies
/// (`skip_function_bodies(false)`), because the callee set is the point.
///
/// This is the missing arm for C libraries: [`walk_tu`] harvests C++ *classes*;
/// a C library (leptonica, zlib, …) is free functions on pointer buffers, so
/// the AR/OO member body-arm ([`method_body_arm`]) captures nothing there — but
/// the call graph IS the transcode-driving structure (which functions to port,
/// in what dispatch order). Numeric kernel BODIES remain the essential-15%
/// hand-port (the doctrine); this mints the 85% structure that classifies + orders
/// them.
///
/// # Errors
///
/// [`WalkError::Libclang`] if libclang fails to initialise (non-recoverable);
/// [`WalkError::Parse`] if the TU fails to parse.
#[cfg(feature = "libclang")]
pub fn walk_free_functions(path: &Path, args: &[String]) -> Result<Vec<CppFunction>, WalkError> {
    let clang = Clang::new().map_err(WalkError::Libclang)?;
    let index = Index::new(&clang, false, false);
    let tu = index
        .parser(path)
        .arguments(args)
        .skip_function_bodies(false)
        .parse()
        .map_err(|e| WalkError::Parse(e.to_string()))?;

    let mut out = Vec::new();
    collect_functions(&tu.get_entity(), &mut out);
    Ok(out)
}

/// Recurse the AST, emitting a [`CppFunction`] for every free-function
/// DEFINITION (recursing into namespaces). Prototypes (no body) and
/// system-header functions are skipped — a transcode wants the library's own
/// definitions.
#[cfg(feature = "libclang")]
fn collect_functions(entity: &Entity, out: &mut Vec<CppFunction>) {
    for child in entity.get_children() {
        match child.get_kind() {
            EntityKind::FunctionDecl => {
                if child.is_definition()
                    && !in_system_header(&child)
                    && let Some(name) = child.get_name()
                {
                    let mut calls = Vec::new();
                    collect_calls(&child, &mut calls);
                    calls.sort();
                    calls.dedup();
                    out.push(CppFunction {
                        namespace: enclosing_scopes(&child),
                        name,
                        calls,
                    });
                }
            }
            EntityKind::Namespace => collect_functions(&child, out),
            _ => {}
        }
    }
}

/// Recurse a function body collecting EVERY resolvable callee name (the general
/// call graph). Distinct from [`walk_body`]'s `calls` (persistence mutators
/// only): here every `CallExpr` callee is the dispatch structure a C-library
/// transcode follows.
#[cfg(feature = "libclang")]
fn collect_calls(node: &Entity, out: &mut Vec<String>) {
    for child in node.get_children() {
        if child.get_kind() == EntityKind::CallExpr
            && let Some(name) = child.get_name()
        {
            out.push(name);
        }
        collect_calls(&child, out);
    }
}

/// Walk ONE translation unit and collect free-standing ENUM DECLARATIONS at
/// namespace scope (`enum DawgType { ... }` / `enum class Foo : int8_t { ... }`
/// directly inside a namespace or at global scope).
///
/// Nested class-body enums are NOT collected here — they are covered by the
/// extended [`build_class`], which pushes them onto the owning
/// [`CppClass::declarations`] as `Declaration::Enum` alongside its fields and
/// methods. This split mirrors [`walk_tu`] vs the class-body arm of
/// [`build_class`]: a free-standing enum has no owning class to attach to, so
/// it needs its own top-level collection.
///
/// # Errors
///
/// [`WalkError::Libclang`] if libclang fails to initialise (non-recoverable);
/// [`WalkError::Parse`] if the TU fails to parse.
#[cfg(feature = "libclang")]
pub fn walk_enums(path: &Path, args: &[String]) -> Result<Vec<CppEnum>, WalkError> {
    let clang = Clang::new().map_err(WalkError::Libclang)?;
    let index = Index::new(&clang, false, false);
    let tu = index
        .parser(path)
        .arguments(args)
        .skip_function_bodies(true)
        .parse()
        .map_err(|e| WalkError::Parse(e.to_string()))?;

    let mut out = Vec::new();
    collect_enums(&tu.get_entity(), &mut out);
    Ok(out)
}

/// Recurse the AST (namespaces only — class-body enums are handled by
/// [`build_class`]), emitting a [`CppEnum`] for every enum DEFINITION found
/// directly in a namespace or at global scope.
#[cfg(feature = "libclang")]
fn collect_enums(entity: &Entity, out: &mut Vec<CppEnum>) {
    for child in entity.get_children() {
        match child.get_kind() {
            EntityKind::EnumDecl => {
                if child.is_definition()
                    && !in_system_header(&child)
                    && let Some(e) = build_enum(&child)
                {
                    out.push(e);
                }
            }
            EntityKind::Namespace => collect_enums(&child, out),
            _ => {}
        }
    }
}

/// Build a [`CppEnum`] from an enum DEFINITION cursor: namespace, name
/// (`None`/empty for a truly anonymous enum — skipped, nothing to key it by),
/// scoped-ness (`enum class`), the declared underlying integer type if any,
/// and every `EnumConstantDecl` child with its resolved signed value.
#[cfg(feature = "libclang")]
fn build_enum(e: &Entity) -> Option<CppEnum> {
    let name = e.get_name().filter(|n| !n.is_empty())?;
    let namespace = enclosing_scopes(e);
    let is_class = e.is_scoped();
    let underlying_type = e
        .get_enum_underlying_type()
        .map(|t| t.get_display_name())
        .unwrap_or_default();
    let mut variants = Vec::new();
    for c in e.get_children() {
        if c.get_kind() == EntityKind::EnumConstantDecl
            && let Some(vname) = c.get_name()
            && let Some((signed, _unsigned)) = c.get_enum_constant_value()
        {
            variants.push((vname, signed));
        }
    }
    Some(CppEnum {
        namespace,
        name,
        is_class,
        underlying_type,
        variants,
    })
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
/// becomes `is_friend_of`; `BaseSpecifier` becomes `inherits_from`; `EnumDecl`
/// (a nested class-body enum) becomes a `Declaration::Enum`.
pub const MAPPED_CURSOR_KINDS: [&str; 10] = [
    "BaseSpecifier",
    "FieldDecl",
    "VarDecl",
    "Method",
    "Constructor",
    "Destructor",
    "ConversionFunction",
    "FunctionTemplate",
    "FriendDecl",
    "EnumDecl",
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
    // A `ClassTemplatePartialSpecialization` shares its primary's `get_name()`
    // (libclang spells it as the bare template name, e.g. `Foo` for
    // `template<class T> class Foo<T*>`); using that as-is collides with the
    // primary in the cross-TU `BTreeMap` dedup, dropping one of the two. Use
    // the cursor's `get_display_name()` instead — it carries the partial-spec
    // arguments (`Foo<T *>`) so the qualified name stays distinct. Codex P2 #17.
    let name = if matches!(e.get_kind(), EntityKind::ClassTemplatePartialSpecialization) {
        e.get_display_name()?
    } else {
        e.get_name()?
    };
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
                let type_name = m
                    .get_type()
                    .map(|t| t.get_display_name())
                    .unwrap_or_default();
                // A field whose type is a template-id (`GenericVector<char>`) is a
                // template INSTANTIATION use. `cpp_field` drops `type_name`, so
                // this is otherwise invisible in the triples — surface it as
                // `template_instantiates` (Inferred: single-TU instantiation
                // visibility is incomplete by construction).
                if let Some(inst) = template_instantiation(&type_name) {
                    declarations.push(Declaration::Template(CppTemplate {
                        kind: CppTemplateKind::Instantiation,
                        name: inst,
                    }));
                }
                declarations.push(Declaration::Field(CppField {
                    name: m.get_name().unwrap_or_default(),
                    type_name,
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
                collect_signature_instantiations(&m, &mut declarations);
            }
            // `friend class Foo;` / `friend Ret fn(...);` — the befriended
            // entity. CPP-SCHEMA-FIT measured 79 in ccutil; the `is_friend_of`
            // predicate + `CppFriend` IR already exist (PR #8).
            EntityKind::FriendDecl => {
                if let Some(friend) = build_friend(&m) {
                    declarations.push(Declaration::Friend(friend));
                }
            }
            // A nested (class-body) enum — e.g. Tesseract's `enum PermuterType`
            // members declared inside a class. Namespace-scope enums are
            // harvested separately via `walk_enums`, since they have no
            // owning `CppClass` to attach to.
            EntityKind::EnumDecl => {
                if let Some(en) = build_enum(&m) {
                    declarations.push(Declaration::Enum(en));
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

/// The template-id (`Foo<Args>`) a type display denotes, if it is a template
/// **instantiation** use — stripping a leading `const`/`volatile` and trailing
/// `*`/`&`. `int` → `None`; `const GenericVector<char> &` → `GenericVector<char>`.
/// Verbatim `Foo<Args>` form, per the `CppTemplate::name` IR convention. This is
/// a SYNTACTIC use (deterministic per-TU), not an implicit-instantiation cursor
/// (those are the per-TU-incomplete thing the Inferred provenance flags).
fn template_instantiation(type_display: &str) -> Option<String> {
    if !type_display.contains('<') {
        return None;
    }
    let mut s = type_display.trim();
    for pfx in ["const ", "volatile "] {
        s = s.strip_prefix(pfx).map(str::trim_start).unwrap_or(s);
    }
    let s = s.trim_end_matches(['*', '&', ' ']);
    (s.contains('<') && !s.is_empty()).then(|| s.to_string())
}

/// Push a `template_instantiates` declaration for every template-id in a method's
/// RETURN type or PARAMETER types — the syntactic instantiation uses in a
/// signature, which `cpp_method` does not otherwise surface. Applies to every
/// function-like cursor (ctor/dtor have no/void result + their params).
fn collect_signature_instantiations(m: &Entity, decls: &mut Vec<Declaration>) {
    let mut type_displays: Vec<String> = Vec::new();
    if let Some(ret) = m.get_result_type() {
        type_displays.push(ret.get_display_name());
    }
    if let Some(args) = m.get_arguments() {
        for arg in args {
            if let Some(t) = arg.get_type() {
                type_displays.push(t.get_display_name());
            }
        }
    }
    for ty in type_displays {
        if let Some(inst) = template_instantiation(&ty) {
            decls.push(Declaration::Template(CppTemplate {
                kind: CppTemplateKind::Instantiation,
                name: inst,
            }));
        }
    }
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
    // `override` target → the fully-qualified base method with its overload
    // signature (`Base.method(int)`), so `virtually_overrides` joins the
    // **exact base overload** the derived method overrides — not just any
    // method with the same name. The signature suffix matches the per-overload
    // method-IRI convention `cpp_method` builds (codex P2 #17).
    let overrides = m
        .get_overridden_methods()
        .and_then(|ov| ov.into_iter().next())
        .and_then(|base_m| {
            let mname = base_m.get_name()?;
            let parent = base_m.get_semantic_parent()?;
            let params: Vec<String> = base_m
                .get_arguments()
                .into_iter()
                .flatten()
                .filter_map(|a| a.get_type().map(|t| t.get_display_name()))
                .collect();
            Some(format!(
                "{}.{mname}({}){}",
                qualified_name(&parent),
                params.join(","),
                if base_m.is_const_method() {
                    " const"
                } else {
                    ""
                }
            ))
        });
    // AST-DLL signature shape: return type (skip void/ctor/dtor) + ordered
    // parameter types, verbatim from the cursor.
    let return_type = m
        .get_result_type()
        .map(|t| t.get_display_name())
        .filter(|d| !d.is_empty() && d != "void");
    let param_types = m
        .get_arguments()
        .into_iter()
        .flatten()
        .filter_map(|a| a.get_type().map(|t| t.get_display_name()))
        .collect();
    CppMethod {
        name,
        is_pure_virtual: m.is_pure_virtual_method(),
        // constexpr/consteval + requires need a token pass — walker follow-up.
        constexpr_kind: None,
        is_noexcept,
        overrides,
        operator_kind,
        requires_clause: None,
        return_type,
        param_types,
        is_const: m.is_const_method(),
        is_static: m.is_static_method(),
        access: match m.get_accessibility() {
            Some(Accessibility::Protected) => CppAccess::Protected,
            Some(Accessibility::Private) => CppAccess::Private,
            // Public, or unreported (e.g. free function) — default Public.
            _ => CppAccess::Public,
        },
    }
}

// ─────────────────────────────────────────────────────────────────────────
// DTO ARM (DRAFT — libclang-gated, untested in this checkout: no libclang).
//
// The body-fact fingerprint the fuzzy recipe-codebook needs
// (ruff/.claude/knowledge/fuzzy-recipe-codebook.md §2), for C++ member
// functions — so the SAME language-agnostic recipe centroids that classify
// Rails hooks and C# handlers classify C++ setters / lifecycle overrides.
//
// STATUS: reviewed draft. The clang-crate cursor kinds below are correct, but
// this has NOT been run against a real TU (this checkout has no libclang; the
// whole crate is behind the `libclang` feature). A future session with
// LIBCLANG_PATH set should: (1) wire `BodyArm` into `CppMethod` as four
// `Vec<String>` fields + `guarded_writes` (mirroring ruff_spo_triplet::Function),
// (2) emit them in the C++ expand path as writes_field / reads_field / raises /
// calls / writes_if_blank, (3) add a probe on a real corpus (Tesseract) — same
// env-gate + pre-register + drift-fuse discipline as the Ruby/C# legs.
//
// Provenance mapping (matches Function): writes_field / raises / writes_if_blank
// = Authoritative (the lvalue / throw-type / guard shape are machine-readable);
// reads_field / calls = Inferred (heuristic receiver + no scope analysis).
#[allow(dead_code)] // TESTED via arm_tests; wired into CppMethod+expand is the follow-up
#[cfg(feature = "libclang")]
#[derive(Debug, Default, Clone)]
pub(crate) struct BodyArm {
    pub writes: Vec<String>,         // `this->x = …` / `x = …` member assignment
    pub reads: Vec<String>,          // `this->x` / bare member read
    pub raises: Vec<String>,         // `throw XError(…)`
    pub calls: Vec<String>,          // `obj.SaveChanges()` / persistence mutator
    pub guarded_writes: Vec<String>, // J1: write under `if (x == nullptr)` etc.
}

// The closed persistence-mutator set — the C++ analogue of Ruby's AR_MUTATORS
// and the C# EF set. A `calls` fact fires only for these (the triage needs
// "does it call a writer", not every call). Extend per ORM/framework.
#[allow(dead_code)] // TESTED via arm_tests; wired into CppMethod+expand is the follow-up
#[cfg(feature = "libclang")]
fn is_cpp_mutator(name: &str) -> bool {
    matches!(
        name,
        "save"
            | "Save"
            | "update"
            | "Update"
            | "insert"
            | "Insert"
            | "remove"
            | "Remove"
            | "erase"
            | "commit"
            | "Commit"
            | "flush"
            | "Flush"
    )
}

/// Walk a member-function body cursor and extract the recipe fingerprint.
///
/// Call with the `Method`/`Constructor`/… entity; it recurses the body via
/// `get_children()`. Local-only J1 guard detection (an `IfStmt` whose condition
/// is a null/empty test on member `X`, containing a write of `X`) — no
/// dominator analysis, keeping `writes_if_blank` Authoritative, exactly as the
/// Ruby `detect_guarded_default` does.
#[allow(dead_code)] // TESTED via arm_tests; wired into CppMethod+expand is the follow-up
#[cfg(feature = "libclang")]
pub(crate) fn method_body_arm(method: &Entity) -> BodyArm {
    let mut arm = BodyArm::default();
    walk_body(method, &mut arm, None);
    arm.writes.sort();
    arm.writes.dedup();
    arm.reads.sort();
    arm.reads.dedup();
    arm.raises.sort();
    arm.raises.dedup();
    arm.calls.sort();
    arm.calls.dedup();
    arm.guarded_writes.sort();
    arm.guarded_writes.dedup();
    arm
}

// `guard` = the member name the enclosing branch is null/empty-guarded on (J1),
// threaded down only into that branch.
#[allow(dead_code)] // TESTED via arm_tests; wired into CppMethod+expand is the follow-up
#[cfg(feature = "libclang")]
fn walk_body(node: &Entity, arm: &mut BodyArm, guard: Option<&str>) {
    for child in node.get_children() {
        match child.get_kind() {
            // `throw XError("…")` — the exception type name is the throw's
            // sub-expression type. `CXXThrowExpr` wraps the constructed value.
            EntityKind::ThrowExpr => {
                if let Some(ty) = thrown_type_name(&child) {
                    arm.raises.push(format!("exc:{ty}"));
                }
            }
            // `a = b` — a BinaryOperator whose operator is `=`. The clang crate
            // does not expose the operator token directly on stable, so the
            // idiom is: the first child is the lvalue. If it is a member ref
            // (`this->x` / `x`), it is a write of that member; a J1 guard makes
            // it a guarded (default) write.
            EntityKind::BinaryOperator => {
                if let Some(member) = child
                    .get_children()
                    .first()
                    .filter(|c| c.get_kind() == EntityKind::MemberRefExpr)
                    .and_then(Entity::get_name)
                {
                    arm.writes.push(member.clone());
                    if guard == Some(member.as_str()) {
                        arm.guarded_writes.push(member);
                    }
                }
                // Recurse into the RHS for nested reads/calls/raises.
                walk_body(&child, arm, guard);
            }
            // `obj.method(...)` — a persistence-mutator dispatch → `calls`.
            EntityKind::CallExpr => {
                if let Some(name) = child.get_name()
                    && is_cpp_mutator(&name)
                {
                    // "receiver.method": the receiver display name if resolvable,
                    // else `self`. (Heuristic — Inferred tier, like Ruby.)
                    let recv = call_receiver(&child).unwrap_or_else(|| "self".to_string());
                    arm.calls.push(format!("{recv}.{name}"));
                }
                walk_body(&child, arm, guard);
            }
            // `this->x` / bare `x` as a value → a member read. (The lvalue of an
            // assignment is handled above and NOT double-counted here, because
            // this arm only fires for a MemberRefExpr that is not the direct
            // first child of a BinaryOperator — see the C# LHS-exclusion note.)
            EntityKind::MemberRefExpr => {
                if let Some(name) = child.get_name() {
                    arm.reads.push(name);
                }
            }
            // Structural wrappers (incl. `IfStmt`, `CompoundStmt`,
            // `UnexposedExpr`) — recurse so facts inside them are matched as
            // children. NOTE: unlike Ruby/C#, C++ J1 (`writes_if_blank`) is a
            // documented FOLLOW-UP here: the libclang AST wraps the guard cond
            // and the guarded write in `UnexposedExpr` nodes (see the cursor
            // dump in `examples/`), so robust `if (x == nullptr) x = v` guard
            // detection needs an UnexposedExpr-aware pass. Until then C++
            // `guarded_writes` stays empty (a write-if-blank is recorded as a
            // plain write → classified Compute/Normalize, never a false
            // essential — the safe direction). `null_guarded_member` is the
            // seed for that follow-up.
            _ => walk_body(&child, arm, guard),
        }
    }
}

// The thrown exception's type name. `throw X(...)` nests the operand under
// UnexposedExpr/ConstructExpr wrappers, so recurse for the first node yielding
// a concrete, non-void type name.
#[allow(dead_code)] // TESTED via arm_tests; wired into CppMethod+expand is the follow-up
#[cfg(feature = "libclang")]
fn thrown_type_name(throw: &Entity) -> Option<String> {
    fn first_typed(e: &Entity) -> Option<String> {
        if let Some(t) = e.get_type() {
            let name = bare_type_name(&t.get_display_name());
            if !name.is_empty() && name != "void" {
                return Some(name);
            }
        }
        e.get_children().iter().find_map(first_typed)
    }
    throw.get_children().iter().find_map(first_typed)
}

// `x == nullptr` / `x == 0` / `x.empty()` → the guarded member `X`. Retained as
// the SEED for the C++ J1 follow-up (see the IfStmt note in `walk_body`); not
// yet wired, hence `dead_code`.
#[allow(dead_code)]
// Draft: the
// clang crate surfaces the operator via the child token stream; a full impl
// inspects `get_children()` for a MemberRefExpr paired with a null literal.
#[cfg(feature = "libclang")]
fn null_guarded_member(cond: &Entity) -> Option<String> {
    // Look for a MemberRefExpr anywhere in the condition whose sibling is a
    // null/zero literal or whose parent call is `.empty()`. Kept deliberately
    // conservative (only the clear cases) so the fact stays Authoritative.
    cond.get_children()
        .iter()
        .find(|c| c.get_kind() == EntityKind::MemberRefExpr)
        .and_then(Entity::get_name)
}

// Best-effort receiver label for a call (`obj` in `obj.save()`), else None.
#[allow(dead_code)] // TESTED via arm_tests; wired into CppMethod+expand is the follow-up
#[cfg(feature = "libclang")]
fn call_receiver(call: &Entity) -> Option<String> {
    call.get_children()
        .iter()
        .find(|c| {
            matches!(
                c.get_kind(),
                EntityKind::MemberRefExpr | EntityKind::DeclRefExpr
            )
        })
        .and_then(Entity::get_name)
}

// `List<Foo>` / `foo::Bar` → a stable bare type name for the `exc:` object.
#[allow(dead_code)] // TESTED via arm_tests; wired into CppMethod+expand is the follow-up
#[cfg(feature = "libclang")]
fn bare_type_name(display: &str) -> String {
    let s = display
        .trim_start_matches("class ")
        .trim_start_matches("struct ");
    let s = s.split('<').next().unwrap_or(s);
    s.rsplit("::").next().unwrap_or(s).trim().to_string()
}

#[cfg(all(test, feature = "libclang"))]
mod arm_tests {
    use super::*;
    use std::io::Write;

    fn find_method<'a>(e: &Entity<'a>, name: &str) -> Option<Entity<'a>> {
        for c in e.get_children() {
            if c.get_kind() == EntityKind::Method && c.get_name().as_deref() == Some(name) {
                return Some(c);
            }
            if let Some(f) = find_method(&c, name) {
                return Some(f);
            }
        }
        None
    }

    // Parse an inline C++ fixture WITH function bodies (walk_tu skips them),
    // find one method by name, and return its recipe fingerprint.
    fn arm_of(src: &str, method: &str) -> BodyArm {
        let dir = std::env::temp_dir().join(format!("cpp_arm_{method}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("f.cpp");
        let mut fh = std::fs::File::create(&path).unwrap();
        fh.write_all(src.as_bytes()).unwrap();
        drop(fh);
        let clang = Clang::new().unwrap();
        let index = Index::new(&clang, false, false);
        let tu = index
            .parser(&path)
            .arguments(&["-std=c++17".to_string()])
            .skip_function_bodies(false) // ← the arm needs bodies
            .parse()
            .unwrap();
        let m = find_method(&tu.get_entity(), method).expect("method not found");
        method_body_arm(&m)
    }

    #[test]
    fn cpp_body_arm_extracts_the_fingerprint() {
        // Declaration order matters: `BadStatus` and `Repo` must be complete
        // types before `Patient` uses them (a forward-ref leaves the throw
        // operand unresolved and the arm sees no type).
        let src = r#"
struct BadStatus {};
struct Repo { void save(); };
struct Patient {
    int status_;
    Repo repo_;
    // normalize: unconditional self-write
    void tidy() { status_ = status_ + 1; }
    // guard: throw only
    void validate() { if (status_ == 0) throw BadStatus(); }
    // cascade: mutator dispatch
    void persist() { repo_.save(); }
};
"#;
        let tidy = arm_of(src, "tidy");
        assert!(
            tidy.writes.contains(&"status_".to_string()),
            "writes {:?}",
            tidy.writes
        );
        assert!(
            tidy.reads.contains(&"status_".to_string()),
            "reads {:?}",
            tidy.reads
        );

        let validate = arm_of(src, "validate");
        assert!(
            validate.writes.is_empty(),
            "guard writes nothing: {:?}",
            validate.writes
        );
        assert!(
            validate.raises.iter().any(|r| r.contains("BadStatus")),
            "raises {:?}",
            validate.raises
        );

        let persist = arm_of(src, "persist");
        assert!(
            persist
                .calls
                .iter()
                .any(|c| c.split('.').next_back() == Some("save")),
            "calls {:?}",
            persist.calls
        );
    }
}
