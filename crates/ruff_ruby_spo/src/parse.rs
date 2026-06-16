//! `parse_models` — walk a Rails `app/models/` tree and produce
//! [`crate::RubyClass`] records.
//!
//! Single AST pass per file via `lib-ruby-parser`. Class discovery
//! recurses into `module ... end` so nested namespaces (e.g.
//! `module OpenProject; module Acts; class Foo`) yield the inner class.
//! STI is captured via the superclass node (passed to [`crate::walk`]).

use std::fs;
use std::path::{Path, PathBuf};

use lib_ruby_parser::{Node, Parser, ParserOptions};

use crate::RubyClass;
use crate::walk::walk_class_body;

/// Walk `<source_tree>/app/models/**/*.rb` and parse every file into
/// the [`RubyClass`] discriminated-Declaration shape.
///
/// Files that fail to parse are skipped (lib-ruby-parser still returns
/// a `ParserResult` with `ast: None` and a `Diagnostic` vec on hard
/// failures; this fn drops those silently — the coverage test
/// surfaces lost-class counts).
///
/// Returned classes are in deterministic order: file path (ASCII sort),
/// then declaration order within a file.
pub(crate) fn parse_models(source_tree: &Path) -> Vec<RubyClass> {
    let mut files: Vec<PathBuf> = collect_rb_files(source_tree.join("app/models").as_path());
    files.sort();
    let mut classes = Vec::with_capacity(files.len());
    for path in files {
        let Ok(src) = fs::read_to_string(&path) else {
            continue;
        };
        let Some(ast) = parse_file(&src, &path) else {
            continue;
        };
        collect_classes_from_node(&ast, &mut classes);
    }
    classes
}

/// Recursively collect all `*.rb` paths under `dir`. Empty if `dir`
/// doesn't exist or isn't readable.
fn collect_rb_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk_rb(dir, &mut out);
    out
}

fn walk_rb(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_rb(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rb") {
            out.push(path);
        }
    }
}

/// Test helper: parse one Ruby source string directly (no file I/O)
/// and return the resulting [`RubyClass`] records. Used by the D-AR-4
/// synthetic fixture test that exercises every routed DSL name.
pub(crate) fn parse_models_from_source_for_test(src: &str) -> Vec<RubyClass> {
    let path = PathBuf::from("<test>");
    let Some(ast) = parse_file(src, &path) else {
        return Vec::new();
    };
    let mut classes = Vec::new();
    collect_classes_from_node(&ast, &mut classes);
    classes
}

/// Parse one Ruby source file. Returns the AST root or `None` on hard
/// parse failure.
fn parse_file(src: &str, path: &Path) -> Option<Node> {
    let options = ParserOptions {
        buffer_name: path.display().to_string(),
        ..Default::default()
    };
    let parser = Parser::new(src.as_bytes().to_vec(), options);
    parser.do_parse().ast.map(|boxed| *boxed)
}

/// Walk an AST node and emit a [`RubyClass`] for every `class X < Y`
/// discovered, recursing into `module ... end` wrappers and `Begin`
/// statement blocks.
fn collect_classes_from_node(node: &Node, out: &mut Vec<RubyClass>) {
    collect_classes_with_namespace(node, &[], out);
}

/// Recursive class-discovery walk that threads the enclosing module
/// namespace through so `module Foo; class Bar < ApplicationRecord;`
/// yields `name = "Foo::Bar"` (codex P2 r3418* — module namespaces
/// were being dropped on the floor).
fn collect_classes_with_namespace(node: &Node, ns: &[String], out: &mut Vec<RubyClass>) {
    match node {
        Node::Begin(b) => {
            for stmt in &b.statements {
                collect_classes_with_namespace(stmt, ns, out);
            }
        }
        Node::Module(m) => {
            let mod_name = const_to_string(&m.name).unwrap_or_default();
            let mut nested = ns.to_vec();
            nested.push(mod_name);
            if let Some(body) = &m.body {
                collect_classes_with_namespace(body, &nested, out);
            }
        }
        Node::Class(c) => {
            let local_name = const_to_string(&c.name).unwrap_or_default();
            // Qualify with the enclosing `module Foo; module Bar; class …`
            // namespace stack so two same-named inner classes don't
            // collide in the SPO graph.
            let qualified = if ns.is_empty() {
                local_name
            } else {
                format!("{}::{local_name}", ns.join("::"))
            };
            let mut class = RubyClass {
                name: qualified,
                declarations: Vec::new(),
            };
            // STI parent is the explicit superclass when it isn't
            // ApplicationRecord / ActiveRecord::Base / a synthetic root.
            if let Some(super_node) = &c.superclass {
                let parent = const_to_string(super_node).unwrap_or_default();
                if is_sti_parent(&parent) {
                    class
                        .declarations
                        .push(crate::Declaration::Sti(ruff_spo_triplet::StiInfo {
                            inherits_from: Some(parent),
                            abstract_class: false,
                            inheritance_column: None,
                        }));
                }
            }
            if let Some(body) = &c.body {
                walk_class_body(body, &mut class.declarations);
            }
            out.push(class);
            // A nested class inside a class body is unusual but possible
            // (`class Outer; class Inner; end; end`); the inner one was
            // walked as part of body — collect_classes_with_namespace
            // handles it via the body's Class node arm.
            if let Some(body) = &c.body {
                collect_nested_classes(body, ns, out);
            }
        }
        _ => {}
    }
}

/// Walk a class body for *nested* classes (rare in models but legal —
/// `class A; class B; end; end`). The walker treats these as siblings
/// at IR level (flat `ModelGraph::models`), matching what the SPO store
/// expects.
fn collect_nested_classes(body: &Node, ns: &[String], out: &mut Vec<RubyClass>) {
    match body {
        Node::Begin(b) => {
            for stmt in &b.statements {
                if matches!(stmt, Node::Class(_) | Node::Module(_)) {
                    collect_classes_with_namespace(stmt, ns, out);
                }
            }
        }
        Node::Class(_) | Node::Module(_) => {
            collect_classes_with_namespace(body, ns, out);
        }
        _ => {}
    }
}

/// Render a `Const` node chain to a dotted (`::`) Ruby constant string.
///
/// `Const { scope: Some(Const{name:"A"}), name: "B" }` → `"A::B"`. The
/// chain bottoms out at a `Const` with `scope: None` or `Cbase`.
fn const_to_string(node: &Node) -> Option<String> {
    match node {
        Node::Const(c) => {
            let suffix = c.name.clone();
            if let Some(scope) = &c.scope {
                if let Node::Cbase(_) = **scope {
                    Some(format!("::{suffix}"))
                } else if let Some(prefix) = const_to_string(scope) {
                    Some(format!("{prefix}::{suffix}"))
                } else {
                    Some(suffix)
                }
            } else {
                Some(suffix)
            }
        }
        _ => None,
    }
}

/// STI parent test: an explicit superclass that isn't the canonical
/// `ActiveRecord` root counts as the STI parent for [`crate::Declaration::Sti`].
fn is_sti_parent(parent: &str) -> bool {
    !matches!(
        parent,
        "ApplicationRecord"
            | "ActiveRecord::Base"
            | "::ActiveRecord::Base"
            | "Object"
            | "::Object"
            | ""
    )
}
