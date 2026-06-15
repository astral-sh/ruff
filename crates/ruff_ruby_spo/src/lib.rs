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
//! 2. Replace the `todo!()` in [`parse_models`] to produce a `Vec<RubyClass>`
//!    populated with structured [`Declaration`] records — one per
//!    class-body DSL call. (The 67-emit-category disambiguation happens
//!    at parse time, not by re-scanning a `body_source` blob.)
//! 3. Replace the `todo!()`s in [`extract_fields`] / [`extract_functions`]
//!    to read the Rails constructs documented on each.
//! 4. Run the locked-shape test after each step — it asserts the
//!    `expand()` output for a hand-built `ModelGraph`, so it tells you when
//!    your extraction produces the right shape.
//! 5. Point [`extract`] at an `OpenProject` `app/models/` tree.
//!
//! The downstream consumers (`lance_graph` SPO loader, `action_emitter`,
//! `link_chain`) need ZERO changes — they already consume the triple shape
//! this crate targets.

use std::path::Path;

use ruff_spo_triplet::{
    ActsAs, AssocDecl, AttrDecl, Callback, ConcernRef, Delegation, DslCall, DynMethod, Field,
    Function, GemDsl, Model, ModelGraph, ScopeDecl, StiInfo, UsingRef, Validation,
};

/// The namespace prefix for `OpenProject` subjects/objects.
pub const NAMESPACE: &str = "openproject";

/// A minimally-parsed Ruby class — what a parser frontend should produce
/// before the IR mapping.
///
/// **Round-1 council ACK (prior-art-savant + dto-soa-savant):** the
/// `Vec<Declaration>` shape replaces the pre-existing
/// `associations: Vec<String>` field. The 67-emit-category disambiguation
/// is done at parse time (one [`Declaration`] variant per category), not
/// by carrying a `body_source` blob and re-scanning it. This keeps the
/// frontend → IR projection a pure unpacking (`extract` below) and the
/// downstream `expand()` chain a pure projection.
#[derive(Debug, Clone, Default)]
pub struct RubyClass {
    /// Class name as written (`WorkPackage`). No dots in Ruby class names,
    /// so no normalisation needed (unlike Odoo's `account.move`).
    pub name: String,
    /// Every class-body DSL call, captured in source order. The
    /// frontend-local discriminated union; the [`extract`] fn unpacks
    /// this into the typed `Model::{associations, validations, …}`
    /// sibling fields the shared IR consumes.
    pub declarations: Vec<Declaration>,
}

/// One class-body DSL call, discriminated by category.
///
/// **Frontend-local IR** (per Round-1 prior-art-savant verdict): the
/// shared `ruff_spo_triplet::Model` already carries 12 sibling-shape
/// `Vec<…>` fields + 1 `Option<StiInfo>` per category. This enum is just
/// the in-source-order shape the parser emits *before* the [`extract`]
/// fn unpacks them. It is NOT exposed in any triple — it disappears at
/// the IR boundary.
#[derive(Debug, Clone)]
pub enum Declaration {
    /// `belongs_to` / `has_many` / `has_one` / `has_and_belongs_to_many` /
    /// `accepts_nested_attributes_for`. Distinct from body-walk
    /// `traverses_relation` (which stays on `Function::traverses`).
    Association(AssocDecl),
    /// `validates` / `validate` / `normalizes` / `validates_associated` /
    /// `validates_each`.
    Validation(Validation),
    /// `before_*` / `after_*` / `around_*` callback macros.
    Callback(Callback),
    /// `include` / `extend` / `prepend` / `class_methods do` / `included do`.
    Concern(ConcernRef),
    /// `attribute` / `attr_*` / `alias_*` / `serialize` / `enum` /
    /// `store_attribute` / `store_accessor` / `define_attribute_method` /
    /// `undef_method`.
    Attribute(AttrDecl),
    /// `delegate :foo, :bar, to: :baz`.
    Delegation(Delegation),
    /// `scope` / `default_scope` / `scopes` (OP plural).
    Scope(ScopeDecl),
    /// `acts_as_*` family.
    ActsAs(ActsAs),
    /// `OpenProject` custom DSL calls (`register_journal_formatter`,
    /// `register_journal_formatted_fields`, plus the long-tail singletons).
    DslCall(DslCall),
    /// Third-party gem DSL (`mount_uploader`, `has_paper_trail`, …).
    GemDsl(GemDsl),
    /// `define_method` dynamic-method site.
    DynamicMethod(DynMethod),
    /// `using SomeRefinement`.
    Using(UsingRef),
    /// `class X < Parent` STI parent (also `abstract_class` /
    /// `inheritance_column` metadata).
    Sti(StiInfo),
}

/// Top-level entry: walk a Rails `app/models/` tree and produce the IR.
///
/// The unpacking is mechanical: each [`Declaration`] variant lands in
/// its corresponding `Model::*` Vec field. STI entries replace
/// `Model::sti` (`None` if absent; last-wins if a class has multiple,
/// which it shouldn't).
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
        for decl in &class.declarations {
            unpack_declaration(&mut model, decl);
        }
        graph.models.push(model);
    }
    graph
}

/// The pure unpacking: route each [`Declaration`] into its typed
/// `Model::*` Vec / Option slot. No semantic transform here — this is
/// the seam between source-order parsing and category-grouped IR.
fn unpack_declaration(model: &mut Model, decl: &Declaration) {
    match decl {
        Declaration::Association(a) => model.associations.push(a.clone()),
        Declaration::Validation(v) => model.validations.push(v.clone()),
        Declaration::Callback(cb) => model.callbacks.push(cb.clone()),
        Declaration::Concern(cr) => model.concerns.push(cr.clone()),
        Declaration::Attribute(a) => model.attributes.push(a.clone()),
        Declaration::Delegation(d) => model.delegations.push(d.clone()),
        Declaration::Scope(s) => model.scopes.push(s.clone()),
        Declaration::ActsAs(aa) => model.acts_as.push(aa.clone()),
        Declaration::DslCall(dc) => model.dsl_calls.push(dc.clone()),
        Declaration::GemDsl(g) => model.gem_dsl.push(g.clone()),
        Declaration::DynamicMethod(dm) => model.dynamic_methods.push(dm.clone()),
        Declaration::Using(u) => model.refinements.push(u.clone()),
        Declaration::Sti(sti) => model.sti = Some(sti.clone()),
    }
}

/// Parse every `*.rb` under `app/models/` into [`RubyClass`] records.
///
/// # What to wire
///
/// Use `lib-ruby-parser` (or tree-sitter-ruby) to:
/// - find each `class X < ApplicationRecord` (and STI subclasses),
/// - walk the class body and emit one [`Declaration`] per DSL call
///   recognised (see the §2 closed-vocab table in
///   `.claude/plans/openproject-ar-shape-extraction-v1.md` for the full
///   67-category routing).
///
/// Also parse `db/schema.rb` once to map each table → its columns; those
/// columns are the baseline [`Field`]s (the `attribute`/`attr_accessor`
/// declarations in the class body are additive, captured as
/// [`Declaration::Attribute`]).
fn parse_models(_source_tree: &Path) -> Vec<RubyClass> {
    todo!(
        "wire a Ruby parser (lib-ruby-parser): collect class defs + \
         structured Declaration list per class body + seed columns from \
         db/schema.rb"
    )
}

/// Extract [`Field`]s from a class.
///
/// # What to wire (Rails → IR)
///
/// - **`Field::name`**: DB columns (from `schema.rb`), plus
///   [`Declaration::Attribute`] entries whose kind is
///   `Attribute` / `AttrAccessor` / `StoreAccessor` / etc.
/// - **`Field::emitted_by`**: a memoized/derived method that assigns the
///   attribute — `def total_hours; @total_hours ||= compute…; end`. The
///   method name is the `emitted_by` target.
/// - **`Field::depends_on`**: the association/attribute chains that derived
///   attribute's method reads (`time_entries.hours`). Emit dotted paths
///   verbatim — the downstream `link_chain` splitter resolves the hops.
fn extract_fields(_class: &RubyClass) -> Vec<Field> {
    todo!(
        "read schema.rb columns + the Declaration::Attribute entries; \
         link derived attrs to their computing method (emitted_by) and \
         that method's read chains (depends_on)"
    )
}

/// Extract [`Function`]s from a class.
///
/// # What to wire (Rails → IR)
///
/// - **`Function::name`**: each `def method_name` in the class body
///   (instance methods; include callback targets referenced by the
///   `Declaration::Callback` entries).
/// - **`Function::reads`**: `self.x` reads and bare attribute reads in the
///   method body (Inferred tier).
/// - **`Function::raises`**: `raise X`, `errors.add(...)`, and declarative
///   `validates`/`validate` (Authoritative). Map declarative validations
///   to `raises ActiveRecord::RecordInvalid` — see the guide §5 step 2.
/// - **`Function::traverses`**: calls in the body whose receiver/name is
///   one of the relation symbols carried by `Declaration::Association`
///   entries (`time_entries.each`, `project.members`). The association
///   name is the relation (Inferred).
fn extract_functions(_class: &RubyClass) -> Vec<Function> {
    todo!(
        "read def bodies: attribute reads (reads), raise/errors.add/validates \
         (raises exc:…), and association walks restricted to the relation \
         names from Declaration::Association (traverses_relation)"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_spo_triplet::{
        AssocDecl, AssocKind, AttrDecl, AttrKind, Callback, ConcernKind, ConcernRef, expand,
    };

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
            ..Default::default()
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

    /// Unpacking lock: a fully-populated `RubyClass.declarations` list
    /// must end up in the corresponding `Model::*` Vec slots after
    /// `unpack_declaration` runs across every variant. This guards the
    /// frontend→IR seam against drift if a new `Declaration` variant is
    /// added without a routing arm.
    #[test]
    fn declarations_unpack_into_typed_model_slots() {
        let mut model = Model::new("Sample");
        let decls = vec![
            Declaration::Association(AssocDecl {
                kind: AssocKind::BelongsTo,
                name: "project".to_string(),
                options: vec![],
            }),
            Declaration::Callback(Callback {
                phase: "before_save".to_string(),
                target: "tidy_up".to_string(),
                options: vec![],
            }),
            Declaration::Concern(ConcernRef {
                kind: ConcernKind::Include,
                module: "Acts::Customizable".to_string(),
                body_ref: None,
            }),
            Declaration::Attribute(AttrDecl {
                kind: AttrKind::AttrAccessor,
                name: "virtual_flag".to_string(),
                options: vec![],
            }),
        ];
        for d in &decls {
            unpack_declaration(&mut model, d);
        }
        assert_eq!(model.associations.len(), 1);
        assert_eq!(model.callbacks.len(), 1);
        assert_eq!(model.concerns.len(), 1);
        assert_eq!(model.attributes.len(), 1);
        // The unrouted slots stay empty.
        assert!(model.validations.is_empty());
        assert!(model.acts_as.is_empty());
        assert!(model.dsl_calls.is_empty());
        assert!(model.sti.is_none());
    }
}
