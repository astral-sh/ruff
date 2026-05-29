//! `ruff_ruby_spo` — **SCAFFOLD** Ruby/Rails frontend for the shared SPO
//! triplet core.
//!
//! This crate exists to be *finished*, not to work yet. It pins the target
//! triple shape (via a passing test) and marks every place a real Ruby
//! parser must plug in with a `todo!()` and a doc-comment naming the exact
//! Rails construct to read.
//!
//! # How to finish it
//!
//! See `crates/ruff_spo_triplet/SPO_TRIPLET_EXTRACTION.md` §5–§6 for the
//! full guide. In short:
//!
//! 1. Add a Ruby parser dep (recommended `lib-ruby-parser`, pure Rust).
//! 2. Replace the `todo!()` in [`parse_models`] to produce a `Vec<RubyClass>`.
//! 3. Replace the `todo!()`s in [`extract_fields`] / [`extract_functions`]
//!    to read the Rails constructs documented on each.
//! 4. Run the locked-shape test after each step — it asserts the
//!    `expand()` output for a hand-built `ModelGraph`, so it tells you when
//!    your extraction produces the right shape.
//! 5. Point [`extract`] at an OpenProject `app/models/` tree.
//!
//! The downstream consumers (`lance_graph` SPO loader, `action_emitter`,
//! `link_chain`) need ZERO changes — they already consume the triple shape
//! this crate targets.

use std::path::Path;

use ruff_spo_triplet::{Field, Function, Model, ModelGraph};

/// The namespace prefix for OpenProject subjects/objects.
pub const NAMESPACE: &str = "openproject";

/// A minimally-parsed Ruby class — what a parser frontend should produce
/// before the IR mapping. Kept tiny on purpose; expand it as the real
/// extractor needs more raw material.
#[derive(Debug, Clone, Default)]
pub struct RubyClass {
    /// Class name as written (`WorkPackage`). No dots in Ruby class names,
    /// so no normalisation needed (unlike Odoo's `account.move`).
    pub name: String,
    /// Raw source of the class body — the extractors scan this.
    pub body_source: String,
    /// Association names declared on the class (`belongs_to`, `has_many`,
    /// `has_one`, `has_and_belongs_to_many`). Seeds the set of valid
    /// relation names so a body call can be told apart from an ordinary
    /// method call.
    pub associations: Vec<String>,
}

/// Top-level entry: walk a Rails `app/models/` tree and produce the IR.
///
/// # Panics
///
/// Currently `todo!()` — wire [`parse_models`] first.
#[must_use]
pub fn extract(source_tree: &Path) -> ModelGraph {
    let classes = parse_models(source_tree);
    let mut graph = ModelGraph::new(NAMESPACE);
    for class in &classes {
        let mut model = Model::new(&class.name);
        model.fields = extract_fields(class);
        model.functions = extract_functions(class);
        graph.models.push(model);
    }
    graph
}

/// Parse every `*.rb` under `app/models/` into [`RubyClass`] records.
///
/// # What to wire
///
/// Use `lib-ruby-parser` (or tree-sitter-ruby) to:
/// - find each `class X < ApplicationRecord` (and STI subclasses),
/// - capture its body source range,
/// - collect `belongs_to` / `has_many` / `has_one` /
///   `has_and_belongs_to_many` argument symbols into `associations`.
///
/// Also parse `db/schema.rb` once to map each table → its columns; those
/// columns are the baseline [`Field`]s (the `attribute`/`attr_accessor`
/// declarations in the class body are additive).
fn parse_models(_source_tree: &Path) -> Vec<RubyClass> {
    todo!(
        "wire a Ruby parser (lib-ruby-parser): collect class defs + \
         association declarations + body source; seed columns from db/schema.rb"
    )
}

/// Extract [`Field`]s from a class.
///
/// # What to wire (Rails → IR)
///
/// - **`Field::name`**: DB columns (from `schema.rb`), plus `attribute :x`,
///   `attr_accessor :x`, `store_accessor :data, :x` in the class body.
/// - **`Field::emitted_by`**: a memoized/derived method that assigns the
///   attribute — `def total_hours; @total_hours ||= compute…; end`. The
///   method name is the `emitted_by` target.
/// - **`Field::depends_on`**: the association/attribute chains that derived
///   attribute's method reads (`time_entries.hours`). Emit dotted paths
///   verbatim — the downstream `link_chain` splitter resolves the hops.
fn extract_fields(_class: &RubyClass) -> Vec<Field> {
    todo!(
        "read schema.rb columns + attribute/attr_accessor/store_accessor; \
         link derived attrs to their computing method (emitted_by) and that \
         method's read chains (depends_on)"
    )
}

/// Extract [`Function`]s from a class.
///
/// # What to wire (Rails → IR)
///
/// - **`Function::name`**: each `def method_name` in the class body
///   (instance methods; include callback targets referenced by
///   `before_save`/`after_create`/etc.).
/// - **`Function::reads`**: `self.x` reads and bare attribute reads in the
///   method body (Inferred tier).
/// - **`Function::raises`**: `raise X`, `errors.add(...)`, and declarative
///   `validates`/`validate` (Authoritative). Map declarative validations to
///   `raises ActiveRecord::RecordInvalid` — see the guide §5 step 2.
/// - **`Function::traverses`**: calls in the body whose receiver/name is one
///   of `class.associations` (`time_entries.each`, `project.members`). The
///   association name is the relation (Inferred).
fn extract_functions(_class: &RubyClass) -> Vec<Function> {
    todo!(
        "read def bodies: attribute reads (reads), raise/errors.add/validates \
         (raises exc:…), and association walks restricted to class.associations \
         (traverses_relation)"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_spo_triplet::expand;

    /// Locked target shape: a hand-built `ModelGraph` matching what a
    /// finished `extract()` MUST produce for a tiny OpenProject-like model.
    /// This test passes today (it does not call the `todo!()` extractors);
    /// it tells the frontend author what "done" looks like.
    fn locked_work_package_graph() -> ModelGraph {
        let mut graph = ModelGraph::new(NAMESPACE);
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
        graph
    }

    #[test]
    fn locked_shape_expands_to_expected_triples() {
        let triples = expand(&locked_work_package_graph());
        let has =
            |s: &str, p: &str, o: &str| triples.iter().any(|t| t.s == s && t.p == p && t.o == o);

        // ObjectType / Property / Function classification.
        assert!(has(
            "openproject:WorkPackage",
            "rdf:type",
            "ogit:ObjectType"
        ));
        assert!(has(
            "openproject:WorkPackage.total_hours",
            "rdf:type",
            "ogit:Property"
        ));
        assert!(has(
            "openproject:WorkPackage.compute_total_hours",
            "rdf:type",
            "ogit:Function"
        ));
        // Compute graph edges.
        assert!(has(
            "openproject:WorkPackage.total_hours",
            "emitted_by",
            "openproject:WorkPackage.compute_total_hours"
        ));
        assert!(has(
            "openproject:WorkPackage.total_hours",
            "depends_on",
            "openproject:WorkPackage.time_entries.hours"
        ));
        // Guard + traversal.
        assert!(has(
            "openproject:WorkPackage.compute_total_hours",
            "raises",
            "exc:ActiveRecord::RecordInvalid"
        ));
        assert!(has(
            "openproject:WorkPackage.compute_total_hours",
            "traverses_relation",
            "openproject:WorkPackage.time_entries"
        ));
    }

    #[test]
    fn namespace_is_openproject() {
        let triples = expand(&locked_work_package_graph());
        assert!(
            triples
                .iter()
                .all(|t| { t.s.starts_with("openproject:") || t.s.starts_with("exc:") })
        );
    }
}
