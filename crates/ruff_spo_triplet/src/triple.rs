//! The triple itself + its closed predicate / entity-kind / provenance
//! vocabularies.
//!
//! These four types are the entire ontological surface. Everything a
//! language frontend produces collapses into a `Vec<Triple>` whose `p`
//! field is one of [`Predicate`], whose `rdf:type` objects are one of
//! [`EntityKind`], and whose `(f, c)` truth comes from a [`Provenance`]
//! tier. Keeping these closed is what lets the Python (Odoo) and Ruby
//! (Rails) frontends emit byte-identical graphs.

use serde::{Deserialize, Serialize};

/// One SPO triple with NARS truth `(frequency, confidence)`.
///
/// `(s, p, o)` is the identity. `(f, c)` carries provenance strength:
/// structural facts are certain, decorator/body-authoritative facts are
/// strong, body-inferred facts are weaker. The downstream store
/// (`lance_graph::graph::spo`) gates queries by NARS expectation, so the
/// truth tier is load-bearing, not decorative.
///
/// This mirrors `lance_graph::graph::spo::odoo_ontology::OntologyTriple`
/// field-for-field so the ndjson this crate writes loads into that store
/// with no transform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Triple {
    /// Subject IRI, e.g. `odoo:account_move.amount_total`.
    pub s: String,
    /// Predicate IRI, e.g. `depends_on`.
    pub p: String,
    /// Object IRI, e.g. `odoo:account_move.line_ids.balance`.
    pub o: String,
    /// NARS frequency.
    pub f: f32,
    /// NARS confidence.
    pub c: f32,
}

impl Triple {
    /// Construct a triple from typed parts + a provenance tier.
    #[must_use]
    #[expect(
        clippy::many_single_char_names,
        reason = "s/p/o/f/c are Triple's canonical SPO + NARS-truth field names"
    )]
    pub fn new(s: impl Into<String>, p: Predicate, o: impl Into<String>, prov: Provenance) -> Self {
        let (f, c) = prov.truth();
        Self {
            s: s.into(),
            p: p.as_str().to_string(),
            o: o.into(),
            f,
            c,
        }
    }

    /// The identity key — what de-duplication and round-trip equality
    /// compare. Truth values are deliberately excluded.
    #[must_use]
    pub fn key(&self) -> (&str, &str, &str) {
        (&self.s, &self.p, &self.o)
    }
}

/// The closed predicate vocabulary.
///
/// Adding a predicate is a deliberate ontology change: a new variant here,
/// a new arm in [`Predicate::as_str`] / [`Predicate::from_str`], and a
/// decision about which [`Provenance`] tier it carries. Frontends MUST NOT
/// emit raw predicate strings — they go through this enum so the Python and
/// Ruby graphs cannot drift.
///
/// # Origin tiers (count = 34)
///
/// 1. **Core 7** (`RdfType` … `TraversesRelation`) — Odoo Python harvest,
///    the original Foundry-shape ontology. Object/Property/Function with
///    declared / body-authoritative / body-inferred truth tiers.
/// 2. **`OpenProject` AR-shape 27** (`DeclaresAssociation` … `UsesRefinement`)
///    — Rails `ActiveRecord` class-body DSL surface, measured on the
///    `OpenProject` corpus (941 models / 1696 declarations / 78 distinct
///    names → 67 emit categories + 11 scope markers; the 27 here cover
///    every emit category that is not already in the core 7). Default
///    tier: [`Provenance::OpenProjectExtracted`] — one notch below
///    [`Provenance::Authoritative`] (Odoo `@api.depends`) to encode the
///    Ruby metaprogramming surface delta. Two per-edge overrides:
///    [`Self::DefinesMethod`] defaults to [`Provenance::Inferred`]
///    (dynamic-method finds are heuristic by definition);
///    [`Self::ConcernIncludedBlock`] / [`Self::ConcernClassMethods`] are
///    structural-by-construction and carry [`Provenance::Structural`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Predicate {
    // ───── Core (Odoo Python harvest) ─────
    /// `(subject, rdf:type, EntityKind)` — structural classification.
    RdfType,
    /// `(model, has_function, model.fn)` — a model owns a function.
    HasFunction,
    /// `(model.field, emitted_by, model.fn)` — the function writes the field.
    EmittedBy,
    /// `(model.field, depends_on, model.dep)` — declared compute dependency.
    DependsOn,
    /// `(model.fn, reads_field, model.field)` — body reads the field.
    ReadsField,
    /// `(model.fn, raises, exc:Type)` — body raises the exception.
    Raises,
    /// `(model.fn, traverses_relation, model.rel)` — body walks the relation.
    TraversesRelation,

    // ───── OpenProject AR-shape (Rails class-body DSL) ─────
    //
    // Subject is the *class* (not a function) for declarative-level facts;
    // the function-level analogue stays on the core 7 (see TraversesRelation
    // for the body-walk counterpart of DeclaresAssociation).
    /// `(model, declares_association, model.<rel>)` — a class-level
    /// `belongs_to` / `has_many` / `has_one` / `has_and_belongs_to_many` /
    /// `accepts_nested_attributes_for` declaration. **Distinct from**
    /// [`Self::TraversesRelation`] (subject = fn, body-walked, Inferred):
    /// this is class-level + OpenProject-extracted-grade because the
    /// declaration is machine-readable in the AST.
    DeclaresAssociation,
    /// `(model, validates_constraint, attr|"block")` — `validates` /
    /// `validate` / `validates_associated` / `validates_each`. The verb
    /// form (vs. the planned `has_constraint`) disambiguates from
    /// declarative `has_*` predicates.
    ValidatesConstraint,
    /// `(model, normalizes_attribute, attr)` — `normalizes :attr, with:`
    /// declaration. Distinct from `ValidatesConstraint` because the
    /// transformation runs on assignment, not on validation.
    NormalizesAttribute,
    /// `(model, has_callback, "<phase>:<target>")` — `before_save :foo`,
    /// `after_create :bar`, etc. The 12 phases are encoded in the object
    /// slot, not as separate predicates, so the vocab stays bounded.
    HasCallback,
    /// `(model, includes_module, mod)` — `include ModX`. Distinct verb
    /// per Rails composition rule (include / extend / prepend each have
    /// different MRO effects).
    IncludesModule,
    /// `(model, extends_module, mod)` — `extend ModX`.
    ExtendsModule,
    /// `(model, prepends_module, mod)` — `prepend ModX` (rare; 4 sites).
    PrependsModule,
    /// `(concern_mod, concern_class_methods, "block")` — `class_methods do
    /// … end` block inside a concern. Marker-only: structural-by-
    /// construction; the contained `def`s become regular `has_function`
    /// triples in the second pass.
    ConcernClassMethods,
    /// `(concern_mod, concern_included_block, "block")` — `included do
    /// … end` block inside a concern. Same shape as `ConcernClassMethods`.
    ConcernIncludedBlock,
    /// `(model, has_attribute, attr)` — `attribute :x, :type` /
    /// `attr_accessor :x` / `attr_reader :x` / `attr_readonly :x` /
    /// `store_attribute …` / `store_accessor …` / `serialize :x` /
    /// `enum :x` / `define_attribute_method :x`. The unified declaration
    /// surface for non-DB-column attributes.
    HasAttribute,
    /// `(model, aliases_attribute, "<new>=<orig>")` — `alias_attribute`.
    AliasesAttribute,
    /// `(model, aliases_method, "<new>=<orig>")` — `alias_method` /
    /// `alias`. Method-level alias (vs. attribute-level above).
    AliasesMethod,
    /// `(model.column, column_override, "<key>=<value>")` — DSL that
    /// overrides a column's behavior (e.g. `serialize :data, JSON`,
    /// `undef_method :foo`). Marker for non-default column treatment.
    ColumnOverride,
    /// `(model, delegates_to, "<method>=>via:<assoc>")` — `delegate :foo,
    /// :bar, to: :baz`. Object encodes one method per triple (multiple
    /// methods in one `delegate` call expand to multiple triples).
    DelegatesTo,
    /// `(model, has_scope, "<name>=<lambda_body_ref>")` — `scope :active,
    /// -> { where(active: true) }`. Lambda body kept as a body-source ref
    /// (per `scope` precedent).
    HasScope,
    /// `(model, has_default_scope, "<lambda_body_ref>")` — `default_scope
    /// -> { … }`. One per class at most.
    HasDefaultScope,
    /// `(model, acts_as, "<variant>[:<options>]")` — the `acts_as_*`
    /// family (`acts_as_list`, `acts_as_attachable`, …). The variant
    /// name + options live in the object slot; the predicate is one.
    ActsAs,
    /// `(model, registers_journal_formatter, "<key>=<formatter>")` —
    /// `OpenProject`'s `register_journal_formatter` (27 sites). Promoted
    /// out of `has_dsl_call` per iron-rule bulk (74 % of the OP custom
    /// registration mass).
    RegistersJournalFormatter,
    /// `(model, registers_journal_formatted_fields, "<key>")` —
    /// `OpenProject`'s `register_journal_formatted_fields` (13 sites).
    /// Same promotion rationale as `RegistersJournalFormatter`.
    RegistersJournalFormattedFields,
    /// `(model, has_dsl_call, "<name>(<args>)")` — long-tail catch-all
    /// for `OpenProject` custom registrations (`register_query`,
    /// `activity_provider_for`, `deprecated_alias`,
    /// `associated_to_ask_before_destruction`, `has_details_table` —
    /// 5 singletons total, ≤ 6 sites each). Keeps the closed vocab from
    /// growing for every one-off DSL while preserving queryability via
    /// the name slot.
    HasDslCall,
    /// `(model, mounts_uploader, "<attr>=<uploader_class>")` —
    /// `CarrierWave`'s `mount_uploader :attr, Class`.
    MountsUploader,
    /// `(model, has_paper_trail, "<options>")` — `paper_trail` gem's
    /// `has_paper_trail` declaration.
    HasPaperTrail,
    /// `(model, has_closure_tree, "<options>")` — `closure_tree` gem's
    /// `has_closure_tree` declaration.
    HasClosureTree,
    /// `(model, counter_cultures, "<column>=<via>")` — `counter_culture`
    /// gem's denormalised-counter declaration.
    CounterCultures,
    /// `(model, auto_strips, "<attrs>")` — `auto_strip_attributes` gem's
    /// declaration.
    AutoStrips,
    /// `(model, defines_method, "<name>=<body_source_ref>")` —
    /// `define_method` dynamic method synthesis. **Default truth tier
    /// is `Inferred`** (dynamic = heuristic): the 24 sites in the
    /// `OpenProject` corpus are inherently un-statically-resolvable.
    DefinesMethod,
    /// `(model, uses_refinement, "<refinement_module>")` — `using
    /// Refinement` declaration.
    UsesRefinement,
    /// `(model.field, field_type, "<rails_type>")` — the static type
    /// annotation from a Rails `attribute :name, :type` declaration
    /// (e.g. `:integer`, `:string`, `:boolean`, `:datetime`,
    /// `:decimal`). Authoritative-grade because the type symbol is a
    /// machine-readable AST literal. Downstream consumers
    /// (e.g. `op-surreal-ast::from_triples`) map the rails-type
    /// string to a `SurrealQL` `Kind` variant.
    FieldType,
    /// `(model.<rel>, association_kind, "<kind>")` — the Rails
    /// association macro that declared the relation, where `<kind>`
    /// is one of `belongs_to`, `has_many`, `has_one`,
    /// `has_and_belongs_to_many`, `accepts_nested_attributes_for`.
    /// Sibling to [`Self::DeclaresAssociation`] (which carries the
    /// existence fact but drops the kind).
    ///
    /// **Why this matters for schema codegen:** only `belongs_to`
    /// puts a FK column on the declaring class — for `has_many`/
    /// `has_one` the FK lives on the OTHER table. A consumer that
    /// emits a `record<Target>` FK for every `declares_association`
    /// triple produces ~1.9× the columns that actually exist in the
    /// DB. The kind triple lets `op-surreal-ast::from_triples` gate
    /// FK emission on `kind == belongs_to`.
    AssociationKind,

    /// `(<model>.<rel>, class_name, "<TargetClass>")` — Rails
    /// `class_name:` association option override (`belongs_to :owner,
    /// class_name: 'User'`).
    ///
    /// Subject is the relation IRI (`openproject:WorkPackage.owner`),
    /// object is the Ruby class name verbatim (`"User"`). When
    /// present, downstream consumers MUST use this as the target
    /// class instead of inferring it from the Rails camelcase-singular
    /// convention on the relation name. Without this, the schema
    /// emits a phantom `record<Owner>` for a `belongs_to :owner,
    /// class_name: 'User'` declaration — the relation name doesn't
    /// map to a real table.
    ///
    /// Only emitted when the `AssocDecl.options` carries a
    /// `class_name` key; absence means "use the Rails convention".
    ClassName,

    /// `(<model>.<attr>, validation_kind, "<kind>")` — Rails
    /// `validates :attr, <kind>: true` option keys (`presence`,
    /// `uniqueness`, `length`, `format`, `numericality`, `inclusion`,
    /// `exclusion`, `acceptance`, `confirmation`).
    ///
    /// Subject is the validated attribute IRI
    /// (`openproject:WorkPackage.subject`), object is the canonical
    /// Rails validation key. One validation declaration with multiple
    /// kinds emits multiple triples (`validates :email, presence:
    /// true, format: { with: /…/ }` → two `validation_kind` triples).
    ///
    /// Distinct from the existence-of-validation
    /// [`Self::ValidatesConstraint`] triple (subject = model). Both
    /// are emitted so the consumer can choose: graph traversal joins
    /// on `validates_constraint`, schema-quality consumers gate on
    /// `validation_kind` to emit richer `ASSERT` clauses or `UNIQUE`
    /// indices.
    ValidationKind,

    // ───── C++ machine-plane (libclang harvest — ruff_cpp_spo) ─────
    //
    // The 13 net-new predicates for the C++ frontend. Subject conventions
    // mirror the existing surface: class-scoped facts take the class IRI as
    // subject (`inherits_from`, `has_field`, `is_friend_of`,
    // `template_*`, `static_asserts`), method-scoped properties take the
    // method IRI (`is_pure_virtual`, `is_constexpr`, `is_noexcept`,
    // `virtually_overrides`, `defines_operator`, `requires_concept`).
    // Default tier is [`Provenance::CppExtracted`] (declarative C++ surface
    // is machine-readable from the AST); three per-edge overrides encode the
    // metaprogramming residual: [`Self::IsFriendOf`] is Structural (purely
    // declarative), [`Self::UsesMacroExpansion`] and
    // [`Self::TemplateInstantiates`] are Inferred (macro provenance + single-
    // TU instantiation visibility are heuristic).
    /// `(class, inherits_from, base_class)` — class inheritance.
    /// One triple per base.
    ///
    /// **Cross-frontend predicate.** Both C++ class inheritance and
    /// Rails STI (Single Table Inheritance, `class Foo <
    /// ApplicationRecord` with `inheritance_column`) emit this. Wire
    /// shape is identical; per-emission provenance differs:
    ///
    /// - C++ frontend (`ruff_cpp_spo`): default
    ///   [`Provenance::CppExtracted`]. The access specifier
    ///   (public/protected/private) and virtual-inheritance flag are
    ///   carried on [`crate::CppBase`] but not emitted in the triple.
    /// - Rails frontend (`ruff_ruby_spo` / `ruff_spo_triplet::expand`):
    ///   emitted with [`Provenance::OpenProjectExtracted`] from the
    ///   `StiInfo` slot. The `abstract_class` /
    ///   `inheritance_column` metadata are carried on the IR only.
    ///
    /// Distinct from [`Self::IncludesModule`] which is method-body
    /// composition — STI shares a single physical table via a
    /// type-discriminator column.
    InheritsFrom,
    /// `(class, has_field, class.field)` — a data-member declaration. The
    /// resolved type is carried on the IR ([`crate::CppField`]); the C++
    /// member field is also classified `(class.field, rdf:type, Property)`.
    HasField,
    /// `(class, template_specialises, "<template><args>")` — explicit
    /// template specialisation (partial or full).
    TemplateSpecialises,
    /// `(class, template_instantiates, "<template><args>")` — a materialised
    /// instantiation visible in the translation unit. **Default tier is
    /// `Inferred`**: a single-TU view of instantiations is incomplete by
    /// construction.
    TemplateInstantiates,
    /// `(class.method, virtually_overrides, ns:Namespace::Base.method)` —
    /// `override` on a virtual base method. Object is the **fully-qualified**
    /// base-method IRI so it joins the base class's own method node (carried
    /// fully-qualified on `CppMethod::overrides`; codex P2, PR #8).
    VirtuallyOverrides,
    /// `(class, is_friend_of, friend)` — `friend class` / `friend fn`
    /// declaration. **Default tier is `Structural`**: purely declarative,
    /// no inference involved.
    IsFriendOf,
    /// `(class.method, defines_operator, "<operator-kind>")` — operator
    /// overload. The operator kind (e.g. `operator==`) is the object.
    DefinesOperator,
    /// `(class, uses_macro_expansion, "<identifier><=<macro>")` — an
    /// identifier that originates from a preprocessor macro expansion.
    /// **Default tier is `Inferred`**: macro provenance loses surface info.
    UsesMacroExpansion,
    /// `(class.method, is_pure_virtual, "true")` — a `= 0` declaration.
    IsPureVirtual,
    /// `(class.method, is_constexpr, "constexpr"|"consteval")` — compile-time
    /// computable marker. The `consteval` (immediate-function) variant rides
    /// the object slot, keeping the vocab bounded (same discipline as
    /// [`Self::HasCallback`] encoding the phase in the object).
    IsConstexpr,
    /// `(class.method, is_noexcept, "true")` — an exception specification
    /// marking the method `noexcept`.
    IsNoexcept,
    /// `(class.method, requires_concept, "<requires-clause>")` — a C++20
    /// `requires` clause constraining the method.
    RequiresConcept,
    /// `(class, static_asserts, "<condition>")` — a `static_assert` in
    /// class scope.
    StaticAsserts,
    /// `(class.method, returns_type, "<type>")` — the method's return type,
    /// verbatim. Not emitted for `void` / ctors / dtors. The return half of the
    /// AST-DLL signature shape (with [`Self::HasParamType`]).
    ReturnsType,
    /// `(class.method, has_param_type, "<index>:<type>")` — one per parameter,
    /// in signature order. The 0-based position rides the object (leading digits
    /// before the first `:`) so a triple SET preserves order + arity. The
    /// parameter half of the AST-DLL signature shape.
    HasParamType,
    /// `(class.method, is_const, "true")` — a const-qualified member function
    /// (read accessor). The ORM-downcast shape.
    IsConst,
    /// `(class.method, is_static, "true")` — a static member function
    /// (class-level, no implicit `this`).
    IsStatic,
    /// `(class.method, has_visibility, "public"|"protected"|"private")` — the
    /// member access specifier. The OO API-surface + intrusiveness signal:
    /// public methods are the adapter surface; private/protected are likely
    /// internal (a routing hint for the codegen's hand-port deny-list).
    HasVisibility,
}

impl Predicate {
    /// The on-the-wire predicate string. Stable; never reformat.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            // Core 7
            Self::RdfType => "rdf:type",
            Self::HasFunction => "has_function",
            Self::EmittedBy => "emitted_by",
            Self::DependsOn => "depends_on",
            Self::ReadsField => "reads_field",
            Self::Raises => "raises",
            Self::TraversesRelation => "traverses_relation",
            // OpenProject AR-shape 27
            Self::DeclaresAssociation => "declares_association",
            Self::ValidatesConstraint => "validates_constraint",
            Self::NormalizesAttribute => "normalizes_attribute",
            Self::HasCallback => "has_callback",
            Self::IncludesModule => "includes_module",
            Self::ExtendsModule => "extends_module",
            Self::PrependsModule => "prepends_module",
            Self::ConcernClassMethods => "concern_class_methods",
            Self::ConcernIncludedBlock => "concern_included_block",
            Self::HasAttribute => "has_attribute",
            Self::AliasesAttribute => "aliases_attribute",
            Self::AliasesMethod => "aliases_method",
            Self::ColumnOverride => "column_override",
            Self::DelegatesTo => "delegates_to",
            Self::HasScope => "has_scope",
            Self::HasDefaultScope => "has_default_scope",
            Self::ActsAs => "acts_as",
            Self::RegistersJournalFormatter => "registers_journal_formatter",
            Self::RegistersJournalFormattedFields => "registers_journal_formatted_fields",
            Self::HasDslCall => "has_dsl_call",
            Self::MountsUploader => "mounts_uploader",
            Self::HasPaperTrail => "has_paper_trail",
            Self::HasClosureTree => "has_closure_tree",
            Self::CounterCultures => "counter_cultures",
            Self::AutoStrips => "auto_strips",
            Self::DefinesMethod => "defines_method",
            Self::UsesRefinement => "uses_refinement",
            Self::FieldType => "field_type",
            Self::AssociationKind => "association_kind",
            Self::ClassName => "class_name",
            Self::ValidationKind => "validation_kind",
            // C++ machine-plane 13
            Self::InheritsFrom => "inherits_from",
            Self::HasField => "has_field",
            Self::TemplateSpecialises => "template_specialises",
            Self::TemplateInstantiates => "template_instantiates",
            Self::VirtuallyOverrides => "virtually_overrides",
            Self::IsFriendOf => "is_friend_of",
            Self::DefinesOperator => "defines_operator",
            Self::UsesMacroExpansion => "uses_macro_expansion",
            Self::IsPureVirtual => "is_pure_virtual",
            Self::IsConstexpr => "is_constexpr",
            Self::IsNoexcept => "is_noexcept",
            Self::RequiresConcept => "requires_concept",
            Self::StaticAsserts => "static_asserts",
            Self::ReturnsType => "returns_type",
            Self::HasParamType => "has_param_type",
            Self::IsConst => "is_const",
            Self::IsStatic => "is_static",
            Self::HasVisibility => "has_visibility",
        }
    }

    /// Parse a predicate string back to the enum. `None` for unknown
    /// predicates — callers should treat that as a hard schema error
    /// (the vocabulary is closed).
    #[must_use]
    #[expect(
        clippy::should_implement_trait,
        reason = "closed-vocab parser returns Option (unknown = hard schema error); std::str::FromStr's Result API would force a bogus Err type"
    )]
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            // Core 7
            "rdf:type" => Self::RdfType,
            "has_function" => Self::HasFunction,
            "emitted_by" => Self::EmittedBy,
            "depends_on" => Self::DependsOn,
            "reads_field" => Self::ReadsField,
            "raises" => Self::Raises,
            "traverses_relation" => Self::TraversesRelation,
            // OpenProject AR-shape 27
            "declares_association" => Self::DeclaresAssociation,
            "validates_constraint" => Self::ValidatesConstraint,
            "normalizes_attribute" => Self::NormalizesAttribute,
            "has_callback" => Self::HasCallback,
            "includes_module" => Self::IncludesModule,
            "extends_module" => Self::ExtendsModule,
            "prepends_module" => Self::PrependsModule,
            "concern_class_methods" => Self::ConcernClassMethods,
            "concern_included_block" => Self::ConcernIncludedBlock,
            "has_attribute" => Self::HasAttribute,
            "aliases_attribute" => Self::AliasesAttribute,
            "aliases_method" => Self::AliasesMethod,
            "column_override" => Self::ColumnOverride,
            "delegates_to" => Self::DelegatesTo,
            "has_scope" => Self::HasScope,
            "has_default_scope" => Self::HasDefaultScope,
            "acts_as" => Self::ActsAs,
            "registers_journal_formatter" => Self::RegistersJournalFormatter,
            "registers_journal_formatted_fields" => Self::RegistersJournalFormattedFields,
            "has_dsl_call" => Self::HasDslCall,
            "mounts_uploader" => Self::MountsUploader,
            "has_paper_trail" => Self::HasPaperTrail,
            "has_closure_tree" => Self::HasClosureTree,
            "counter_cultures" => Self::CounterCultures,
            "auto_strips" => Self::AutoStrips,
            "defines_method" => Self::DefinesMethod,
            "uses_refinement" => Self::UsesRefinement,
            "field_type" => Self::FieldType,
            "association_kind" => Self::AssociationKind,
            "class_name" => Self::ClassName,
            "validation_kind" => Self::ValidationKind,
            // C++ machine-plane 13
            "inherits_from" => Self::InheritsFrom,
            "has_field" => Self::HasField,
            "template_specialises" => Self::TemplateSpecialises,
            "template_instantiates" => Self::TemplateInstantiates,
            "virtually_overrides" => Self::VirtuallyOverrides,
            "is_friend_of" => Self::IsFriendOf,
            "defines_operator" => Self::DefinesOperator,
            "uses_macro_expansion" => Self::UsesMacroExpansion,
            "is_pure_virtual" => Self::IsPureVirtual,
            "is_constexpr" => Self::IsConstexpr,
            "is_noexcept" => Self::IsNoexcept,
            "requires_concept" => Self::RequiresConcept,
            "static_asserts" => Self::StaticAsserts,
            "returns_type" => Self::ReturnsType,
            "has_param_type" => Self::HasParamType,
            "is_const" => Self::IsConst,
            "is_static" => Self::IsStatic,
            "has_visibility" => Self::HasVisibility,
            _ => return None,
        })
    }

    /// Every predicate in canonical declaration order. Used by the
    /// closed-vocab round-trip test and by any consumer that needs to
    /// enumerate the whole surface (e.g. the ndjson validator).
    ///
    /// **Length invariant:** `ALL.len() == 53` (7 core + 29 AR-shape +
    /// 17 C++ machine-plane). A new variant added to [`Predicate`] **must**
    /// be appended here in the same order, or the closed-vocab round-trip
    /// test fails.
    pub const ALL: &'static [Predicate] = &[
        // Core 7
        Self::RdfType,
        Self::HasFunction,
        Self::EmittedBy,
        Self::DependsOn,
        Self::ReadsField,
        Self::Raises,
        Self::TraversesRelation,
        // OpenProject AR-shape 27
        Self::DeclaresAssociation,
        Self::ValidatesConstraint,
        Self::NormalizesAttribute,
        Self::HasCallback,
        Self::IncludesModule,
        Self::ExtendsModule,
        Self::PrependsModule,
        Self::ConcernClassMethods,
        Self::ConcernIncludedBlock,
        Self::HasAttribute,
        Self::AliasesAttribute,
        Self::AliasesMethod,
        Self::ColumnOverride,
        Self::DelegatesTo,
        Self::HasScope,
        Self::HasDefaultScope,
        Self::ActsAs,
        Self::RegistersJournalFormatter,
        Self::RegistersJournalFormattedFields,
        Self::HasDslCall,
        Self::MountsUploader,
        Self::HasPaperTrail,
        Self::HasClosureTree,
        Self::CounterCultures,
        Self::AutoStrips,
        Self::DefinesMethod,
        Self::UsesRefinement,
        Self::FieldType,
        Self::AssociationKind,
        Self::ClassName,
        Self::ValidationKind,
        // C++ machine-plane 13
        Self::InheritsFrom,
        Self::HasField,
        Self::TemplateSpecialises,
        Self::TemplateInstantiates,
        Self::VirtuallyOverrides,
        Self::IsFriendOf,
        Self::DefinesOperator,
        Self::UsesMacroExpansion,
        Self::IsPureVirtual,
        Self::IsConstexpr,
        Self::IsNoexcept,
        Self::RequiresConcept,
        Self::StaticAsserts,
        Self::ReturnsType,
        Self::HasParamType,
        Self::IsConst,
        Self::IsStatic,
        Self::HasVisibility,
    ];

    /// The default provenance tier for this predicate, per the Odoo
    /// extraction calibration + `OpenProject` hand-tune:
    ///
    /// - structural (`rdf:type`, `has_function`, concern markers) →
    ///   [`Provenance::Structural`]
    /// - declared / body-authoritative (`emitted_by`, `depends_on`,
    ///   `raises`) → [`Provenance::Authoritative`]
    /// - body-inferred (`reads_field`, `traverses_relation`,
    ///   `defines_method`) → [`Provenance::Inferred`]
    /// - `OpenProject` Rails class-body DSL (the 22 remaining new
    ///   predicates) → [`Provenance::OpenProjectExtracted`]
    ///
    /// Frontends may override per-edge (e.g. a Rails frontend that proves a
    /// read statically can promote `reads_field` to Authoritative; one
    /// `define_method` whose name is a static literal can be promoted to
    /// `OpenProjectExtracted`), but the default keeps cross-language
    /// graphs comparable.
    #[must_use]
    pub const fn default_provenance(self) -> Provenance {
        match self {
            // Structural-by-construction
            Self::RdfType
            | Self::HasFunction
            | Self::ConcernClassMethods
            | Self::ConcernIncludedBlock
            | Self::IsFriendOf => Provenance::Structural,
            // Body-authoritative (Odoo + Rails declared)
            Self::EmittedBy | Self::DependsOn | Self::Raises => Provenance::Authoritative,
            // Body-inferred (heuristic by definition) — including the two
            // C++ metaprogramming-residual predicates (macro provenance,
            // single-TU template instantiation visibility).
            Self::ReadsField
            | Self::TraversesRelation
            | Self::DefinesMethod
            | Self::UsesMacroExpansion
            | Self::TemplateInstantiates => Provenance::Inferred,
            // C++ machine-plane declarative surface (the 10 remaining of 13)
            Self::InheritsFrom
            | Self::HasField
            | Self::TemplateSpecialises
            | Self::VirtuallyOverrides
            | Self::DefinesOperator
            | Self::IsPureVirtual
            | Self::IsConstexpr
            | Self::IsNoexcept
            | Self::RequiresConcept
            | Self::StaticAsserts
            | Self::ReturnsType
            | Self::HasParamType
            | Self::IsConst
            | Self::IsStatic
            | Self::HasVisibility => Provenance::CppExtracted,
            // OpenProject AR-shape (everything else from the 27)
            Self::DeclaresAssociation
            | Self::ValidatesConstraint
            | Self::NormalizesAttribute
            | Self::HasCallback
            | Self::IncludesModule
            | Self::ExtendsModule
            | Self::PrependsModule
            | Self::HasAttribute
            | Self::AliasesAttribute
            | Self::AliasesMethod
            | Self::ColumnOverride
            | Self::DelegatesTo
            | Self::HasScope
            | Self::HasDefaultScope
            | Self::ActsAs
            | Self::RegistersJournalFormatter
            | Self::RegistersJournalFormattedFields
            | Self::HasDslCall
            | Self::MountsUploader
            | Self::HasPaperTrail
            | Self::HasClosureTree
            | Self::CounterCultures
            | Self::AutoStrips
            | Self::UsesRefinement
            | Self::FieldType
            | Self::AssociationKind
            | Self::ClassName
            | Self::ValidationKind => Provenance::OpenProjectExtracted,
        }
    }
}

/// The `rdf:type` object vocabulary — the Foundry-shape entity classes.
///
/// Object Type = an entity/model (Odoo model, Rails `ActiveRecord` class).
/// Property = a field/attribute. Function = a method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityKind {
    /// A model / entity (e.g. `account.move`, `WorkPackage`).
    ObjectType,
    /// A field / attribute (e.g. `amount_total`, `subject`).
    Property,
    /// A method / function (e.g. `_compute_amount`, `compute_total_hours`).
    Function,
}

impl EntityKind {
    /// The OGIT-namespaced IRI used as the `rdf:type` object.
    ///
    /// Uses the canonical OGIT vocabulary prefix `ogit:` — NOT a
    /// project-local namespace. The OGIT base is
    /// `http://www.purl.org/ogit/`; consumers resolve `ogit:` against it.
    #[must_use]
    pub const fn iri(self) -> &'static str {
        match self {
            Self::ObjectType => "ogit:ObjectType",
            Self::Property => "ogit:Property",
            Self::Function => "ogit:Function",
        }
    }
}

/// Provenance tier → NARS `(frequency, confidence)`.
///
/// The first three tiers are the calibration from the Odoo harvest:
/// structural facts are certain; decorator/declared/body-authoritative
/// facts are strong; purely body-inferred facts are weaker so the
/// `TruthGate` can filter them out under a strict expectation threshold.
///
/// [`Self::OpenProjectExtracted`] is one notch below `Authoritative`,
/// hand-tuned per `I-NOISE-FLOOR-JIRAK`: Ruby `ActiveRecord` declarative
/// facts (`belongs_to :project`) are as truth-functionally certain as
/// Odoo `@api.depends`, so the **frequency** matches at `0.95`. The
/// **confidence** drops `0.90 → 0.88` (two NARS revision-counts) to
/// encode the Ruby metaprogramming surface delta — `class_methods do`,
/// `included do`, `define_method`, `class << self`, dynamic constants —
/// a small fraction of declarations are unresolvable at static-AST time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provenance {
    /// Structural facts that are true by construction — `(1.0, 1.0)`.
    /// e.g. "this name is a model", "this model owns this method".
    Structural,
    /// Declared or directly-observed-in-body facts — `(0.95, 0.90)`.
    /// e.g. an `@api.depends(...)` argument, a `raise` statement, the
    /// field a compute method assigns to.
    Authoritative,
    /// Heuristically inferred from body shape — `(0.85, 0.75)`.
    /// e.g. a field name that appears as an attribute read, a relation
    /// walked in a `for r in self.<rel>` loop.
    Inferred,
    /// `OpenProject` Rails class-body DSL — `(0.95, 0.88)`. Same frequency
    /// as `Authoritative` (the declaration IS the fact), confidence one
    /// notch lower to encode the Ruby metaprogramming residual.
    OpenProjectExtracted,
    /// C++ machine-plane libclang harvest — `(0.95, 0.82)`. Same frequency
    /// as `Authoritative` (the C++ declaration IS the fact, resolved by the
    /// AST), confidence below `OpenProjectExtracted` to encode the deeper
    /// metaprogramming surface — templates (two-phase lookup, partial
    /// specialisation), the preprocessor, and ADL each add a layer a static
    /// AST view does not fully resolve. Initial target per the
    /// `ruff_cpp_spo` headstone; recalibrate against a Tesseract corpus
    /// baseline once `CPP-SCHEMA-FIT` runs.
    CppExtracted,
}

impl Provenance {
    /// The NARS `(frequency, confidence)` pair for this tier.
    #[must_use]
    pub const fn truth(self) -> (f32, f32) {
        match self {
            Self::Structural => (1.0, 1.0),
            Self::Authoritative => (0.95, 0.90),
            Self::Inferred => (0.85, 0.75),
            Self::OpenProjectExtracted => (0.95, 0.88),
            Self::CppExtracted => (0.95, 0.82),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicate_string_round_trips_for_every_canonical() {
        // The closed-vocab guarantee: every variant in ALL must round-trip
        // through as_str → from_str. Adding a Predicate variant without
        // updating ALL or the match arms breaks this test loudly.
        for p in Predicate::ALL {
            assert_eq!(
                Predicate::from_str(p.as_str()),
                Some(*p),
                "predicate `{}` failed to round-trip",
                p.as_str()
            );
        }
        assert_eq!(Predicate::from_str("not_a_predicate"), None);
    }

    #[test]
    fn predicate_count_locked_at_56() {
        // The exact count is part of the schema contract: 7 core (Odoo
        // Python) + 31 OpenProject AR-shape (PR #15 added
        // `association_kind` so the FK-direction bug is fixable
        // downstream; #18 added `class_name` so `belongs_to :owner,
        // class_name: 'User'` can emit `record<User>` instead of
        // phantom `record<Owner>`; this PR adds `validation_kind` so
        // the downstream consumer can lift Rails validation kinds
        // (`presence` → ASSERT, `uniqueness` → UNIQUE INDEX, …) into
        // richer SurrealQL constraints) + 18 C++ machine-plane =
        // 56. Council review of any new variant means this number
        // changes — the test must change with the source.
        assert_eq!(Predicate::ALL.len(), 56);
    }

    #[test]
    fn provenance_truth_tiers_match_calibration() {
        assert_eq!(Provenance::Structural.truth(), (1.0, 1.0));
        assert_eq!(Provenance::Authoritative.truth(), (0.95, 0.90));
        assert_eq!(Provenance::Inferred.truth(), (0.85, 0.75));
        assert_eq!(Provenance::OpenProjectExtracted.truth(), (0.95, 0.88));
        assert_eq!(Provenance::CppExtracted.truth(), (0.95, 0.82));
    }

    #[test]
    #[allow(clippy::float_cmp)] // exact comparison is the assertion's whole point
    fn cpp_extracted_is_below_open_project_extracted() {
        // Same frequency as Authoritative / OpenProjectExtracted (the C++
        // declaration IS the fact), confidence strictly below the Ruby tier
        // to encode the templates + preprocessor + ADL surface delta.
        let (auth_f, _) = Provenance::Authoritative.truth();
        let (op_f, op_c) = Provenance::OpenProjectExtracted.truth();
        let (cpp_f, cpp_c) = Provenance::CppExtracted.truth();
        assert_eq!(cpp_f, auth_f, "frequency matches Authoritative");
        assert_eq!(cpp_f, op_f, "frequency matches OpenProjectExtracted");
        assert!(
            cpp_c < op_c,
            "C++ confidence must be strictly below the Ruby tier"
        );
    }

    #[test]
    #[allow(clippy::float_cmp)] // exact comparison is the assertion's whole point
    fn open_project_extracted_is_one_notch_below_authoritative() {
        // Hand-tune assertion (per I-NOISE-FLOOR-JIRAK): same frequency
        // as Authoritative, confidence two NARS revision-counts lower.
        let (auth_f, auth_c) = Provenance::Authoritative.truth();
        let (op_f, op_c) = Provenance::OpenProjectExtracted.truth();
        assert_eq!(auth_f, op_f, "frequency must match Authoritative");
        assert!(
            op_c < auth_c,
            "confidence must be strictly below Authoritative"
        );
        assert!(
            (auth_c - op_c - 0.02).abs() < 1e-6,
            "exactly two revision-counts below"
        );
    }

    #[test]
    fn default_provenance_matches_predicate_class() {
        // Structural-by-construction
        assert_eq!(
            Predicate::RdfType.default_provenance(),
            Provenance::Structural
        );
        assert_eq!(
            Predicate::ConcernClassMethods.default_provenance(),
            Provenance::Structural
        );
        assert_eq!(
            Predicate::ConcernIncludedBlock.default_provenance(),
            Provenance::Structural
        );
        // Authoritative (Odoo)
        assert_eq!(
            Predicate::DependsOn.default_provenance(),
            Provenance::Authoritative
        );
        // Inferred — including DefinesMethod (per-edge override possible
        // but the default tier is heuristic)
        assert_eq!(
            Predicate::ReadsField.default_provenance(),
            Provenance::Inferred
        );
        assert_eq!(
            Predicate::DefinesMethod.default_provenance(),
            Provenance::Inferred
        );
        // OpenProjectExtracted (the bulk of the new 27)
        assert_eq!(
            Predicate::DeclaresAssociation.default_provenance(),
            Provenance::OpenProjectExtracted
        );
        assert_eq!(
            Predicate::ActsAs.default_provenance(),
            Provenance::OpenProjectExtracted
        );
        assert_eq!(
            Predicate::RegistersJournalFormatter.default_provenance(),
            Provenance::OpenProjectExtracted
        );
        // C++ machine-plane: CppExtracted default + 3 per-edge overrides.
        assert_eq!(
            Predicate::InheritsFrom.default_provenance(),
            Provenance::CppExtracted
        );
        assert_eq!(
            Predicate::IsPureVirtual.default_provenance(),
            Provenance::CppExtracted
        );
        // is_friend_of → Structural (purely declarative).
        assert_eq!(
            Predicate::IsFriendOf.default_provenance(),
            Provenance::Structural
        );
        // macro expansion + single-TU instantiation → Inferred.
        assert_eq!(
            Predicate::UsesMacroExpansion.default_provenance(),
            Provenance::Inferred
        );
        assert_eq!(
            Predicate::TemplateInstantiates.default_provenance(),
            Provenance::Inferred
        );
    }

    #[test]
    fn every_predicate_has_a_default_provenance() {
        // Exhaustiveness lock: const match in default_provenance must
        // cover every variant in ALL.
        for p in Predicate::ALL {
            let _ = p.default_provenance();
        }
    }

    #[test]
    fn triple_new_carries_provenance_truth() {
        let t = Triple::new(
            "odoo:m.f",
            Predicate::EmittedBy,
            "odoo:m._fn",
            Provenance::Authoritative,
        );
        assert_eq!(t.p, "emitted_by");
        assert_eq!((t.f, t.c), (0.95, 0.90));
        assert_eq!(t.key(), ("odoo:m.f", "emitted_by", "odoo:m._fn"));
    }

    #[test]
    fn triple_new_carries_op_extracted_truth() {
        let t = Triple::new(
            "openproject:WorkPackage",
            Predicate::DeclaresAssociation,
            "openproject:WorkPackage.project",
            Provenance::OpenProjectExtracted,
        );
        assert_eq!(t.p, "declares_association");
        assert_eq!((t.f, t.c), (0.95, 0.88));
    }

    #[test]
    fn entity_kind_uses_canonical_ogit_prefix() {
        assert_eq!(EntityKind::ObjectType.iri(), "ogit:ObjectType");
        assert_eq!(EntityKind::Property.iri(), "ogit:Property");
        assert_eq!(EntityKind::Function.iri(), "ogit:Function");
    }
}
