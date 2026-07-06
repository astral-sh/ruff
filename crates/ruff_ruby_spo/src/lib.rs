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
mod schema;
mod views;
mod walk;

pub use schema::{SchemaReport, extract_app_with_schema};
pub use views::{
    ViewFieldSet, ViewScanReport, ViewTarget, extract_view_field_sets,
    extract_view_field_sets_with_report,
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
    /// Method-body extraction (D-AR-3.5): one [`Function`] per `def`
    /// in the class body. Populated by [`parse::parse_models`] alongside
    /// `declarations`, then flowed straight onto `Model::functions` by
    /// [`extract`] below.
    pub functions: Vec<Function>,
    /// Non-public (`private`/`protected`) defs, walked with the same body
    /// pass as `functions` but kept OUT of the routable-action surface.
    /// Rails lifecycle callbacks conventionally target these; carried so
    /// body-fact analysis (OGAR F17 body triage) can resolve hook targets.
    pub helpers: Vec<Function>,
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

/// Top-level entry: walk a Rails `app/models/` tree and produce the IR
/// tagged with the default [`NAMESPACE`] (`"openproject"`). Thin wrapper
/// over [`extract_with`] for backward compatibility — every existing
/// caller (`ogar-from-rails::extract`) keeps working unchanged.
///
/// New callers harvesting a non-OpenProject Rails curator (Redmine, Spree,
/// Open-Source-Billing, …) should use [`extract_with`] to tag the produced
/// `ModelGraph.namespace` correctly — otherwise downstream consumers see
/// every harvest prefixed `"openproject:"` regardless of source.
#[must_use]
pub fn extract(source_tree: &Path) -> ModelGraph {
    extract_with(source_tree, NAMESPACE)
}

/// Walk a Rails `app/models/` tree and produce the IR, tagging the
/// resulting [`ModelGraph`] with the caller-supplied `namespace`. The
/// namespace becomes the IRI prefix on every produced triple's subject /
/// object, so the same parser handles any Rails curator (`OpenProject`,
/// Redmine, Spree, Solidus, Open-Source-Billing, …) by simply passing the
/// curator's namespace string.
///
/// The unpacking is mechanical: each [`Declaration`] variant lands in
/// its corresponding `Model::*` Vec field. Same-named [`RubyClass`]
/// entries (Ruby's `class X` reopen idiom across multiple files —
/// e.g. `OpenProject`'s `WorkPackage` reopened from
/// `app/models/work_package/inexistent_work_package.rb` etc.) are
/// merged into a single [`Model`] in source-file order: Vec fields
/// concatenate; `sti` is first-non-`None`-wins.
#[must_use]
pub fn extract_with(source_tree: &Path, namespace: &str) -> ModelGraph {
    let classes = parse::parse_models(source_tree);
    let mut graph = ModelGraph::new(namespace);
    graph.models = build_models(&classes);
    graph
}

/// Walk an **arbitrary Rails app subtree** directly — `app/controllers`,
/// `app/models`, `app/jobs`, … — with no `app/models` suffix assumption, and
/// produce the IR. This is the **DO-arm / controller-harvest entry**: point it
/// at `app/controllers` and each controller's public actions land in
/// `Model::functions`, which `ogar_from_ruff::lift_actions` lifts to a
/// standalone `Vec<ActionDef>` (the DO arm). `extract`/`extract_with` remain
/// the `app/models` (THINK-arm) specialisations.
#[must_use]
pub fn extract_tree_with(dir: &Path, namespace: &str) -> ModelGraph {
    let classes = parse::parse_tree(dir);
    let mut graph = ModelGraph::new(namespace);
    graph.models = build_models(&classes);
    graph
}

/// Like [`extract_with`], but walks the **whole Rails application** — the
/// core `app/models` **plus every mounted engine's `app/models`**
/// (`modules/*/app/models`, `engines/*/app/models`).
///
/// `OpenProject` keeps a large share of its domain in `modules/*` engines
/// (e.g. `TimeEntry` lives in `modules/costs/app/models`), invisible to
/// the core-only [`extract`]. All roots feed one `build_models` pass, so a
/// class reopened across engines still merges into a single [`Model`].
///
/// Plain [`extract`] / [`extract_with`] stay core-only for backward
/// compatibility; opt into engines explicitly with this entry point.
#[must_use]
pub fn extract_app_with(source_tree: &Path, namespace: &str) -> ModelGraph {
    let classes = parse::parse_models_with_engines(source_tree);
    let mut graph = ModelGraph::new(namespace);
    graph.models = build_models(&classes);
    graph
}

/// Whole-application extraction (core + engines) tagged with the default
/// [`NAMESPACE`]. Thin wrapper over [`extract_app_with`]; see [`extract`]
/// for the namespace caveat.
#[must_use]
pub fn extract_app(source_tree: &Path) -> ModelGraph {
    extract_app_with(source_tree, NAMESPACE)
}

/// Build the deduplicated `Vec<Model>` from parsed [`RubyClass`]es —
/// merging same-named reopens into a single Model.
///
/// Order: first occurrence of each name keeps its slot; later
/// occurrences merge in-place (Vec fields concatenate in encounter
/// order; `sti` is first-non-`None`-wins so empty reopens cannot
/// overwrite a previously-set inheritance fact).
fn build_models(classes: &[RubyClass]) -> Vec<Model> {
    use std::collections::HashMap;
    let mut models: Vec<Model> = Vec::with_capacity(classes.len());
    let mut index: HashMap<String, usize> = HashMap::with_capacity(classes.len());
    for class in classes {
        let mut next = Model::new(&class.name);
        next.fields = extract_fields(class);
        // D-AR-3.5: method-name + raise/reads/traverses extraction
        // already happened at parse time (see `parse.rs`). The class
        // carries a populated `Function` vec.
        next.functions.clone_from(&class.functions);
        next.helpers.clone_from(&class.helpers);
        for decl in &class.declarations {
            unpack_declaration(&mut next, decl);
        }
        if let Some(&slot) = index.get(&class.name) {
            merge_model(&mut models[slot], next);
        } else {
            index.insert(class.name.clone(), models.len());
            models.push(next);
        }
    }
    models
}

/// Merge `src` into `dst` in-place. Used by [`build_models`] when a
/// later `class X` reopen extends an earlier definition: Vec fields
/// concatenate (preserving source-file order); `sti` is
/// first-non-`None`-wins (empty reopens cannot drop inheritance).
fn merge_model(dst: &mut Model, src: Model) {
    dst.fields.extend(src.fields);
    dst.functions.extend(src.functions);
    dst.helpers.extend(src.helpers);
    dst.associations.extend(src.associations);
    dst.validations.extend(src.validations);
    dst.callbacks.extend(src.callbacks);
    dst.concerns.extend(src.concerns);
    dst.attributes.extend(src.attributes);
    dst.delegations.extend(src.delegations);
    dst.scopes.extend(src.scopes);
    dst.acts_as.extend(src.acts_as);
    dst.dsl_calls.extend(src.dsl_calls);
    dst.gem_dsl.extend(src.gem_dsl);
    dst.dynamic_methods.extend(src.dynamic_methods);
    dst.refinements.extend(src.refinements);
    if dst.sti.is_none() {
        dst.sti = src.sti;
    }
    // C++ sibling Vecs are populated only by `ruff_cpp_spo`; Ruby
    // extraction always leaves them empty, so extending is a no-op in
    // practice but keeps the merge total over the `Model` shape.
    dst.bases.extend(src.bases);
    dst.member_fields.extend(src.member_fields);
    dst.methods.extend(src.methods);
    dst.templates.extend(src.templates);
    dst.friends.extend(src.friends);
    dst.macro_uses.extend(src.macro_uses);
    dst.static_asserts.extend(src.static_asserts);
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
                ..Default::default()
            }],
            functions: vec![Function {
                name: "compute_total_hours".to_string(),
                reads: vec!["status".to_string()],
                raises: vec!["ActiveRecord::RecordInvalid".to_string()],
                traverses: vec!["time_entries".to_string()],
                ..Default::default()
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

    /// `extract` (no-arg-namespace form) tags the graph with the default
    /// [`NAMESPACE`] for back-compat — existing callers keep working.
    #[test]
    fn extract_defaults_to_openproject_namespace() {
        // A non-existent path returns an empty `ModelGraph` (no panic);
        // the namespace tag is still set correctly.
        let g = extract(std::path::Path::new("/__nonexistent_for_ruff_test__"));
        assert_eq!(g.namespace, NAMESPACE);
        assert_eq!(g.namespace, "openproject");
    }

    /// `extract_with` tags the graph with the caller-supplied namespace,
    /// so a Redmine / Spree / OSB harvest is not silently prefixed
    /// `openproject:`. Pins the new API for the curator-namespace use case.
    #[test]
    fn extract_with_tags_caller_supplied_namespace() {
        let g = extract_with(
            std::path::Path::new("/__nonexistent_for_ruff_test__"),
            "redmine",
        );
        assert_eq!(g.namespace, "redmine");
        let g = extract_with(
            std::path::Path::new("/__nonexistent_for_ruff_test__"),
            "osb",
        );
        assert_eq!(g.namespace, "osb");
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
        assert_eq!(
            attr, 4,
            "attribute + attr_accessor + alias_attribute + serialize"
        );
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

    /// Engine-walking: [`extract_app`] harvests core `app/models` PLUS every
    /// mounted engine's `app/models`, and a class reopened across roots
    /// merges into one `Model` (cross-root reopen-merge). Plain [`extract`]
    /// stays core-only.
    #[test]
    fn extract_app_walks_engines_and_merges_across_roots() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("ruff_engines_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let core = base.join("app/models");
        let engine = base.join("modules/costs/app/models");
        fs::create_dir_all(&core).unwrap();
        fs::create_dir_all(&engine).unwrap();
        fs::write(
            core.join("project.rb"),
            "class Project < ApplicationRecord\n  has_many :issues\nend\n",
        )
        .unwrap();
        // Engine-only model — invisible to core-only `extract`.
        fs::write(
            engine.join("time_entry.rb"),
            "class TimeEntry < ApplicationRecord\n  belongs_to :project\nend\n",
        )
        .unwrap();
        // Same class reopened in BOTH roots — must merge into one Model.
        fs::write(
            core.join("user.rb"),
            "class User < ApplicationRecord\n  has_many :members\nend\n",
        )
        .unwrap();
        fs::write(
            engine.join("user_costs.rb"),
            "class User < ApplicationRecord\n  has_many :time_entries\nend\n",
        )
        .unwrap();

        // core-only extract misses the engine model.
        let core_only = extract(&base);
        assert!(core_only.models.iter().any(|m| m.name == "Project"));
        assert!(
            !core_only.models.iter().any(|m| m.name == "TimeEntry"),
            "core-only extract must NOT see the engine model",
        );

        // extract_app sees both, namespace-tagged, and merges the reopen.
        let app = extract_app_with(&base, "openproject");
        assert_eq!(app.namespace, "openproject");
        assert!(app.models.iter().any(|m| m.name == "Project"));
        assert!(
            app.models.iter().any(|m| m.name == "TimeEntry"),
            "extract_app must harvest modules/*/app/models",
        );
        let users: Vec<&Model> = app.models.iter().filter(|m| m.name == "User").collect();
        assert_eq!(
            users.len(),
            1,
            "cross-root reopen must merge into ONE Model"
        );
        assert_eq!(
            users[0].associations.len(),
            2,
            "both roots' associations must be merged",
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// Real-corpus engine gate: on `OpenProject`, [`extract_app`] (core +
    /// engines) harvests strictly more than core-only [`extract`], and
    /// surfaces an engine-only model (`TimeEntry` lives in
    /// `modules/costs/app/models`). Env-gated like the coverage gate.
    #[test]
    #[allow(clippy::print_stderr)]
    fn extract_app_harvests_openproject_engines() {
        let Ok(root) = std::env::var("OPENPROJECT_PATH") else {
            eprintln!("OPENPROJECT_PATH unset; skipping engine real-corpus gate");
            return;
        };
        let path = std::path::Path::new(&root);
        let core = extract(path).models.len();
        let app = extract_app(path);
        let app_count = app.models.len();
        assert!(
            app_count > core,
            "extract_app ({app_count}) must exceed core-only ({core})",
        );
        assert!(
            app.models.iter().any(|m| m.name == "TimeEntry"),
            "TimeEntry (modules/costs/app/models) must be harvested by extract_app",
        );
        eprintln!("engine gate: core={core}, core+engines={app_count}");
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
        let opt_keys: Vec<&str> = assoc.options.iter().map(|(k, _)| k.as_str()).collect();
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
        assert!(
            attrs.contains(&"theme"),
            "theme must be extracted: {attrs:?}"
        );
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

    /// **D-AR-5.2** — `attribute :age, :integer` puts `:integer` as a
    /// Sym at args[1]. The walker now lifts it into the `AttrDecl`
    /// `options` as `("type", "integer")` so the expander can emit a
    /// `field_type` triple.
    #[test]
    fn attribute_macro_extracts_positional_type_into_options() {
        let src = r#"
class M < ApplicationRecord
  attribute :age, :integer
  attribute :name, :string, default: ""
  store_attribute :prefs, :font_size, :integer
  attribute :no_type
end
"#;
        let classes = extract_from_source(src);
        let attrs: Vec<(&str, Option<&str>)> = classes[0]
            .declarations
            .iter()
            .filter_map(|d| match d {
                Declaration::Attribute(a) => Some((
                    a.name.as_str(),
                    a.options
                        .iter()
                        .find(|(k, _)| k == "type")
                        .map(|(_, v)| v.as_str()),
                )),
                _ => None,
            })
            .collect();
        assert!(
            attrs.contains(&("age", Some("integer"))),
            "age must carry `integer` type; got {attrs:?}",
        );
        assert!(
            attrs.contains(&("name", Some("string"))),
            "name must carry `string` type",
        );
        assert!(
            attrs.contains(&("font_size", Some("integer"))),
            "store_attribute's attr must carry `integer` type",
        );
        assert!(
            attrs.contains(&("no_type", None)),
            "untyped attribute must carry no type option",
        );
    }

    // ───── reopen-merge tests (the OP `WorkPackage` regression) ─────

    /// Two `class WorkPackage` declarations across files must produce ONE
    /// `Model` whose Vec fields concatenate, not two duplicate entries.
    /// Repro for the `OpenProject` case: an empty reopener (from
    /// `app/models/work_package/inexistent_work_package.rb` etc.) was
    /// being emitted as a separate `Model { name: "WorkPackage", … }`
    /// alongside the rich one — and a naïve `.find(|c| c.name == "WorkPackage")`
    /// would land on the empty side.
    #[test]
    fn build_models_merges_same_named_reopens_into_one() {
        let empty_reopener = RubyClass {
            name: "WorkPackage".to_string(),
            declarations: Vec::new(),
            functions: Vec::new(),
            helpers: Vec::new(),
        };
        let rich = RubyClass {
            name: "WorkPackage".to_string(),
            declarations: vec![
                Declaration::Association(AssocDecl {
                    kind: AssocKind::BelongsTo,
                    name: "project".to_string(),
                    options: Vec::new(),
                }),
                Declaration::Concern(ConcernRef {
                    kind: ConcernKind::Include,
                    module: "WorkPackage::Validations".to_string(),
                    body_ref: None,
                }),
                Declaration::Attribute(AttrDecl {
                    kind: AttrKind::Attribute,
                    name: "subject".to_string(),
                    options: Vec::new(),
                }),
            ],
            functions: Vec::new(),
            helpers: Vec::new(),
        };
        let models = build_models(&[empty_reopener, rich]);

        // Exactly one Model with the qualified name — not two.
        assert_eq!(models.iter().filter(|m| m.name == "WorkPackage").count(), 1);
        let wp = models.iter().find(|m| m.name == "WorkPackage").unwrap();
        // Merged content from the rich reopener, undisturbed by the empty one.
        assert_eq!(wp.associations.len(), 1);
        assert_eq!(wp.associations[0].name, "project");
        assert_eq!(wp.concerns.len(), 1);
        assert_eq!(wp.attributes.len(), 1);
    }

    /// First-occurrence wins for the [`Model`] slot; later reopens append
    /// in source-file order. Mirrors the directory walk that produces the
    /// reopener BEFORE the main file alphabetically
    /// (e.g. `work_package/inexistent_work_package.rb` sorts before
    /// `work_package.rb`).
    #[test]
    fn build_models_preserves_first_occurrence_slot_and_source_order() {
        let first = RubyClass {
            name: "WorkPackage".to_string(),
            declarations: vec![Declaration::Association(AssocDecl {
                kind: AssocKind::BelongsTo,
                name: "project".to_string(),
                options: Vec::new(),
            })],
            functions: Vec::new(),
            helpers: Vec::new(),
        };
        let second = RubyClass {
            name: "Other".to_string(),
            declarations: Vec::new(),
            functions: Vec::new(),
            helpers: Vec::new(),
        };
        let third = RubyClass {
            name: "WorkPackage".to_string(),
            declarations: vec![Declaration::Association(AssocDecl {
                kind: AssocKind::BelongsTo,
                name: "author".to_string(),
                options: Vec::new(),
            })],
            functions: Vec::new(),
            helpers: Vec::new(),
        };
        let models = build_models(&[first, second, third]);
        // No duplicate slot for WorkPackage; "Other" sits between the two
        // reopens in the input but the merged WorkPackage keeps the
        // first-occurrence slot (index 0).
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "WorkPackage");
        assert_eq!(models[1].name, "Other");
        // Concatenation in encounter order: project, then author.
        assert_eq!(
            models[0]
                .associations
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>(),
            vec!["project", "author"],
        );
    }

    /// STI is first-non-`None`-wins so an empty reopener cannot strip
    /// inheritance from a class that earlier declared it.
    #[test]
    fn build_models_sti_first_non_none_wins() {
        let with_sti = RubyClass {
            name: "Article".to_string(),
            declarations: vec![Declaration::Sti(ruff_spo_triplet::StiInfo {
                inherits_from: Some("ApplicationRecord".to_string()),
                abstract_class: false,
                inheritance_column: None,
            })],
            functions: Vec::new(),
            helpers: Vec::new(),
        };
        let empty_reopen = RubyClass {
            name: "Article".to_string(),
            declarations: Vec::new(),
            functions: Vec::new(),
            helpers: Vec::new(),
        };
        let models = build_models(&[with_sti, empty_reopen]);
        assert_eq!(models.len(), 1);
        assert!(models[0].sti.is_some(), "STI must survive an empty reopen");
        assert_eq!(
            models[0].sti.as_ref().unwrap().inherits_from.as_deref(),
            Some("ApplicationRecord"),
        );
    }

    #[test]
    fn multi_symbol_callback_emits_one_callback_per_target() {
        // Rails registers one callback per symbol: `before_save :a, :b, :c`
        // is three hooks in order. Dropping targets 2..N silently loses
        // hooks (found on the Redmine corpus: issue.rb declares four
        // before_save targets in one statement).
        let classes = extract_from_source(
            r#"
class Issue < ApplicationRecord
  before_save :close_duplicates, :update_done_ratio, :force_updated
end
"#,
        );
        let targets: Vec<&str> = classes[0]
            .declarations
            .iter()
            .filter_map(|d| match d {
                Declaration::Callback(cb) => Some(cb.target.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            targets,
            ["close_duplicates", "update_done_ratio", "force_updated"]
        );
    }
}
