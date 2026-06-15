//! The language-agnostic intermediate representation.
//!
//! A frontend's ONLY job is to fill a [`ModelGraph`] from its own AST. The
//! Python/Odoo frontend reads `@api.depends`, compute-method bodies, and
//! `raise` statements; a Ruby/Rails frontend reads `ActiveRecord`
//! associations, `validate`/`validates` callbacks, and memoized methods.
//! Both produce the SAME `ModelGraph`, so [`crate::expand`] yields the same
//! triple shape.
//!
//! This IR is intentionally dumb: plain owned data, no behaviour, no
//! parsing. It is the contract seam between "how language X exposes its
//! model graph" and "what the SPO store consumes".
//!
//! # Mapping cheat-sheet (core 7 predicates)
//!
//! | IR field                | Odoo (Python)                       | Rails (Ruby)                                  |
//! | ---                     | ---                                 | ---                                           |
//! | [`Model::name`]         | `_name` / class (`account.move`)    | `ActiveRecord` class (`WorkPackage`)            |
//! | [`Field::name`]         | `fields.X = fields.Type(...)`       | DB column / `attribute` / `attr_accessor`     |
//! | [`Field::depends_on`]   | `@api.depends("a.b.c")` args        | `belongs_to`/`has_many` chains a method reads |
//! | [`Field::emitted_by`]   | `compute="_compute_x"`              | memoized/derived method assigning the attr    |
//! | [`Function::name`]      | `def _compute_x(self)`              | `def compute_x` / instance method             |
//! | [`Function::reads`]     | attribute reads in body             | `self.x` / association reads in body          |
//! | [`Function::raises`]    | `raise UserError(...)`              | `raise`, `errors.add`, `validates`            |
//! | [`Function::traverses`] | `for r in self.line_ids:`           | `work_package.children.each`                  |
//!
//! # `OpenProject` AR-shape (Rails class-body DSL — the 13 [`Model`] siblings)
//!
//! The Rails `ActiveRecord` class body is a much richer DSL than what the
//! core 7 covers. The 13 sibling-shape `Vec<…>` fields on [`Model`] hold
//! the structured class-level facts; [`crate::expand`] turns them into
//! the 27 `OpenProject` AR-shape predicates added in `triple.rs`. Each
//! field is a thin owned struct (no behaviour, no derivation) — the
//! frontend fills them and the expander projects them into triples.

use serde::{Deserialize, Serialize};

/// The whole extracted model graph for one source tree.
///
/// **Schema invariant:** zero new fields here. The IR's growth in the
/// `OpenProject` AR-shape expansion lands inside [`Model`] (13 sibling-shape
/// `Vec<…>` fields), keeping the top-level shape stable for downstream
/// consumers that walk `models` only.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ModelGraph {
    /// The IRI namespace prefix for subjects/objects (e.g. `"odoo"`,
    /// `"openproject"`). Subjects become `"<namespace>:<model>.<member>"`.
    pub namespace: String,
    /// Every model/entity in the tree.
    pub models: Vec<Model>,
}

impl ModelGraph {
    /// Create an empty graph for the given namespace.
    #[must_use]
    pub fn new(namespace: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            models: Vec::new(),
        }
    }
}

/// One model / entity (Odoo model, Rails `ActiveRecord` class).
///
/// The first three fields ([`Self::name`] / [`Self::fields`] /
/// [`Self::functions`]) are the **core** shape — what both the Odoo and
/// Rails frontends fill, and what the original 7 predicates expand.
///
/// The remaining 13 fields are the **`OpenProject` AR-shape** — populated
/// only by the Rails frontend (`ruff_ruby_spo`). The Odoo frontend
/// leaves them at their `Default::default()` empty values; the
/// [`crate::expand`] function silently emits no triples for empty
/// collections, so the Python pipeline is unaffected.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Model {
    /// The model identity — kept exactly as the source names it, except
    /// that dots in Odoo model names (`account.move`) are normalised to
    /// underscores by convention so the IRI dot is unambiguously the
    /// model↔member separator. The frontend owns this normalisation.
    pub name: String,
    /// Fields / attributes / columns.
    pub fields: Vec<Field>,
    /// Methods / functions.
    pub functions: Vec<Function>,

    // ───── OpenProject AR-shape: 12 Vec + 1 Option ─────
    /// Class-level association declarations (`belongs_to`, `has_many`,
    /// `has_one`, `has_and_belongs_to_many`, `accepts_nested_attributes_for`).
    /// Expanded as `declares_association` (`OpenProjectExtracted`) per entry.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub associations: Vec<AssocDecl>,
    /// `validates` / `validate` / `normalizes` / `validates_associated` /
    /// `validates_each` declarations. Expanded as `validates_constraint`
    /// (and `normalizes_attribute` for the `normalizes` variant).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validations: Vec<Validation>,
    /// Callback declarations (`before_save`, `after_create`, …). Expanded
    /// as `has_callback` with the phase encoded in the object slot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub callbacks: Vec<Callback>,
    /// Concern / module composition references (`include`, `extend`,
    /// `prepend`, `class_methods do`, `included do`). Expanded as
    /// `includes_module` / `extends_module` / `prepends_module` /
    /// `concern_class_methods` / `concern_included_block`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub concerns: Vec<ConcernRef>,
    /// Attribute declarations beyond the schema baseline (`attribute`,
    /// `attr_accessor`, `attr_reader`, `attr_readonly`, `alias_attribute`,
    /// `alias_method`, `alias`, `undef_method`, `serialize`, `enum`,
    /// `store_attribute`, `store_accessor`, `define_attribute_method`).
    /// Expanded as `has_attribute` / `aliases_attribute` / `aliases_method` /
    /// `column_override` per kind.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<AttrDecl>,
    /// `delegate :foo, :bar, to: :baz`. Expanded as `delegates_to` —
    /// one triple per (method, to) pair.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delegations: Vec<Delegation>,
    /// `scope` / `default_scope` / `scopes` (OP plural). Expanded as
    /// `has_scope` / `has_default_scope` with the lambda body kept as
    /// a body-source ref (per existing `scope` precedent).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<ScopeDecl>,
    /// `OpenProject` `acts_as_*` family declarations. Expanded as `acts_as`
    /// with the variant + options in the object slot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acts_as: Vec<ActsAs>,
    /// `OpenProject` custom DSL registrations (`register_journal_formatter`,
    /// `register_journal_formatted_fields`, plus the long-tail singletons).
    /// Expanded as `registers_journal_formatter` /
    /// `registers_journal_formatted_fields` (the two promoted predicates)
    /// or `has_dsl_call` (catch-all) per name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dsl_calls: Vec<DslCall>,
    /// Third-party gem DSL (`mount_uploader`, `has_paper_trail`,
    /// `has_closure_tree`, `counter_culture`, `auto_strip_attributes`).
    /// Expanded as `mounts_uploader` / `has_paper_trail` / `has_closure_tree` /
    /// `counter_cultures` / `auto_strips` per gem.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gem_dsl: Vec<GemDsl>,
    /// `define_method` sites. Expanded as `defines_method` with
    /// [`crate::Provenance::Inferred`] (per-edge override on top of the
    /// predicate's `Inferred` default — dynamic by definition).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dynamic_methods: Vec<DynMethod>,
    /// `using Refinement` declarations (2 sites in the `OpenProject` corpus).
    /// Expanded as `uses_refinement`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refinements: Vec<UsingRef>,
    /// Single-Table Inheritance metadata. `None` for non-STI classes.
    /// Currently only the `inherits_from` parent is emitted (as
    /// `includes_module`); `abstract_class` + `inheritance_column` are
    /// carried for downstream consumers but produce no triples here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sti: Option<StiInfo>,
}

/// One field / attribute / column.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Field {
    /// Field name (e.g. `amount_total`).
    pub name: String,
    /// Declared compute dependencies — dotted relation paths
    /// (`line_ids.balance`). Emitted as `depends_on` (Authoritative).
    pub depends_on: Vec<String>,
    /// The function that computes/writes this field, if any. Emitted as
    /// `(field, emitted_by, fn)` (Authoritative).
    pub emitted_by: Option<String>,
}

/// One method / function.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Function {
    /// Function name (e.g. `_compute_amount`).
    pub name: String,
    /// Field names this function reads in its body. Emitted as
    /// `reads_field` (Inferred).
    pub reads: Vec<String>,
    /// Exception/error type names this function raises. Emitted as
    /// `raises` against the `exc:` namespace (Authoritative).
    pub raises: Vec<String>,
    /// Relation names this function traverses (loop targets). Emitted as
    /// `traverses_relation` (Inferred).
    pub traverses: Vec<String>,
}

impl Model {
    /// Convenience constructor for a bare model with no members yet.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// OpenProject AR-shape declarative types
// ─────────────────────────────────────────────────────────────────────────

/// One class-level association declaration.
///
/// The Rails frontend emits one of these per `belongs_to` / `has_many` /
/// `has_one` / `has_and_belongs_to_many` / `accepts_nested_attributes_for`
/// macro call. Options are kept as a `(key, value)` list so the
/// 10 nested options (`class_name`, `dependent`, `optional`, `inverse_of`,
/// `through`, `polymorphic`, `foreign_key`, `as`, `source`, `touch`) are
/// represented verbatim without a 10-way enum that would couple the IR
/// to today's option set.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AssocDecl {
    /// The macro that declared this association.
    pub kind: AssocKind,
    /// The relation symbol (e.g. `project` from `belongs_to :project`).
    pub name: String,
    /// Nested options, verbatim, in source order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<(String, String)>,
}

/// The 5 Rails association macros.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssocKind {
    /// `belongs_to :rel`
    BelongsTo,
    /// `has_many :rel`
    HasMany,
    /// `has_one :rel`
    HasOne,
    /// `has_and_belongs_to_many :rel`
    HasAndBelongsToMany,
    /// `accepts_nested_attributes_for :rel`
    AcceptsNestedAttributesFor,
}

/// One validation declaration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Validation {
    /// The validation macro variant.
    pub kind: ValidationKind,
    /// Attribute name, method name, or `"<block>"` for block-form
    /// `validate { … }`.
    pub target: String,
    /// Validation options (presence / numericality / format / inclusion /
    /// length / uniqueness / etc.), verbatim.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<(String, String)>,
}

/// The 5 Rails validation macros.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ValidationKind {
    /// `validates :attr, presence: true`
    Validates,
    /// `validate :method_name` or `validate { … }`
    Validate,
    /// `normalizes :attr, with: …` (kept as `ValidationKind` because the
    /// frontend collects it alongside validations; the expander emits
    /// `normalizes_attribute` distinct from `validates_constraint`).
    Normalizes,
    /// `validates_associated :rel`
    ValidatesAssociated,
    /// `validates_each :attr, :attr2 { |record, attr, value| … }`
    ValidatesEach,
}

/// One callback declaration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Callback {
    /// The callback phase (e.g. `"before_save"`, `"after_create"`,
    /// `"around_destroy"`, `"after_destroy_commit"`). Kept as a string
    /// because the phase set is 13+ entries and Rails adds more
    /// (`after_create_commit`, etc.) — the IR doesn't gate.
    pub phase: String,
    /// Method symbol or block ref the callback dispatches to.
    pub target: String,
    /// Conditional options (`if:`, `unless:`, `on:`), verbatim.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<(String, String)>,
}

/// One concern / module composition reference.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConcernRef {
    /// How the module is composed.
    pub kind: ConcernKind,
    /// Module name (e.g. `Redmine::Acts::Customizable`). For
    /// [`ConcernKind::ClassMethodsBlock`] and
    /// [`ConcernKind::IncludedBlock`] this is the *enclosing* concern's
    /// own name (the block runs on `self.included` / `class_methods`).
    pub module: String,
    /// Body source ref for `class_methods do` / `included do` blocks.
    /// `None` for ordinary `include` / `extend` / `prepend`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_ref: Option<String>,
}

/// The 5 Rails module-composition forms covered by the AR-shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConcernKind {
    /// `include Mod` — mix into instance method namespace.
    Include,
    /// `extend Mod` — mix into singleton (class) method namespace.
    Extend,
    /// `prepend Mod` — mix in BEFORE the class itself in MRO.
    Prepend,
    /// `class_methods do … end` inside a concern.
    ClassMethodsBlock,
    /// `included do … end` inside a concern.
    IncludedBlock,
}

/// One attribute declaration (non-DB-column).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AttrDecl {
    /// The declaration macro variant.
    pub kind: AttrKind,
    /// The attribute name (or `<new>=<orig>` for aliases).
    pub name: String,
    /// Type / serializer / enum-mapping / store-key options, verbatim.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<(String, String)>,
}

/// The 13 Rails attribute-declaration macros covered by the AR-shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AttrKind {
    /// `attribute :x, :type`
    Attribute,
    /// `attr_accessor :x`
    AttrAccessor,
    /// `attr_reader :x`
    AttrReader,
    /// `attr_readonly :x` (Rails read-only column marker)
    AttrReadonly,
    /// `alias_attribute :new, :orig` (attribute-level alias)
    AliasAttribute,
    /// `alias_method :new, :orig` (method-level alias, explicit form)
    AliasMethod,
    /// `alias new orig` (method-level alias, sugar form)
    Alias,
    /// `undef_method :foo`
    UndefMethod,
    /// `serialize :data, JSON`
    Serialize,
    /// `enum :status, { … }`
    Enum,
    /// `store_attribute :store_key, :attr, :type`
    StoreAttribute,
    /// `store_accessor :store_key, :attr1, :attr2`
    StoreAccessor,
    /// `define_attribute_method :attr` (Rails-internal)
    DefineAttributeMethod,
}

/// One `delegate` declaration. A single `delegate :foo, :bar, to: :baz`
/// expands to one [`Delegation`] with `methods = ["foo", "bar"]` and
/// `to = "baz"`; the expander unwinds it into one `delegates_to` triple
/// per method.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Delegation {
    /// Method names being delegated.
    pub methods: Vec<String>,
    /// The receiver (association name or method symbol).
    pub to: String,
    /// `prefix:` / `allow_nil:` / `private:`, verbatim.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<(String, String)>,
}

/// One scope declaration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ScopeDecl {
    /// `scope` / `default_scope` / `scopes` (OP plural form).
    pub kind: ScopeKind,
    /// Scope name (empty string for `default_scope`).
    pub name: String,
    /// Lambda body source ref, kept verbatim per the existing
    /// `Function::reads` "preserve body shape" precedent.
    pub body_ref: String,
}

/// The 3 Rails scope-declaration macros covered by the AR-shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScopeKind {
    /// `scope :name, -> { … }`
    Scope,
    /// `default_scope -> { … }`
    DefaultScope,
    /// `scopes :name1, :name2` — `OpenProject` plural form.
    Scopes,
}

/// One `acts_as_*` declaration. The variant lives in the `name` field
/// (`"list"`, `"attachable"`, `"watchable"`, `"searchable"`,
/// `"journalized"`, `"event"`, `"customizable"`, `"tree"`,
/// `"favoritable"`, `"url"`) — kept as a string because new variants
/// arrive without ontology changes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActsAs {
    /// The variant suffix (e.g. `"list"` for `acts_as_list`).
    pub variant: String,
    /// Options to the macro call, verbatim.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<(String, String)>,
}

/// One `OpenProject` custom DSL call. The frontend writes one of these
/// for every class-body method call that isn't covered by another more
/// specific declaration type; the expander routes by `name` into either
/// a promoted predicate (`registers_journal_formatter`,
/// `registers_journal_formatted_fields`) or the catch-all `has_dsl_call`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DslCall {
    /// The DSL call name (e.g. `"register_journal_formatter"`).
    pub name: String,
    /// Args, verbatim, preserved as a single string for queryability.
    pub args: String,
}

/// One third-party gem DSL call.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GemDsl {
    /// Which gem's DSL.
    pub gem: GemKind,
    /// Args, verbatim.
    pub args: String,
}

/// The 5 third-party gem DSLs covered by the AR-shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GemKind {
    /// `CarrierWave`'s `mount_uploader :attr, Class`.
    MountUploader,
    /// `has_paper_trail` (audit log).
    HasPaperTrail,
    /// `has_closure_tree` (tree structures).
    HasClosureTree,
    /// `counter_culture` (denormalised counter columns).
    CounterCulture,
    /// `auto_strip_attributes` (whitespace strip on assignment).
    AutoStripAttributes,
}

/// One `define_method` dynamic-method site. The default expander
/// emission uses [`crate::Provenance::Inferred`] for these — dynamism
/// makes static identification heuristic by definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DynMethod {
    /// The name expression — a literal symbol if the source is
    /// `define_method(:foo) { … }`, or an arbitrary Ruby expression for
    /// `define_method("dynamic_#{x}") { … }`.
    pub name_expr: String,
    /// Body source ref.
    pub body_ref: String,
}

/// One `using SomeRefinement` declaration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UsingRef {
    /// The refinement module name.
    pub refinement_module: String,
}

/// Single-Table Inheritance metadata. Carried on [`Model::sti`] when the
/// class participates in STI.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StiInfo {
    /// `class X < Parent` — the parent class name when not
    /// `ApplicationRecord` / `ActiveRecord::Base`. Becomes an
    /// `includes_module` triple in the expander.
    pub inherits_from: Option<String>,
    /// `self.abstract_class = true`.
    #[serde(default)]
    pub abstract_class: bool,
    /// `self.inheritance_column = "type"` — the column STI dispatches
    /// on (default `"type"` if not overridden).
    pub inheritance_column: Option<String>,
}
