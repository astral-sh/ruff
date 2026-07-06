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
    /// Non-public (`private`/`protected`) defs — same [`Function`] body
    /// facts as `functions`, but NOT routable actions: [`crate::expand`]
    /// emits no triples for them (no `has_function`), keeping the action
    /// surface unchanged. Carried in the IR because Rails lifecycle
    /// callbacks conventionally target private methods and body-fact
    /// analysis (OGAR F17 body triage) needs to resolve those hook
    /// targets. Additive + serde-defaulted: existing dumps deserialize
    /// with an empty vec.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub helpers: Vec<Function>,

    /// Frontend-agnostic prototype/delegation inheritance — the parent
    /// models this model `inherits_from` (Odoo `_inherit`, and any future
    /// language's plain "extends `<name>`"). Names are already
    /// frontend-normalised (dot→underscore); the expander emits
    /// `(ns:model, inherits_from, ns:parent)` per entry with
    /// [`Provenance::Authoritative`]. Distinct from `bases` (C++ base
    /// classes, `CppExtracted`) and `sti` (single-parent Rails STI): a
    /// multi-parent list carrying no per-parent metadata. Self-references
    /// (an Odoo reopen where the sole `_inherit` == the model name) are
    /// excluded by the frontend, so this never emits a `model inherits_from
    /// model` self-edge.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inherits: Vec<String>,

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

    // ───── C++ machine-plane: 7 sibling Vecs (filled only by ruff_cpp_spo) ─────
    //
    // Populated only by the C++ frontend (`ruff_cpp_spo`); the Python/Ruby
    // frontends leave them at `Default::default()` empty, and
    // `skip_serializing_if` keeps their ndjson byte-identical. The expander
    // emits no triples for empty collections, so no other pipeline is
    // affected.
    /// Base-class declarations (`class X : public Base`). Expanded as
    /// `inherits_from` (`CppExtracted`) per base.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bases: Vec<CppBase>,
    /// Data-member declarations. Expanded as `has_field` (`CppExtracted`)
    /// plus a structural `(class.field, rdf:type, Property)` classification.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub member_fields: Vec<CppField>,
    /// Method declarations carrying their C++ property flags (virtual /
    /// override / pure-virtual / constexpr / noexcept / operator / requires).
    /// Each method is classified `(class.method, rdf:type, Function)` +
    /// `(class, has_function, class.method)`; the flags expand to the
    /// method-property predicates.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<CppMethod>,
    /// Template specialisation / instantiation declarations. Expanded as
    /// `template_specialises` (`CppExtracted`) / `template_instantiates`
    /// (`Inferred`) per kind.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub templates: Vec<CppTemplate>,
    /// `friend class` / `friend fn` declarations. Expanded as `is_friend_of`
    /// (Structural).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub friends: Vec<CppFriend>,
    /// Identifiers originating from preprocessor macro expansion. Expanded
    /// as `uses_macro_expansion` (`Inferred`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub macro_uses: Vec<CppMacroUse>,
    /// `static_assert` declarations in class scope. Expanded as
    /// `static_asserts` (`CppExtracted`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub static_asserts: Vec<CppStaticAssert>,
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
    /// For a relational field, the comodel as the raw dotted Odoo model
    /// name (`res.partner`). Emitted as `(field, target, "<comodel>")`
    /// (Authoritative) — the object is the string verbatim, NOT an IRI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// For a One2many field, the inverse Many2one field name (`move_id`),
    /// raw. Emitted as `(field, inverse_name, "<inverse>")` (Authoritative).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inverse_name: Option<String>,
    /// For a relational field, the Odoo constructor lowercased (`many2one`
    /// / `one2many` / `many2many`). Emitted as `(field, relation_kind,
    /// "<kind>")` (Authoritative). Disambiguates a Many2one (scalar FK)
    /// from a Many2many (join table) — both carry only a `target` and no
    /// `inverse_name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_kind: Option<String>,
    /// For a **non-relational** field, the Odoo constructor lowercased
    /// (`char` / `text` / `html` / `integer` / `float` / `monetary` /
    /// `boolean` / `date` / `datetime` / `binary` / `selection` / `json` /
    /// …). Emitted as `(field, field_type, "<kind>")` (Authoritative) —
    /// the same `field_type` predicate the Rails `AttrDecl` path uses. Lets
    /// a downstream lift upgrade an untyped scalar (`OgScalar`) into a
    /// concrete typed wrapper. Mutually exclusive with [`Self::relation_kind`]:
    /// relational fields carry their cardinality there, scalars carry their
    /// constructor here, so the two predicates never double-emit for one field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_type: Option<String>,
    /// For a **DB-column** field (D-AR-3.5 schema stratum: extracted from
    /// the Rails migration DSL, `db/migrate/tables/*.rb`), `Some(true)`
    /// when the column carries `null: false`. Emitted as
    /// `(field, column_not_null, "true")` — only for `Some(true)`; `None`
    /// / `Some(false)` emit nothing (nullable is the Rails default, and
    /// absence-means-nullable keeps the triple volume proportional to the
    /// constraint count). Downstream this is the `required` axis of
    /// `DEFINE FIELD` (`TYPE <t>` vs `TYPE option<t>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_null: Option<bool>,
    /// `store=True` on a computed Odoo field — the compute result is
    /// persisted in a DB column rather than recomputed on read. `None` when
    /// the constructor carries no `store=` kwarg (Odoo default: not stored for
    /// computed fields). Emitted as `(field, stored, "true")` — only for
    /// `Some(true)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stored: Option<bool>,
}

/// One method / function.
///
/// The first four fields are the original query-shape facts (what the body
/// *reads*). The last two — [`Self::writes`] and [`Self::calls`] — are the
/// **command-shape** facts (what the body *mutates* / *dispatches*), added so
/// the body-pass triage can split a method into query (read-only) vs command
/// (writes state) — the accidentally-imperative-vs-essentially-foreign cut
/// (E-ACCIDENTAL-IMPERATIVE / OGAR F17). Both are `skip_serializing_if`-empty,
/// so a frontend that doesn't populate them (Odoo Python today) leaves the
/// ndjson byte-identical.
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
    /// Field names this function WRITES via a `self.<field> = …` setter call
    /// in its body. Emitted as `writes_field` (Authoritative — the assignment
    /// names its target unambiguously; only the value is uncertain). The
    /// command-side counterpart of [`Self::reads`]; together they let the
    /// triage classify a method as read-only vs mutating. Plain instance-var
    /// assignment (`@x = …`, local memoization) is deliberately NOT a write.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub writes: Vec<String>,
    /// Fields whose write is **guarded by a blank/nil test on that same field**
    /// (`self.x = v if self.x.blank?`, or the nil/false-guarded `self.x ||= v`
    /// — a narrower falsy test than `.blank?`, but the same "absent" guard for
    /// J1 purposes). The J1 fact (`.claude/knowledge/fuzzy-recipe-codebook.md`
    /// §5) that splits the fuzzy `SelfMap` recipe into schema-default
    /// (present) vs `normalizes` (absent). Always a subset of `writes`;
    /// emitted as `writes_if_blank` (Authoritative). Additive + serde-defaulted
    /// (existing dumps deserialize with an empty vec).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guarded_writes: Vec<String>,
    /// Lifecycle-mutator calls the body dispatches, as `"<receiver>.<method>"`
    /// (e.g. `self.save`, `order.update`, `line_ids.destroy_all`). Only the
    /// closed `ActiveRecord` mutator set is captured (create/update/save/
    /// destroy/…) — not every call — because the signal the body-pass triage
    /// needs is "this method calls a writer", i.e. it dispatches a lifecycle
    /// verb on some target. Emitted as `calls` (Inferred — receiver
    /// resolution is heuristic at static-AST time; the verb itself is a
    /// literal).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calls: Vec<String>,
    /// Field paths that trigger this method as an `@api.constrains` validation.
    /// Emitted as `constrains` (Authoritative — the decorator names them).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constrains: Vec<String>,
    /// Field paths that trigger this method as an `@api.onchange` UI recompute.
    /// Emitted as `onchange` (Authoritative — the decorator names them).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub onchange: Vec<String>,
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

// ─────────────────────────────────────────────────────────────────────────
// C++ machine-plane declarative types (filled only by ruff_cpp_spo)
// ─────────────────────────────────────────────────────────────────────────

/// One base-class declaration (`class Derived : public Base`).
///
/// The expander emits `(class, inherits_from, ns:base)` with the access
/// specifier and virtual-inheritance flag carried here on the IR but not
/// in the triple — the object stays a clean base-class IRI for graph
/// traversal (mirroring how [`AssocDecl::kind`] is carried but not emitted).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CppBase {
    /// Base class name as resolved by the AST (e.g. `Tesseract::Classify`).
    pub name: String,
    /// `public` / `protected` / `private` inheritance.
    pub access: CppAccess,
    /// `class X : virtual public Base` — virtual (diamond-resolving) base.
    #[serde(default)]
    pub virtual_base: bool,
}

/// C++ access specifiers for inheritance + members.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum CppAccess {
    /// `public` — visible everywhere.
    #[default]
    Public,
    /// `protected` — visible to the class and its derivatives.
    Protected,
    /// `private` — visible only to the class itself.
    Private,
}

/// One data-member declaration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CppField {
    /// Member name (e.g. `recognizer_`).
    pub name: String,
    /// Resolved type, verbatim (e.g. `std::unique_ptr<LSTMRecognizer>`).
    /// Carried for downstream consumers; not emitted in the triple.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub type_name: String,
}

/// One method declaration carrying its C++ property flags.
///
/// Every method is classified (`rdf:type Function` + `has_function`); each
/// set flag additionally expands to a method-property predicate. The flags
/// are not mutually exclusive (a method can be both `constexpr` and
/// `noexcept`, an `operator` and an `override`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent C++ method qualifiers (pure-virtual / noexcept / const / static) — \
              not a state machine; any combination is valid, so two-variant enums would be artificial"
)]
pub struct CppMethod {
    /// Method name (e.g. `Recognize`). For operators, the spelled name
    /// (e.g. `operator==`); the operator kind is also set in
    /// [`Self::operator_kind`] so the classification IRI stays stable.
    pub name: String,
    /// `virtual ... = 0` pure-virtual declaration → `is_pure_virtual`.
    #[serde(default)]
    pub is_pure_virtual: bool,
    /// `constexpr` / `consteval` marker → `is_constexpr` (the kind rides
    /// the object slot). `None` for ordinary runtime methods.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constexpr_kind: Option<ConstexprKind>,
    /// `noexcept` exception specification → `is_noexcept`.
    #[serde(default)]
    pub is_noexcept: bool,
    /// `override` of a virtual base method → `virtually_overrides`. The
    /// value is the **fully-qualified** overridden base method
    /// (`Namespace::Base.method`), so the emitted `{ns}:` IRI joins the base
    /// class's own method node. A bare `Base.method` would dangle for any
    /// namespaced base (the base class is modeled as `{ns}:Namespace::Base`)
    /// — codex P2, PR #8.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overrides: Option<String>,
    /// Operator overload kind (e.g. `operator==`) → `defines_operator`.
    /// `None` for non-operator methods.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator_kind: Option<String>,
    /// C++20 `requires` clause, verbatim → `requires_concept`. `None` when
    /// unconstrained.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_clause: Option<String>,
    /// Return type, verbatim (e.g. `bool`, `const char *`) → `returns_type`.
    /// `None` (and not emitted) for `void` / constructors / destructors — the
    /// AST-DLL shape treats absent `returns_type` as "no value returned".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    /// Parameter types in signature order, verbatim → one `has_param_type` each.
    /// Order + arity are preserved by the `<index>:<type>` object encoding the
    /// expander emits (a triple set is unordered, so the position rides the
    /// object). The AST-DLL codegen reconstructs the ordered signature from
    /// `return_type` + these.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_types: Vec<String>,
    /// `T method() const;` — a const-qualified member function → `is_const`.
    /// The ORM-downcast shape: a const method is a read accessor (no mutation).
    #[serde(default)]
    pub is_const: bool,
    /// `static T method();` — a static member function → `is_static`
    /// (class-level, no implicit `this`).
    #[serde(default)]
    pub is_static: bool,
    /// Member access specifier → `has_visibility`. The OO API-surface +
    /// intrusiveness signal (public = adapter surface; private/protected =
    /// likely internal). Defaults to `Public`.
    #[serde(default)]
    pub access: CppAccess,
}

/// `constexpr` vs `consteval` compile-time markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConstexprKind {
    /// `constexpr` — usable in a constant expression.
    Constexpr,
    /// `consteval` — an immediate function (MUST run at compile time).
    Consteval,
}

/// One template specialisation or instantiation declaration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CppTemplate {
    /// Explicit specialisation vs materialised instantiation.
    pub kind: CppTemplateKind,
    /// Template name + arguments, verbatim (e.g. `GenericVector<int>`).
    pub name: String,
}

/// Whether a [`CppTemplate`] is an explicit specialisation or a
/// materialised instantiation visible in the translation unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CppTemplateKind {
    /// `template <> class Foo<int> { … }` — explicit (partial or full)
    /// specialisation. Expanded as `template_specialises` (`CppExtracted`).
    Specialisation,
    /// `Foo<int>` materialised in this TU. Expanded as
    /// `template_instantiates` (`Inferred` — single-TU view is incomplete).
    Instantiation,
}

/// One `friend class` / `friend fn` declaration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CppFriend {
    /// The friend class or function name, verbatim.
    pub name: String,
}

/// One identifier originating from a preprocessor macro expansion.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CppMacroUse {
    /// The identifier that was produced by the expansion.
    pub identifier: String,
    /// The macro it expanded from.
    pub macro_name: String,
}

/// One `static_assert` declaration in class scope.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CppStaticAssert {
    /// The asserted condition, verbatim.
    pub condition: String,
}
