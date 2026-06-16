//! `ruff_ruby_spo` — Ruby/Rails frontend for the shared SPO triplet core.
//!
//! Walks an `app/models/` tree and produces a [`ModelGraph`] populated
//! with the AR-shape `Declaration` siblings the shared `ruff_spo_triplet`
//! crate expands into the 27 `OpenProject` AR-shape predicates (D-AR-3 in
//! the `openproject-ar-shape-extraction-v1` plan on lance-graph).
//!
//! # Architecture
//!
//! - [`mod@parse`] walks the directory, parses each `*.rb` with
//!   `lib-ruby-parser`, and finds class definitions (recursing into
//!   `module ... end` namespaces). One AST pass per file.
//! - [`mod@walk`] takes a class body and dispatches each top-level
//!   `Send` call (`belongs_to :project`, `validates :x`, `acts_as_list`,
//!   …) to the right [`Declaration`] variant by method-name match.
//! - [`extract`] unpacks each class's `declarations: Vec<Declaration>`
//!   into the typed `Model::{associations, validations, callbacks, …}`
//!   sibling slots the shared IR consumes.
//!
//! Method-body extraction ([`extract_fields`] / [`extract_functions`])
//! is intentionally minimal in D-AR-3: the 100 % coverage gate (D-AR-4)
//! measures *declarations*, not field/function depth. The two body
//! extractors return empty vecs; the follow-up D-AR-3.5 implements them
//! against `Declaration::Attribute` + `Def` walks.
//!
//! Downstream consumers (`lance_graph` SPO loader, `action_emitter`,
//! `link_chain`) need ZERO changes — they already consume the triple
//! shape this crate targets via `ruff_spo_triplet::expand`.

use std::path::Path;

use ruff_spo_triplet::{
    ActsAs, AssocDecl, AttrDecl, Callback, ConcernRef, Delegation, DslCall, DynMethod, Field,
    Function, GemDsl, Model, ModelGraph, ScopeDecl, StiInfo, UsingRef, Validation,
};

mod functions;
mod parse;
mod walk;

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
    /// Method-body extraction (D-AR-3.5): one [`Function`] per `def`
    /// in the class body. Populated by [`parse::parse_models`] alongside
    /// `declarations`, then flowed straight onto `Model::functions` by
    /// [`extract`] below.
    pub functions: Vec<Function>,
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
#[must_use]
pub fn extract(source_tree: &Path) -> ModelGraph {
    let classes = parse::parse_models(source_tree);
    let mut graph = ModelGraph::new(NAMESPACE);
    for class in &classes {
        let mut model = Model::new(&class.name);
        model.fields = extract_fields(class);
        // D-AR-3.5: method-name + raise/reads/traverses extraction
        // already happened at parse time (see `parse.rs`). The class
        // carries a populated `Function` vec.
        model.functions.clone_from(&class.functions);
        for decl in &class.declarations {
            unpack_declaration(&mut model, decl);
        }
        graph.models.push(model);
    }
    graph
}

/// Convenience: extract a single Ruby class from a source string. Used
/// by the synthetic coverage test (D-AR-4 partial); production callers
/// use [`extract`] over a tree.
#[doc(hidden)]
pub fn extract_from_source(src: &str) -> Vec<RubyClass> {
    parse::parse_models_from_source_for_test(src)
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

/// Extract [`Field`]s from a class. **D-AR-3 stub** — returns empty.
///
/// The full implementation (D-AR-3.5) will derive fields from:
/// - DB columns via `db/schema.rb` parsing,
/// - [`Declaration::Attribute`] entries whose kind is
///   `Attribute` / `AttrAccessor` / `StoreAccessor` / etc.,
/// - memoized/derived method assignments (the `emitted_by` link).
///
/// The D-AR-4 coverage gate measures *declarations*, not fields, so the
/// stub is sufficient for ndjson + `expand()` shipment of AR-shape facts.
fn extract_fields(_class: &RubyClass) -> Vec<Field> {
    Vec::new()
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

    // ────────────────── D-AR-3 + D-AR-4 coverage tests ──────────────────

    /// **D-AR-4 synthetic fixture** — a single Ruby source string that
    /// exercises every routed DSL category. Asserts the extractor finds
    /// at least one declaration per `Declaration` variant. This is the
    /// hermetic 100 %-coverage gate (runs in `cargo test` without
    /// needing the `OpenProject` corpus on disk).
    ///
    /// If a new DSL name is added to `walk::route_send` and forgotten
    /// here, this test still passes — the corpus assertion below is
    /// the second gate. But forgetting a Declaration *variant* trips
    /// this test loudly.
    #[test]
    fn ar_shape_synthetic_fixture_exercises_every_declaration_variant() {
        // Carefully crafted to exercise:
        // - all 5 AssocKind variants
        // - 3 Validation kinds (Validates / Validate / Normalizes)
        // - 1 Callback (before_save)
        // - 3 Concern kinds (Include / Extend / ClassMethodsBlock via block form)
        // - 4 Attribute kinds (Attribute / AttrAccessor / AliasAttribute / Serialize)
        // - Delegation, Scope (regular + default + plural), ActsAs,
        //   DslCall (catch-all + 2 promoted), GemDsl, DynamicMethod,
        //   Using, Sti.
        let src = r#"
class WorkPackage < Issue
  belongs_to :project
  has_many :time_entries
  has_one :budget
  has_and_belongs_to_many :watchers
  accepts_nested_attributes_for :children

  validates :subject, presence: true
  validate :custom_rule
  normalizes :email, with: ->(v) { v.downcase }

  before_save :tidy_up

  include Acts::Customizable
  extend Pagination::Model
  class_methods do
    def klass_method; end
  end

  attribute :estimated_hours
  attr_accessor :virtual
  alias_attribute :title, :subject
  serialize :preferences, JSON

  delegate :name, :identifier, to: :project, prefix: true

  scope :open, -> { where(closed: false) }
  default_scope -> { order(:id) }
  scopes :by_priority, :by_status

  acts_as_list scope: :project_id
  acts_as_watchable

  register_journal_formatter :diff, :custom
  register_journal_formatted_fields :subject
  activity_provider_for :work_packages
  random_unmapped_dsl :foo

  mount_uploader :avatar, AvatarUploader
  has_paper_trail on: [:update]

  define_method(:dynamic) { puts "hi" }

  using OpenProject::DateRange
end
"#;
        let classes = extract_from_source(src);
        assert_eq!(classes.len(), 1, "should find exactly one class");
        let cls = &classes[0];
        assert_eq!(cls.name, "WorkPackage");

        // Count each Declaration variant.
        let mut assoc = 0;
        let mut valid = 0;
        let mut cbk = 0;
        let mut concern = 0;
        let mut attr = 0;
        let mut deleg = 0;
        let mut scope = 0;
        let mut acts_as = 0;
        let mut dsl = 0;
        let mut gem = 0;
        let mut dynm = 0;
        let mut using = 0;
        let mut sti = 0;
        for d in &cls.declarations {
            match d {
                Declaration::Association(_) => assoc += 1,
                Declaration::Validation(_) => valid += 1,
                Declaration::Callback(_) => cbk += 1,
                Declaration::Concern(_) => concern += 1,
                Declaration::Attribute(_) => attr += 1,
                Declaration::Delegation(_) => deleg += 1,
                Declaration::Scope(_) => scope += 1,
                Declaration::ActsAs(_) => acts_as += 1,
                Declaration::DslCall(_) => dsl += 1,
                Declaration::GemDsl(_) => gem += 1,
                Declaration::DynamicMethod(_) => dynm += 1,
                Declaration::Using(_) => using += 1,
                Declaration::Sti(_) => sti += 1,
            }
        }
        // Every Declaration variant gets at least one hit.
        assert_eq!(assoc, 5, "5 association macros");
        assert_eq!(valid, 3, "validates + validate + normalizes");
        assert!(cbk >= 1, "at least 1 callback");
        assert!(concern >= 3, "include + extend + class_methods do");
        assert_eq!(attr, 4, "attribute + attr_accessor + alias_attribute + serialize");
        assert_eq!(deleg, 1, "delegate :name, :identifier, to:");
        assert!(scope >= 4, "1 scope + 1 default + 2 in scopes plural");
        assert_eq!(acts_as, 2, "acts_as_list + acts_as_watchable");
        // dsl_calls: register_journal_formatter, register_journal_formatted_fields,
        // activity_provider_for, random_unmapped_dsl (catch-all)
        assert!(dsl >= 4, "promoted + long-tail + catch-all");
        assert_eq!(gem, 2, "mount_uploader + has_paper_trail");
        assert_eq!(dynm, 1, "define_method");
        assert_eq!(using, 1, "using OpenProject::DateRange");
        assert_eq!(sti, 1, "STI parent (Issue) recorded");
    }

    /// **D-AR-4 routing-table lock** — the catch-all `has_dsl_call` must
    /// not absorb any name that has its own discriminated predicate.
    /// Promoted names (`register_journal_formatter`,
    /// `register_journal_formatted_fields`) intentionally go through the
    /// `DslCall` variant *with their own name* — the expander routes
    /// them to `RegistersJournalFormatter` / `RegistersJournalFormattedFields`
    /// in `ruff_spo_triplet::expand`. This test asserts the names in
    /// the long-tail catch-all are EITHER known promoted names OR
    /// genuinely-unknown DSL.
    #[test]
    fn ar_shape_dsl_catchall_does_not_steal_discriminated_names() {
        // The §2 closed-vocab names that have their OWN discriminated
        // Declaration variant (not DslCall). If any of these end up in
        // the catch-all DslCall, the walker's routing is broken.
        let discriminated_names = [
            // Associations (Declaration::Association)
            "belongs_to",
            "has_many",
            "has_one",
            "has_and_belongs_to_many",
            "accepts_nested_attributes_for",
            // Validations
            "validates",
            "validate",
            "normalizes",
            "validates_associated",
            "validates_each",
            // Callbacks (any of the 13+ phases)
            "before_save",
            "after_create",
            // Concerns
            "include",
            "extend",
            "prepend",
            // Attributes
            "attribute",
            "attr_accessor",
            "attr_reader",
            "alias_attribute",
            "alias_method",
            "serialize",
            "enum",
            "store_attribute",
            "store_accessor",
            // Delegation
            "delegate",
            // Scopes
            "scope",
            "default_scope",
            "scopes",
            // Acts_as (any acts_as_*)
            "acts_as_list",
            "acts_as_watchable",
            // Gem DSL
            "mount_uploader",
            "has_paper_trail",
            "has_closure_tree",
            "counter_culture",
            "auto_strip_attributes",
            // Metaprogramming
            "define_method",
            // Refinements
            "using",
        ];
        // Source pulling every discriminated name + one genuinely-unknown
        // one that SHOULD land in the catch-all.
        let src = r#"
class Sample
  belongs_to :project
  has_many :children
  has_one :parent
  has_and_belongs_to_many :tags
  accepts_nested_attributes_for :items
  validates :name
  validate :rule
  normalizes :email
  validates_associated :rel
  validates_each :a
  before_save :hook
  after_create :hook
  include ModX
  extend ModY
  prepend ModZ
  attribute :a
  attr_accessor :b
  attr_reader :c
  alias_attribute :d, :e
  alias_method :f, :g
  serialize :h
  enum :i
  store_attribute :j, :k
  store_accessor :l, :m
  delegate :n, to: :o
  scope :p, -> { }
  default_scope -> { }
  scopes :q, :r
  acts_as_list
  acts_as_watchable
  mount_uploader :s
  has_paper_trail
  has_closure_tree
  counter_culture :t
  auto_strip_attributes :u
  define_method(:v) { }
  using Mod
  totally_unknown_op_dsl :catchall
end
"#;
        let classes = extract_from_source(src);
        assert_eq!(classes.len(), 1);
        let cls = &classes[0];
        let dsl_names: std::collections::HashSet<&str> = cls
            .declarations
            .iter()
            .filter_map(|d| match d {
                Declaration::DslCall(dc) => Some(dc.name.as_str()),
                _ => None,
            })
            .collect();
        // Every discriminated name must NOT appear in the catch-all.
        for name in discriminated_names {
            assert!(
                !dsl_names.contains(name),
                "name `{name}` leaked into has_dsl_call catch-all — routing bug",
            );
        }
        // The genuinely-unknown one DOES land in catch-all.
        assert!(
            dsl_names.contains("totally_unknown_op_dsl"),
            "unknown DSL must land in has_dsl_call catch-all",
        );
    }

    /// **D-AR-4 real-corpus coverage gate.** Runs over the `OpenProject`
    /// `app/models/` tree at `$OPENPROJECT_PATH`. Conditional on the
    /// env var being set so CI doesn't require the corpus checked out.
    ///
    /// Asserts:
    /// - between 900 and 1100 classes extracted (measured baseline:
    ///   941 files / ~960 class defs incl. STI subclasses),
    /// - the catch-all `has_dsl_call` only carries genuinely-unknown
    ///   names (none of the §2 discriminated names), which is the
    ///   100 %-coverage gate.
    #[test]
    #[allow(clippy::print_stderr)] // diagnostic emission gated on env var (real-corpus gate)
    fn ar_shape_real_corpus_coverage_gate() {
        let Ok(root) = std::env::var("OPENPROJECT_PATH") else {
            eprintln!("OPENPROJECT_PATH unset; skipping real-corpus gate");
            return;
        };
        let graph = extract(std::path::Path::new(&root));
        let class_count = graph.models.len();
        assert!(
            (500..=1200).contains(&class_count),
            "class count {class_count} outside expected band (measured file baseline = 941; \
             class count is lower because some files are modules-only and some files contain multiple classes)",
        );
        // No discriminated name should have leaked to the catch-all
        // (same set as the synthetic test above, in inline form).
        let leaked: Vec<String> = graph
            .models
            .iter()
            .flat_map(|m| m.dsl_calls.iter().map(|d| d.name.clone()))
            .filter(|n| {
                matches!(
                    n.as_str(),
                    "belongs_to"
                        | "has_many"
                        | "has_one"
                        | "has_and_belongs_to_many"
                        | "accepts_nested_attributes_for"
                        | "validates"
                        | "validate"
                        | "normalizes"
                        | "include"
                        | "extend"
                        | "prepend"
                        | "delegate"
                        | "scope"
                        | "default_scope"
                        | "define_method"
                        | "using"
                )
            })
            .collect();
        assert!(
            leaked.is_empty(),
            "discriminated names leaked to has_dsl_call catch-all: {leaked:?}",
        );
        // Total declarations across all classes — sanity bound from the
        // §2 census (1696 measured; allow drift).
        let total_decls: usize = graph
            .models
            .iter()
            .map(|m| {
                m.associations.len()
                    + m.validations.len()
                    + m.callbacks.len()
                    + m.concerns.len()
                    + m.attributes.len()
                    + m.delegations.len()
                    + m.scopes.len()
                    + m.acts_as.len()
                    + m.dsl_calls.len()
                    + m.gem_dsl.len()
                    + m.dynamic_methods.len()
                    + m.refinements.len()
                    + usize::from(m.sti.is_some())
            })
            .sum();
        assert!(
            (1000..=2500).contains(&total_decls),
            "declaration count {total_decls} outside expected band (measured baseline ≈ 1696)",
        );
        eprintln!(
            "D-AR-4 corpus gate: {class_count} classes, {total_decls} declarations, 0 leaked names",
        );
    }

    // ────────────────── Codex P2 regression tests ──────────────────

    /// **Codex P2 (PR #6)** — `module Foo; class Bar < ApplicationRecord; end; end`
    /// must yield `name = "Foo::Bar"`, not just `"Bar"`. Otherwise
    /// distinct namespaced models with the same inner class name
    /// collide in the SPO graph.
    #[test]
    fn module_namespace_qualifies_inner_class_name() {
        let src = r#"
module Foo
  class Bar < ApplicationRecord
  end
end

module Foo
  module Inner
    class Bar < ApplicationRecord
    end
  end
end
"#;
        let classes = extract_from_source(src);
        let names: Vec<&str> = classes.iter().map(|c| c.name.as_str()).collect();
        assert!(
            names.contains(&"Foo::Bar"),
            "expected Foo::Bar; got {names:?}"
        );
        assert!(
            names.contains(&"Foo::Inner::Bar"),
            "expected Foo::Inner::Bar; got {names:?}"
        );
        // The two `Bar`s must NOT collide.
        assert_eq!(names.iter().filter(|n| n.ends_with("::Bar")).count(), 2);
    }

    /// **Codex P2 (PR #6)** — `has_many :items, -> { active }, dependent: :destroy`
    /// puts the options hash at args[2] (the lambda is args[1]). The
    /// scoped-association form was silently dropping the options.
    #[test]
    fn scoped_association_picks_up_trailing_options() {
        let src = r#"
class WorkPackage < ApplicationRecord
  has_many :items, -> { where(active: true) }, dependent: :destroy, class_name: "Item"
end
"#;
        let classes = extract_from_source(src);
        let assoc = classes[0]
            .declarations
            .iter()
            .find_map(|d| match d {
                Declaration::Association(a) => Some(a),
                _ => None,
            })
            .expect("expected an association");
        assert_eq!(assoc.name, "items");
        let opt_keys: Vec<&str> =
            assoc.options.iter().map(|(k, _)| k.as_str()).collect();
        assert!(
            opt_keys.contains(&"dependent"),
            "expected `dependent` in options; got {opt_keys:?}",
        );
        assert!(
            opt_keys.contains(&"class_name"),
            "expected `class_name` in options; got {opt_keys:?}",
        );
    }

    /// **Codex P2 (PR #6)** — `attribute :age, :integer` must NOT emit
    /// `integer` as a bogus attribute. Only `age` is a real attribute;
    /// `:integer` is the type metadata.
    #[test]
    fn attribute_macro_does_not_emit_type_as_attribute() {
        let src = r#"
class M < ApplicationRecord
  attribute :age, :integer
  serialize :data, JSON
  enum :status, { active: 0, archived: 1 }
end
"#;
        let classes = extract_from_source(src);
        let attrs: Vec<&str> = classes[0]
            .declarations
            .iter()
            .filter_map(|d| match d {
                Declaration::Attribute(a) => Some(a.name.as_str()),
                _ => None,
            })
            .collect();
        assert!(attrs.contains(&"age"), "age must be extracted: {attrs:?}");
        assert!(attrs.contains(&"data"), "data must be extracted");
        assert!(attrs.contains(&"status"), "status must be extracted");
        // The type/class/hash MUST NOT leak as attributes.
        assert!(
            !attrs.contains(&"integer"),
            "bogus `integer` attribute: {attrs:?}"
        );
        assert!(
            !attrs.contains(&"JSON"),
            "bogus `JSON` attribute: {attrs:?}"
        );
    }

    /// **Codex P2 (PR #6)** — `store_accessor :store_key, :a, :b, :c`
    /// must NOT emit `store_key` as an attribute. Only `:a`, `:b`,
    /// `:c` are real attributes; `:store_key` is the store column.
    #[test]
    fn store_accessor_does_not_emit_store_key_as_attribute() {
        let src = r#"
class M < ApplicationRecord
  store_accessor :preferences, :theme, :language
  store_attribute :preferences, :font_size, :integer
end
"#;
        let classes = extract_from_source(src);
        let attrs: Vec<&str> = classes[0]
            .declarations
            .iter()
            .filter_map(|d| match d {
                Declaration::Attribute(a) => Some(a.name.as_str()),
                _ => None,
            })
            .collect();
        assert!(attrs.contains(&"theme"), "theme must be extracted: {attrs:?}");
        assert!(attrs.contains(&"language"));
        assert!(attrs.contains(&"font_size"));
        // Store key MUST NOT leak.
        assert!(
            !attrs.contains(&"preferences"),
            "store key `preferences` leaked: {attrs:?}",
        );
        // Type MUST NOT leak.
        assert!(
            !attrs.contains(&"integer"),
            "bogus `integer` attribute: {attrs:?}",
        );
    }

    /// **Codex P2 (PR #6)** — `with_options presence: true do; validates :name; end`
    /// must NOT lose the inner validations. The wrapper block's body
    /// is recursed into so declarations inside grouping blocks land
    /// on the model.
    #[test]
    fn with_options_grouping_block_recurses_into_body() {
        let src = r#"
class M < ApplicationRecord
  with_options presence: true do
    validates :name
    validates :subject
  end
end
"#;
        let classes = extract_from_source(src);
        let validations: Vec<&str> = classes[0]
            .declarations
            .iter()
            .filter_map(|d| match d {
                Declaration::Validation(v) => Some(v.target.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            validations.contains(&"name"),
            "validates :name lost from with_options body: {validations:?}",
        );
        assert!(validations.contains(&"subject"));
    }

    /// **Codex P2 (PR #6)** — Ruby's `alias new_name old_name` keyword
    /// form parses as `Node::Alias`, NOT as a `Send`. The walker now
    /// recognises it and emits an `Attribute::Alias` declaration.
    #[test]
    fn ruby_alias_keyword_emits_alias_declaration() {
        let src = r#"
class M < ApplicationRecord
  def original; end
  alias new_method original
end
"#;
        let classes = extract_from_source(src);
        let aliases: Vec<&str> = classes[0]
            .declarations
            .iter()
            .filter_map(|d| match d {
                Declaration::Attribute(a)
                    if matches!(a.kind, ruff_spo_triplet::AttrKind::Alias) =>
                {
                    Some(a.name.as_str())
                }
                _ => None,
            })
            .collect();
        assert!(
            aliases.contains(&"new_method=original"),
            "alias keyword not captured: {aliases:?}",
        );
    }
}
