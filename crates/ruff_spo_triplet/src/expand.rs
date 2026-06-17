//! The expander — the single deterministic projection from [`ModelGraph`]
//! IR to a sorted, de-duplicated `Vec<Triple>`.
//!
//! This is the whole point of the crate: one function, called by every
//! language frontend, so the SPO graph is identical regardless of source
//! language. Determinism is structural — output is sorted by `(s, p, o)`
//! and de-duplicated, so two runs over the same IR are byte-identical.

use std::collections::BTreeSet;

use crate::ir::{
    ActsAs, AssocDecl, AssocKind, AttrDecl, AttrKind, Callback, ConcernKind, ConcernRef,
    ConstexprKind, CppBase, CppField, CppFriend, CppMacroUse, CppMethod, CppStaticAssert,
    CppTemplate, CppTemplateKind, Delegation, DslCall, DynMethod, GemDsl, GemKind, Model,
    ModelGraph, ScopeDecl, ScopeKind, StiInfo, UsingRef, Validation, ValidationKind,
};
use crate::triple::{EntityKind, Predicate, Provenance, Triple};

/// Expand a [`ModelGraph`] into canonical SPO triples.
///
/// # Emission rules — core 7 (per model)
///
/// 1. `(ns:model, rdf:type, ogit:ObjectType)` — Structural.
/// 2. For each field: `(ns:model.field, rdf:type, ogit:Property)` — Structural.
/// 3. For each function:
///    - `(ns:model.fn, rdf:type, ogit:Function)` — Structural.
///    - `(ns:model, has_function, ns:model.fn)` — Structural.
/// 4. For each field with `emitted_by`:
///    `(ns:model.field, emitted_by, ns:model.fn)` — Authoritative.
/// 5. For each field dependency:
///    `(ns:model.field, depends_on, ns:model.<dep>)` — Authoritative.
/// 6. For each function read:
///    `(ns:model.fn, reads_field, ns:model.field)` — Inferred.
/// 7. For each function raise:
///    `(ns:model.fn, raises, exc:<Type>)` — Authoritative.
/// 8. For each function traversal:
///    `(ns:model.fn, traverses_relation, ns:model.<rel>)` — Inferred.
///
/// `depends_on` and `traverses_relation` objects are emitted verbatim as
/// dotted paths under the model IRI — the [`crate`]-downstream link-chain
/// splitter (`lance_graph::graph::spo::link_chain`) decomposes them into
/// per-hop link triples; this crate does NOT pre-split them, keeping the
/// emitter source-faithful.
///
/// # Emission rules — `OpenProject` AR-shape (per model)
///
/// 9.  For each `associations[i]`:
///     `(ns:model, declares_association, ns:model.<rel>)` — `OpenProjectExtracted`.
/// 10. For each `validations[i]`:
///     - kind ≠ Normalizes → `(ns:model, validates_constraint, <target>)` — `OpenProjectExtracted`.
///     - kind == Normalizes → `(ns:model, normalizes_attribute, <target>)` — `OpenProjectExtracted`.
/// 11. For each `callbacks[i]`:
///     `(ns:model, has_callback, "<phase>:<target>")` — `OpenProjectExtracted`.
/// 12. For each `concerns[i]` (by kind):
///     - Include → `includes_module`, Extend → `extends_module`,
///       Prepend → `prepends_module` — `OpenProjectExtracted`.
///     - `ClassMethodsBlock` → `concern_class_methods`, `IncludedBlock` →
///       `concern_included_block` — Structural (block declarations are
///       structural-by-construction).
/// 13. For each `attributes[i]` (by kind):
///     - Attribute / `AttrAccessor` / `AttrReader` / `AttrReadonly` /
///       `StoreAttribute` / `StoreAccessor` / Serialize / Enum /
///       `DefineAttributeMethod` → `(ns:model, has_attribute, <name>)` —
///       `OpenProjectExtracted`.
///     - `AliasAttribute` → `aliases_attribute`.
///     - `AliasMethod` / Alias → `aliases_method`.
///     - `UndefMethod` → `column_override` (marks a column as undefined).
/// 14. For each `delegations[i]`, expand one triple per delegated method:
///     `(ns:model, delegates_to, "<method>=>via:<to>")` — `OpenProjectExtracted`.
/// 15. For each `scopes[i]` (by kind):
///     - Scope → `has_scope`, `DefaultScope` → `has_default_scope`,
///       Scopes (OP plural) → `has_scope` (one triple per name).
/// 16. For each `acts_as[i]`:
///     `(ns:model, acts_as, "<variant>[:<options>]")` — `OpenProjectExtracted`.
/// 17. For each `dsl_calls[i]`, route by name:
///     - `register_journal_formatter` → `registers_journal_formatter`,
///     - `register_journal_formatted_fields` → `registers_journal_formatted_fields`,
///     - everything else → `has_dsl_call` (catch-all).
/// 18. For each `gem_dsl[i]` (by gem):
///     - `MountUploader` → `mounts_uploader`, `HasPaperTrail` →
///       `has_paper_trail`, `HasClosureTree` → `has_closure_tree`,
///       `CounterCulture` → `counter_cultures`, `AutoStripAttributes` →
///       `auto_strips`.
/// 19. For each `dynamic_methods[i]`:
///     `(ns:model, defines_method, "<name_expr>=<body_ref>")` — Inferred
///     (per-edge, since dynamism is the whole reason for the variant).
/// 20. For each `refinements[i]`:
///     `(ns:model, uses_refinement, <refinement_module>)` — `OpenProjectExtracted`.
/// 21. If `sti.is_some()`: emit `(ns:model, includes_module, <parent>)`
///     for `sti.inherits_from` — `OpenProjectExtracted`. The
///     `abstract_class` / `inheritance_column` fields are metadata only.
///
/// # Determinism
///
/// The returned Vec is sorted by `(s, p, o)` and de-duplicated. Truth
/// values do not participate in ordering or de-duplication — if the same
/// `(s, p, o)` is produced twice with different provenance, the
/// first-in-sort-order (which, after sort, is deterministic but provenance-
/// arbitrary) wins. Frontends should not emit conflicting provenance for
/// one identity; [`crate::ndjson`] round-trips assume a clean IR.
#[must_use]
pub fn expand(graph: &ModelGraph) -> Vec<Triple> {
    let mut exp = Expander::new();
    for model in &graph.models {
        exp.model(&graph.namespace, model);
    }
    exp.finish()
}

struct Expander {
    triples: Vec<Triple>,
    set: BTreeSet<(String, String, String)>,
}

impl Expander {
    fn new() -> Self {
        Self {
            triples: Vec::new(),
            set: BTreeSet::new(),
        }
    }

    fn push(&mut self, s: String, p: Predicate, o: String, prov: Provenance) {
        let key = (s.clone(), p.as_str().to_string(), o.clone());
        if self.set.insert(key) {
            self.triples.push(Triple::new(s, p, o, prov));
        }
    }

    fn finish(mut self) -> Vec<Triple> {
        self.triples.sort_by(|a, b| a.key().cmp(&b.key()));
        self.triples
    }

    fn model(&mut self, ns: &str, model: &Model) {
        let model_iri = format!("{ns}:{}", model.name);

        // 1. model rdf:type ObjectType
        self.push(
            model_iri.clone(),
            Predicate::RdfType,
            EntityKind::ObjectType.iri().to_string(),
            Provenance::Structural,
        );

        // 2 + 4 + 5. fields
        for field in &model.fields {
            let field_iri = format!("{model_iri}.{}", field.name);
            self.push(
                field_iri.clone(),
                Predicate::RdfType,
                EntityKind::Property.iri().to_string(),
                Provenance::Structural,
            );
            if let Some(fn_name) = &field.emitted_by {
                self.push(
                    field_iri.clone(),
                    Predicate::EmittedBy,
                    format!("{model_iri}.{fn_name}"),
                    Provenance::Authoritative,
                );
            }
            for dep in &field.depends_on {
                self.push(
                    field_iri.clone(),
                    Predicate::DependsOn,
                    format!("{model_iri}.{dep}"),
                    Provenance::Authoritative,
                );
            }
        }

        // 3 + 6 + 7 + 8. functions
        for func in &model.functions {
            let fn_iri = format!("{model_iri}.{}", func.name);
            self.push(
                fn_iri.clone(),
                Predicate::RdfType,
                EntityKind::Function.iri().to_string(),
                Provenance::Structural,
            );
            self.push(
                model_iri.clone(),
                Predicate::HasFunction,
                fn_iri.clone(),
                Provenance::Structural,
            );
            for read in &func.reads {
                self.push(
                    fn_iri.clone(),
                    Predicate::ReadsField,
                    format!("{model_iri}.{read}"),
                    Provenance::Inferred,
                );
            }
            for exc in &func.raises {
                self.push(
                    fn_iri.clone(),
                    Predicate::Raises,
                    format!("exc:{exc}"),
                    Provenance::Authoritative,
                );
            }
            for rel in &func.traverses {
                self.push(
                    fn_iri.clone(),
                    Predicate::TraversesRelation,
                    format!("{model_iri}.{rel}"),
                    Provenance::Inferred,
                );
            }
        }

        // ───── OpenProject AR-shape ─────

        for assoc in &model.associations {
            self.association(&model_iri, assoc);
        }
        for v in &model.validations {
            self.validation(&model_iri, v);
        }
        for cb in &model.callbacks {
            self.callback(&model_iri, cb);
        }
        for cr in &model.concerns {
            self.concern(&model_iri, cr);
        }
        for a in &model.attributes {
            self.attribute(&model_iri, a);
        }
        for d in &model.delegations {
            self.delegation(&model_iri, d);
        }
        for s in &model.scopes {
            self.scope(&model_iri, s);
        }
        for aa in &model.acts_as {
            self.acts_as(&model_iri, aa);
        }
        for dc in &model.dsl_calls {
            self.dsl_call(&model_iri, dc);
        }
        for g in &model.gem_dsl {
            self.gem_dsl(&model_iri, g);
        }
        for dm in &model.dynamic_methods {
            self.dynamic_method(&model_iri, dm);
        }
        for r in &model.refinements {
            self.refinement(&model_iri, r);
        }
        if let Some(sti) = &model.sti {
            self.sti(&model_iri, sti);
        }

        // ───── C++ machine-plane ─────

        for base in &model.bases {
            self.cpp_base(ns, &model_iri, base);
        }
        for field in &model.member_fields {
            self.cpp_field(&model_iri, field);
        }
        for method in &model.methods {
            self.cpp_method(ns, &model_iri, method);
        }
        for tpl in &model.templates {
            self.cpp_template(&model_iri, tpl);
        }
        for fr in &model.friends {
            self.cpp_friend(&model_iri, fr);
        }
        for mu in &model.macro_uses {
            self.cpp_macro_use(&model_iri, mu);
        }
        for sa in &model.static_asserts {
            self.cpp_static_assert(&model_iri, sa);
        }
    }

    fn association(&mut self, model_iri: &str, a: &AssocDecl) {
        // The existence-of-relation fact, kind-agnostic.
        let rel_iri = format!("{model_iri}.{}", a.name);
        self.push(
            model_iri.to_string(),
            Predicate::DeclaresAssociation,
            rel_iri.clone(),
            Provenance::OpenProjectExtracted,
        );
        // Kind sibling — only the consumer that cares about FK direction
        // reads this; other consumers can ignore it.
        //
        // Why split into two triples (instead of encoding kind in the
        // `declares_association` object): the existence fact is queried
        // by many consumers (lance-graph SPO store, graph-traversal,
        // ndjson roundtrip tests); the kind is only needed by schema
        // codegen. Two predicates keep the existence query cheap.
        let kind_str = match a.kind {
            AssocKind::BelongsTo => "belongs_to",
            AssocKind::HasMany => "has_many",
            AssocKind::HasOne => "has_one",
            AssocKind::HasAndBelongsToMany => "has_and_belongs_to_many",
            AssocKind::AcceptsNestedAttributesFor => "accepts_nested_attributes_for",
        };
        self.push(
            rel_iri,
            Predicate::AssociationKind,
            kind_str.to_string(),
            Provenance::OpenProjectExtracted,
        );
        // Options remain on the IR for downstream consumers that need
        // them (e.g. an OpenProject-specific catalog mapper) but are
        // not surfaced as triples here.
        let _ = a.options;
    }

    fn validation(&mut self, model_iri: &str, v: &Validation) {
        let pred = match v.kind {
            ValidationKind::Normalizes => Predicate::NormalizesAttribute,
            ValidationKind::Validates
            | ValidationKind::Validate
            | ValidationKind::ValidatesAssociated
            | ValidationKind::ValidatesEach => Predicate::ValidatesConstraint,
        };
        self.push(
            model_iri.to_string(),
            pred,
            v.target.clone(),
            Provenance::OpenProjectExtracted,
        );
    }

    fn callback(&mut self, model_iri: &str, cb: &Callback) {
        self.push(
            model_iri.to_string(),
            Predicate::HasCallback,
            format!("{}:{}", cb.phase, cb.target),
            Provenance::OpenProjectExtracted,
        );
    }

    fn concern(&mut self, model_iri: &str, cr: &ConcernRef) {
        let (pred, prov) = match cr.kind {
            ConcernKind::Include => (Predicate::IncludesModule, Provenance::OpenProjectExtracted),
            ConcernKind::Extend => (Predicate::ExtendsModule, Provenance::OpenProjectExtracted),
            ConcernKind::Prepend => (Predicate::PrependsModule, Provenance::OpenProjectExtracted),
            ConcernKind::ClassMethodsBlock => {
                (Predicate::ConcernClassMethods, Provenance::Structural)
            }
            ConcernKind::IncludedBlock => (Predicate::ConcernIncludedBlock, Provenance::Structural),
        };
        let o = match cr.kind {
            ConcernKind::ClassMethodsBlock | ConcernKind::IncludedBlock => {
                cr.body_ref.clone().unwrap_or_else(|| "<block>".to_string())
            }
            _ => cr.module.clone(),
        };
        self.push(model_iri.to_string(), pred, o, prov);
    }

    fn attribute(&mut self, model_iri: &str, a: &AttrDecl) {
        let pred = match a.kind {
            AttrKind::AliasAttribute => Predicate::AliasesAttribute,
            AttrKind::AliasMethod | AttrKind::Alias => Predicate::AliasesMethod,
            AttrKind::UndefMethod => Predicate::ColumnOverride,
            AttrKind::Attribute
            | AttrKind::AttrAccessor
            | AttrKind::AttrReader
            | AttrKind::AttrReadonly
            | AttrKind::StoreAttribute
            | AttrKind::StoreAccessor
            | AttrKind::Serialize
            | AttrKind::Enum
            | AttrKind::DefineAttributeMethod => Predicate::HasAttribute,
        };
        self.push(
            model_iri.to_string(),
            pred,
            a.name.clone(),
            Provenance::OpenProjectExtracted,
        );
        // D-AR-5.2: emit `(model.field, field_type, "<rails_type>")`
        // when the AttrDecl carries an explicit type annotation. The
        // Rails AST literal (`attribute :age, :integer` →
        // `options[("type", "integer")]`) is the static type signal
        // the downstream Schema consumer needs to upgrade `Kind::Any`
        // into a concrete SurrealQL kind.
        if let Some(rails_type) = field_type_from_options(&a.options) {
            self.push(
                format!("{model_iri}.{}", a.name),
                Predicate::FieldType,
                rails_type,
                Provenance::OpenProjectExtracted,
            );
        }
    }

    fn delegation(&mut self, model_iri: &str, d: &Delegation) {
        // Honour Rails' `prefix:` option (codex P2 PR #5):
        //   delegate :name, :identifier, to: :project, prefix: true
        //     → exposes `project_name` and `project_identifier` on the
        //       caller (NOT `name` / `identifier`).
        //   delegate :name, to: :project, prefix: :owner
        //     → exposes `owner_name`.
        // Without honouring this, the graph would record an edge to a
        // method that doesn't exist (`name`) while queries for the real
        // method (`project_name`) would miss.
        let prefix = delegate_prefix(d);
        for method in &d.methods {
            let exposed = match &prefix {
                Some(p) => format!("{p}_{method}"),
                None => method.clone(),
            };
            self.push(
                model_iri.to_string(),
                Predicate::DelegatesTo,
                format!("{exposed}=>via:{}", d.to),
                Provenance::OpenProjectExtracted,
            );
        }
    }

    fn scope(&mut self, model_iri: &str, s: &ScopeDecl) {
        match s.kind {
            ScopeKind::Scope => {
                self.push(
                    model_iri.to_string(),
                    Predicate::HasScope,
                    format!("{}={}", s.name, s.body_ref),
                    Provenance::OpenProjectExtracted,
                );
            }
            ScopeKind::DefaultScope => {
                self.push(
                    model_iri.to_string(),
                    Predicate::HasDefaultScope,
                    s.body_ref.clone(),
                    Provenance::OpenProjectExtracted,
                );
            }
            ScopeKind::Scopes => {
                // OP plural form: one HasScope per name; body_ref carries
                // a placeholder (the plural form has no per-scope lambda).
                self.push(
                    model_iri.to_string(),
                    Predicate::HasScope,
                    format!("{}={}", s.name, s.body_ref),
                    Provenance::OpenProjectExtracted,
                );
            }
        }
    }

    fn acts_as(&mut self, model_iri: &str, aa: &ActsAs) {
        let obj = if aa.options.is_empty() {
            aa.variant.clone()
        } else {
            let opts = aa
                .options
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(",");
            format!("{}:{}", aa.variant, opts)
        };
        self.push(
            model_iri.to_string(),
            Predicate::ActsAs,
            obj,
            Provenance::OpenProjectExtracted,
        );
    }

    fn dsl_call(&mut self, model_iri: &str, dc: &DslCall) {
        let pred = match dc.name.as_str() {
            "register_journal_formatter" => Predicate::RegistersJournalFormatter,
            "register_journal_formatted_fields" => Predicate::RegistersJournalFormattedFields,
            _ => Predicate::HasDslCall,
        };
        let obj = match pred {
            Predicate::HasDslCall => format!("{}({})", dc.name, dc.args),
            _ => dc.args.clone(),
        };
        self.push(
            model_iri.to_string(),
            pred,
            obj,
            Provenance::OpenProjectExtracted,
        );
    }

    fn gem_dsl(&mut self, model_iri: &str, g: &GemDsl) {
        let pred = match g.gem {
            GemKind::MountUploader => Predicate::MountsUploader,
            GemKind::HasPaperTrail => Predicate::HasPaperTrail,
            GemKind::HasClosureTree => Predicate::HasClosureTree,
            GemKind::CounterCulture => Predicate::CounterCultures,
            GemKind::AutoStripAttributes => Predicate::AutoStrips,
        };
        self.push(
            model_iri.to_string(),
            pred,
            g.args.clone(),
            Provenance::OpenProjectExtracted,
        );
    }

    fn dynamic_method(&mut self, model_iri: &str, dm: &DynMethod) {
        self.push(
            model_iri.to_string(),
            Predicate::DefinesMethod,
            format!("{}={}", dm.name_expr, dm.body_ref),
            Provenance::Inferred,
        );
    }

    fn refinement(&mut self, model_iri: &str, r: &UsingRef) {
        self.push(
            model_iri.to_string(),
            Predicate::UsesRefinement,
            r.refinement_module.clone(),
            Provenance::OpenProjectExtracted,
        );
    }

    fn sti(&mut self, model_iri: &str, sti: &StiInfo) {
        if let Some(parent) = &sti.inherits_from {
            self.push(
                model_iri.to_string(),
                Predicate::IncludesModule,
                parent.clone(),
                Provenance::OpenProjectExtracted,
            );
        }
        // abstract_class / inheritance_column carried on IR only.
        let _ = sti.abstract_class;
        let _ = &sti.inheritance_column;
    }

    // ───── C++ machine-plane arms ─────

    fn cpp_base(&mut self, ns: &str, model_iri: &str, base: &CppBase) {
        // Access specifier + virtual-inheritance flag are carried on the IR
        // (CppBase) but NOT emitted — the object stays a clean base-class
        // IRI for graph traversal (mirrors `association` dropping AssocKind).
        let _ = base.access;
        let _ = base.virtual_base;
        self.push(
            model_iri.to_string(),
            Predicate::InheritsFrom,
            format!("{ns}:{}", base.name),
            Provenance::CppExtracted,
        );
    }

    fn cpp_field(&mut self, model_iri: &str, field: &CppField) {
        let field_iri = format!("{model_iri}.{}", field.name);
        // Structural classification, then the ownership edge.
        self.push(
            field_iri.clone(),
            Predicate::RdfType,
            EntityKind::Property.iri().to_string(),
            Provenance::Structural,
        );
        let _ = &field.type_name; // carried on IR for catalog consumers
        self.push(
            model_iri.to_string(),
            Predicate::HasField,
            field_iri,
            Provenance::CppExtracted,
        );
    }

    fn cpp_method(&mut self, ns: &str, model_iri: &str, method: &CppMethod) {
        let method_iri = format!("{model_iri}.{}", method.name);
        // Universal classification — same shape the core 7 give a Function.
        self.push(
            method_iri.clone(),
            Predicate::RdfType,
            EntityKind::Function.iri().to_string(),
            Provenance::Structural,
        );
        self.push(
            model_iri.to_string(),
            Predicate::HasFunction,
            method_iri.clone(),
            Provenance::Structural,
        );
        // Property flags → method-property predicates.
        if method.is_pure_virtual {
            self.push(
                method_iri.clone(),
                Predicate::IsPureVirtual,
                "true".to_string(),
                Provenance::CppExtracted,
            );
        }
        if let Some(kind) = method.constexpr_kind {
            let marker = match kind {
                ConstexprKind::Constexpr => "constexpr",
                ConstexprKind::Consteval => "consteval",
            };
            self.push(
                method_iri.clone(),
                Predicate::IsConstexpr,
                marker.to_string(),
                Provenance::CppExtracted,
            );
        }
        if method.is_noexcept {
            self.push(
                method_iri.clone(),
                Predicate::IsNoexcept,
                "true".to_string(),
                Provenance::CppExtracted,
            );
        }
        if let Some(base_method) = &method.overrides {
            self.push(
                method_iri.clone(),
                Predicate::VirtuallyOverrides,
                format!("{ns}:{base_method}"),
                Provenance::CppExtracted,
            );
        }
        if let Some(op) = &method.operator_kind {
            self.push(
                method_iri.clone(),
                Predicate::DefinesOperator,
                op.clone(),
                Provenance::CppExtracted,
            );
        }
        if let Some(req) = &method.requires_clause {
            // Last potential use of `method_iri` — move, don't clone.
            self.push(
                method_iri,
                Predicate::RequiresConcept,
                req.clone(),
                Provenance::CppExtracted,
            );
        }
    }

    fn cpp_template(&mut self, model_iri: &str, tpl: &CppTemplate) {
        let (pred, prov) = match tpl.kind {
            CppTemplateKind::Specialisation => {
                (Predicate::TemplateSpecialises, Provenance::CppExtracted)
            }
            CppTemplateKind::Instantiation => {
                (Predicate::TemplateInstantiates, Provenance::Inferred)
            }
        };
        self.push(model_iri.to_string(), pred, tpl.name.clone(), prov);
    }

    fn cpp_friend(&mut self, model_iri: &str, fr: &CppFriend) {
        self.push(
            model_iri.to_string(),
            Predicate::IsFriendOf,
            fr.name.clone(),
            Provenance::Structural,
        );
    }

    fn cpp_macro_use(&mut self, model_iri: &str, mu: &CppMacroUse) {
        self.push(
            model_iri.to_string(),
            Predicate::UsesMacroExpansion,
            format!("{}<={}", mu.identifier, mu.macro_name),
            Provenance::Inferred,
        );
    }

    fn cpp_static_assert(&mut self, model_iri: &str, sa: &CppStaticAssert) {
        self.push(
            model_iri.to_string(),
            Predicate::StaticAsserts,
            sa.condition.clone(),
            Provenance::CppExtracted,
        );
    }
}

/// Resolve the Rails type annotation from `AttrDecl::options`.
///
/// The extractor stores the type-positional Sym (e.g. the `:integer`
/// in `attribute :age, :integer`) as `options = [("type", "integer")]`.
/// Returns `None` when no explicit type is recorded.
fn field_type_from_options(options: &[(String, String)]) -> Option<String> {
    options
        .iter()
        .find_map(|(k, v)| (k == "type" && !v.is_empty()).then(|| v.clone()))
}

fn delegate_prefix(d: &Delegation) -> Option<String> {
    for (k, v) in &d.options {
        if k == "prefix" {
            let trimmed = v.trim();
            return match trimmed {
                "true" => Some(d.to.clone()),
                "false" | "nil" => None,
                // Strip leading `:` (symbol) or surrounding quotes (string).
                other => {
                    let stripped = other
                        .strip_prefix(':')
                        .or_else(|| other.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
                        .or_else(|| other.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
                        .unwrap_or(other);
                    if stripped.is_empty() {
                        None
                    } else {
                        Some(stripped.to_string())
                    }
                }
            };
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        AssocKind, CppAccess, CppBase, CppField, CppFriend, CppMacroUse, CppMethod,
        CppStaticAssert, CppTemplate, CppTemplateKind, Field, Function, Model,
    };

    /// A minimal account_move-shaped graph mirroring the Odoo fixture used
    /// in `lance_graph::graph::spo::action_emitter`.
    fn fixture() -> ModelGraph {
        ModelGraph {
            namespace: "odoo".to_string(),
            models: vec![Model {
                name: "account_move".to_string(),
                fields: vec![Field {
                    name: "amount_total".to_string(),
                    depends_on: vec!["line_ids.balance".to_string()],
                    emitted_by: Some("_compute_amount".to_string()),
                }],
                functions: vec![Function {
                    name: "_compute_amount".to_string(),
                    reads: vec!["currency_id".to_string()],
                    raises: vec!["UserError".to_string()],
                    traverses: vec!["line_ids".to_string()],
                }],
                ..Default::default()
            }],
        }
    }

    #[test]
    fn expands_all_predicate_classes() {
        let triples = expand(&fixture());
        let has =
            |s: &str, p: &str, o: &str| triples.iter().any(|t| t.s == s && t.p == p && t.o == o);

        assert!(has("odoo:account_move", "rdf:type", "ogit:ObjectType"));
        assert!(has(
            "odoo:account_move.amount_total",
            "rdf:type",
            "ogit:Property"
        ));
        assert!(has(
            "odoo:account_move._compute_amount",
            "rdf:type",
            "ogit:Function"
        ));
        assert!(has(
            "odoo:account_move",
            "has_function",
            "odoo:account_move._compute_amount"
        ));
        assert!(has(
            "odoo:account_move.amount_total",
            "emitted_by",
            "odoo:account_move._compute_amount"
        ));
        assert!(has(
            "odoo:account_move.amount_total",
            "depends_on",
            "odoo:account_move.line_ids.balance"
        ));
        assert!(has(
            "odoo:account_move._compute_amount",
            "reads_field",
            "odoo:account_move.currency_id"
        ));
        assert!(has(
            "odoo:account_move._compute_amount",
            "raises",
            "exc:UserError"
        ));
        assert!(has(
            "odoo:account_move._compute_amount",
            "traverses_relation",
            "odoo:account_move.line_ids"
        ));
    }

    #[test]
    fn output_is_sorted_and_deterministic() {
        let a = expand(&fixture());
        let b = expand(&fixture());
        assert_eq!(a, b, "expansion must be deterministic");
        for w in a.windows(2) {
            assert!(w[0].key() <= w[1].key(), "triples not sorted by (s,p,o)");
        }
    }

    #[test]
    fn duplicate_edges_collapse() {
        let mut g = fixture();
        // Push a duplicate depends_on.
        g.models[0].fields[0]
            .depends_on
            .push("line_ids.balance".to_string());
        let triples = expand(&g);
        let count = triples
            .iter()
            .filter(|t| t.p == "depends_on" && t.o == "odoo:account_move.line_ids.balance")
            .count();
        assert_eq!(count, 1, "duplicate depends_on must collapse");
    }

    #[test]
    fn truth_tiers_are_assigned_per_predicate() {
        let triples = expand(&fixture());
        let truth = |p: &str, o: &str| {
            triples
                .iter()
                .find(|t| t.p == p && t.o == o)
                .map(|t| (t.f, t.c))
        };
        // Structural
        assert_eq!(truth("rdf:type", "ogit:ObjectType"), Some((1.0, 1.0)));
        // Authoritative
        assert_eq!(
            truth("emitted_by", "odoo:account_move._compute_amount"),
            Some((0.95, 0.90))
        );
        // Inferred
        assert_eq!(
            truth("reads_field", "odoo:account_move.currency_id"),
            Some((0.85, 0.75))
        );
    }

    #[test]
    fn empty_graph_yields_no_triples() {
        let g = ModelGraph::new("openproject");
        assert!(expand(&g).is_empty());
    }

    // ────────────────── OpenProject AR-shape tests ──────────────────

    /// A fully-populated WorkPackage-shaped model exercising every AR-shape
    /// match arm. This is the [`crate::expand`] half of the
    /// `Declaration → Triple` round-trip the council gate asks for.
    fn ar_fixture() -> ModelGraph {
        let mut wp = Model::new("WorkPackage");
        wp.associations.push(AssocDecl {
            kind: AssocKind::BelongsTo,
            name: "project".to_string(),
            options: vec![("class_name".to_string(), "Project".to_string())],
        });
        wp.associations.push(AssocDecl {
            kind: AssocKind::HasMany,
            name: "time_entries".to_string(),
            options: vec![],
        });
        wp.validations.push(Validation {
            kind: ValidationKind::Validates,
            target: "subject".to_string(),
            options: vec![("presence".to_string(), "true".to_string())],
        });
        wp.validations.push(Validation {
            kind: ValidationKind::Normalizes,
            target: "email".to_string(),
            options: vec![],
        });
        wp.callbacks.push(Callback {
            phase: "before_save".to_string(),
            target: "set_default_status".to_string(),
            options: vec![],
        });
        wp.concerns.push(ConcernRef {
            kind: ConcernKind::Include,
            module: "Acts::Customizable".to_string(),
            body_ref: None,
        });
        wp.concerns.push(ConcernRef {
            kind: ConcernKind::Extend,
            module: "Pagination::Model".to_string(),
            body_ref: None,
        });
        wp.concerns.push(ConcernRef {
            kind: ConcernKind::Prepend,
            module: "Overrides".to_string(),
            body_ref: None,
        });
        wp.concerns.push(ConcernRef {
            kind: ConcernKind::ClassMethodsBlock,
            module: "WorkPackage".to_string(),
            body_ref: Some("methods.rb:42-58".to_string()),
        });
        wp.concerns.push(ConcernRef {
            kind: ConcernKind::IncludedBlock,
            module: "WorkPackage".to_string(),
            body_ref: Some("methods.rb:60-72".to_string()),
        });
        wp.attributes.push(AttrDecl {
            kind: AttrKind::Attribute,
            name: "estimated_hours".to_string(),
            options: vec![("type".to_string(), "decimal".to_string())],
        });
        wp.attributes.push(AttrDecl {
            kind: AttrKind::AttrAccessor,
            name: "virtual_flag".to_string(),
            options: vec![],
        });
        wp.attributes.push(AttrDecl {
            kind: AttrKind::AliasAttribute,
            name: "title=subject".to_string(),
            options: vec![],
        });
        wp.attributes.push(AttrDecl {
            kind: AttrKind::AliasMethod,
            name: "title=subject".to_string(),
            options: vec![],
        });
        wp.attributes.push(AttrDecl {
            kind: AttrKind::UndefMethod,
            name: "deprecated_column".to_string(),
            options: vec![],
        });
        wp.attributes.push(AttrDecl {
            kind: AttrKind::Serialize,
            name: "preferences".to_string(),
            options: vec![("serializer".to_string(), "JSON".to_string())],
        });
        wp.delegations.push(Delegation {
            methods: vec!["name".to_string(), "identifier".to_string()],
            to: "project".to_string(),
            options: vec![("prefix".to_string(), "project".to_string())],
        });
        wp.scopes.push(ScopeDecl {
            kind: ScopeKind::Scope,
            name: "open".to_string(),
            body_ref: "wp.rb:120".to_string(),
        });
        wp.scopes.push(ScopeDecl {
            kind: ScopeKind::DefaultScope,
            name: String::new(),
            body_ref: "wp.rb:130".to_string(),
        });
        wp.scopes.push(ScopeDecl {
            kind: ScopeKind::Scopes,
            name: "by_priority".to_string(),
            body_ref: "<plural>".to_string(),
        });
        wp.acts_as.push(ActsAs {
            variant: "list".to_string(),
            options: vec![("scope".to_string(), ":project_id".to_string())],
        });
        wp.acts_as.push(ActsAs {
            variant: "watchable".to_string(),
            options: vec![],
        });
        wp.dsl_calls.push(DslCall {
            name: "register_journal_formatter".to_string(),
            args: ":diff,:custom".to_string(),
        });
        wp.dsl_calls.push(DslCall {
            name: "register_journal_formatted_fields".to_string(),
            args: ":subject".to_string(),
        });
        wp.dsl_calls.push(DslCall {
            name: "activity_provider_for".to_string(),
            args: ":work_packages".to_string(),
        });
        wp.gem_dsl.push(GemDsl {
            gem: GemKind::MountUploader,
            args: ":attachments=AttachmentUploader".to_string(),
        });
        wp.gem_dsl.push(GemDsl {
            gem: GemKind::HasPaperTrail,
            args: "on: [:update]".to_string(),
        });
        wp.gem_dsl.push(GemDsl {
            gem: GemKind::HasClosureTree,
            args: String::new(),
        });
        wp.gem_dsl.push(GemDsl {
            gem: GemKind::CounterCulture,
            args: ":project=>:work_packages_count".to_string(),
        });
        wp.gem_dsl.push(GemDsl {
            gem: GemKind::AutoStripAttributes,
            args: ":subject,:description".to_string(),
        });
        wp.dynamic_methods.push(DynMethod {
            name_expr: ":custom_for_#{field}".to_string(),
            body_ref: "wp.rb:200-210".to_string(),
        });
        wp.refinements.push(UsingRef {
            refinement_module: "OpenProject::DateRange".to_string(),
        });
        wp.sti = Some(StiInfo {
            inherits_from: Some("Issue".to_string()),
            abstract_class: false,
            inheritance_column: Some("type".to_string()),
        });

        ModelGraph {
            namespace: "openproject".to_string(),
            models: vec![wp],
        }
    }

    #[test]
    fn ar_shape_emits_declares_association() {
        let triples = expand(&ar_fixture());
        let has =
            |s: &str, p: &str, o: &str| triples.iter().any(|t| t.s == s && t.p == p && t.o == o);
        assert!(has(
            "openproject:WorkPackage",
            "declares_association",
            "openproject:WorkPackage.project"
        ));
        assert!(has(
            "openproject:WorkPackage",
            "declares_association",
            "openproject:WorkPackage.time_entries"
        ));
    }

    /// Every `declares_association` triple gets a sibling
    /// `association_kind` triple on the relation IRI naming the Rails
    /// macro that declared it. Downstream schema codegen reads this to
    /// gate FK-column emission (only `belongs_to` puts a column on the
    /// declaring class; `has_many`/`has_one` keep the FK on the other
    /// table — without the kind triple, ~57 % of the OpenProject
    /// corpus's record FKs are phantom).
    #[test]
    fn ar_shape_emits_association_kind_per_relation() {
        let triples = expand(&ar_fixture());
        let has =
            |s: &str, p: &str, o: &str| triples.iter().any(|t| t.s == s && t.p == p && t.o == o);
        // The fixture declares `belongs_to :project` and
        // `has_many :time_entries`.
        assert!(has(
            "openproject:WorkPackage.project",
            "association_kind",
            "belongs_to",
        ));
        assert!(has(
            "openproject:WorkPackage.time_entries",
            "association_kind",
            "has_many",
        ));
        // The other 3 AssocKind variants — locked via a focused
        // fixture below so a future enum change can't silently drop
        // the mapping.
    }

    /// All 5 `AssocKind` variants map to the documented kind string.
    #[test]
    fn association_kind_string_table() {
        let mut g = ModelGraph::new("openproject");
        let mut m = Model::new("M");
        for (kind, _label) in [
            (AssocKind::BelongsTo, "belongs_to"),
            (AssocKind::HasMany, "has_many"),
            (AssocKind::HasOne, "has_one"),
            (AssocKind::HasAndBelongsToMany, "has_and_belongs_to_many"),
            (
                AssocKind::AcceptsNestedAttributesFor,
                "accepts_nested_attributes_for",
            ),
        ] {
            m.associations.push(AssocDecl {
                kind,
                name: format!("{kind:?}").to_lowercase(),
                options: vec![],
            });
        }
        g.models.push(m);
        let triples = expand(&g);
        let kinds_seen: std::collections::BTreeSet<&str> = triples
            .iter()
            .filter(|t| t.p == "association_kind")
            .map(|t| t.o.as_str())
            .collect();
        for expected in [
            "belongs_to",
            "has_many",
            "has_one",
            "has_and_belongs_to_many",
            "accepts_nested_attributes_for",
        ] {
            assert!(
                kinds_seen.contains(expected),
                "association_kind `{expected}` missing from emission; saw {kinds_seen:?}",
            );
        }
    }

    #[test]
    fn ar_shape_emits_validates_and_normalizes() {
        let triples = expand(&ar_fixture());
        assert!(
            triples
                .iter()
                .any(|t| t.p == "validates_constraint" && t.o == "subject")
        );
        assert!(
            triples
                .iter()
                .any(|t| t.p == "normalizes_attribute" && t.o == "email")
        );
    }

    #[test]
    fn ar_shape_emits_callback_with_phase_in_object() {
        let triples = expand(&ar_fixture());
        assert!(
            triples
                .iter()
                .any(|t| t.p == "has_callback" && t.o == "before_save:set_default_status")
        );
    }

    #[test]
    fn ar_shape_emits_concern_composition_per_kind() {
        let triples = expand(&ar_fixture());
        let has = |p: &str, o: &str| triples.iter().any(|t| t.p == p && t.o == o);
        assert!(has("includes_module", "Acts::Customizable"));
        assert!(has("extends_module", "Pagination::Model"));
        assert!(has("prepends_module", "Overrides"));
        assert!(has("concern_class_methods", "methods.rb:42-58"));
        assert!(has("concern_included_block", "methods.rb:60-72"));
    }

    #[test]
    fn ar_shape_concern_blocks_carry_structural_truth() {
        let triples = expand(&ar_fixture());
        let truth = |p: &str| {
            triples
                .iter()
                .find(|t| t.p == p)
                .map(|t| (t.f, t.c))
                .unwrap()
        };
        assert_eq!(truth("concern_class_methods"), (1.0, 1.0));
        assert_eq!(truth("concern_included_block"), (1.0, 1.0));
    }

    #[test]
    fn ar_shape_emits_attributes_per_kind() {
        let triples = expand(&ar_fixture());
        let has = |p: &str, o: &str| triples.iter().any(|t| t.p == p && t.o == o);
        assert!(has("has_attribute", "estimated_hours"));
        assert!(has("has_attribute", "virtual_flag"));
        assert!(has("has_attribute", "preferences"));
        assert!(has("aliases_attribute", "title=subject"));
        assert!(has("aliases_method", "title=subject"));
        assert!(has("column_override", "deprecated_column"));
    }

    #[test]
    fn ar_shape_expands_delegation_to_one_triple_per_method() {
        // The fixture's `delegate :name, :identifier, to: :project, prefix: "project"`
        // exposes `project_name` / `project_identifier` (NOT `name` /
        // `identifier`), per Rails' `prefix:` semantics. Codex P2 PR #5
        // — the expander now honours the prefix option.
        let triples = expand(&ar_fixture());
        assert!(
            triples
                .iter()
                .any(|t| t.p == "delegates_to" && t.o == "project_name=>via:project"),
            "delegate :name + prefix: \"project\" → exposes project_name"
        );
        assert!(
            triples
                .iter()
                .any(|t| t.p == "delegates_to" && t.o == "project_identifier=>via:project")
        );
        // The un-prefixed forms must NOT appear (the original methods
        // do not exist on the caller class).
        assert!(
            !triples
                .iter()
                .any(|t| t.p == "delegates_to" && t.o == "name=>via:project"),
            "un-prefixed name must NOT appear when prefix: is set",
        );
    }

    /// **Codex P2 regression (PR #5 r…)** — verify each `prefix:` shape
    /// (true / symbol / false / absent) maps to the correct exposed
    /// method name.
    #[test]
    fn delegate_prefix_option_shapes() {
        let cases = [
            // (prefix-option-value, expected-prefix-or-none)
            ("true", Some("project".to_string())), // prefix: true → use `to`
            (":owner", Some("owner".to_string())), // prefix: :owner
            ("\"owner\"", Some("owner".to_string())), // prefix: "owner"
            ("false", None),                       // prefix: false → no rename
            ("nil", None),
        ];
        for (opt_value, expected) in cases {
            let d = crate::ir::Delegation {
                methods: vec!["name".to_string()],
                to: "project".to_string(),
                options: vec![("prefix".to_string(), opt_value.to_string())],
            };
            assert_eq!(
                delegate_prefix(&d),
                expected,
                "prefix: {opt_value} should yield {expected:?}",
            );
        }
        // Absent prefix → None.
        let d = crate::ir::Delegation {
            methods: vec!["name".to_string()],
            to: "project".to_string(),
            options: vec![],
        };
        assert_eq!(delegate_prefix(&d), None);
    }

    #[test]
    fn ar_shape_emits_scopes_per_kind() {
        let triples = expand(&ar_fixture());
        let has = |p: &str, o: &str| triples.iter().any(|t| t.p == p && t.o == o);
        assert!(has("has_scope", "open=wp.rb:120"));
        assert!(has("has_default_scope", "wp.rb:130"));
        assert!(has("has_scope", "by_priority=<plural>"));
    }

    #[test]
    fn ar_shape_emits_acts_as_with_variant_and_options() {
        let triples = expand(&ar_fixture());
        assert!(
            triples
                .iter()
                .any(|t| t.p == "acts_as" && t.o.starts_with("list:"))
        );
        assert!(
            triples
                .iter()
                .any(|t| t.p == "acts_as" && t.o == "watchable")
        );
    }

    #[test]
    fn ar_shape_routes_dsl_calls_by_name() {
        let triples = expand(&ar_fixture());
        // Promoted predicates carry just the args (not the name).
        assert!(
            triples
                .iter()
                .any(|t| t.p == "registers_journal_formatter" && t.o == ":diff,:custom")
        );
        assert!(
            triples
                .iter()
                .any(|t| t.p == "registers_journal_formatted_fields" && t.o == ":subject")
        );
        // Catch-all carries name(args).
        assert!(
            triples
                .iter()
                .any(|t| t.p == "has_dsl_call" && t.o == "activity_provider_for(:work_packages)")
        );
    }

    #[test]
    fn ar_shape_emits_gem_dsl_per_gem() {
        let triples = expand(&ar_fixture());
        assert!(triples.iter().any(|t| t.p == "mounts_uploader"));
        assert!(triples.iter().any(|t| t.p == "has_paper_trail"));
        assert!(triples.iter().any(|t| t.p == "has_closure_tree"));
        assert!(triples.iter().any(|t| t.p == "counter_cultures"));
        assert!(triples.iter().any(|t| t.p == "auto_strips"));
    }

    #[test]
    fn ar_shape_defines_method_uses_inferred_per_edge() {
        let triples = expand(&ar_fixture());
        let t = triples.iter().find(|t| t.p == "defines_method").unwrap();
        assert_eq!((t.f, t.c), (0.85, 0.75));
    }

    #[test]
    fn ar_shape_emits_refinement_and_sti() {
        let triples = expand(&ar_fixture());
        assert!(
            triples
                .iter()
                .any(|t| t.p == "uses_refinement" && t.o == "OpenProject::DateRange")
        );
        // STI → includes_module to parent.
        assert!(
            triples
                .iter()
                .any(|t| t.p == "includes_module" && t.o == "Issue")
        );
    }

    #[test]
    fn ar_shape_op_extracted_triples_carry_calibrated_truth() {
        let triples = expand(&ar_fixture());
        // declares_association uses OpenProjectExtracted
        let t = triples
            .iter()
            .find(|t| t.p == "declares_association")
            .unwrap();
        assert_eq!((t.f, t.c), (0.95, 0.88));
    }

    /// Coverage proof for D-AR-2: every predicate the AR-shape declares
    /// fires from this fixture. The expansion covers all of the new
    /// predicates except `reads_field` and `traverses_relation` (those
    /// are part of the core 7 and need a populated `Function` IR which
    /// this fixture intentionally omits).
    #[test]
    fn ar_shape_emits_every_ar_predicate() {
        let triples = expand(&ar_fixture());
        let predicates_seen: BTreeSet<&str> = triples.iter().map(|t| t.p.as_str()).collect();
        for p in [
            // OpenProjectExtracted defaults
            "declares_association",
            "validates_constraint",
            "normalizes_attribute",
            "has_callback",
            "includes_module",
            "extends_module",
            "prepends_module",
            "has_attribute",
            "aliases_attribute",
            "aliases_method",
            "column_override",
            "delegates_to",
            "has_scope",
            "has_default_scope",
            "acts_as",
            "registers_journal_formatter",
            "registers_journal_formatted_fields",
            "has_dsl_call",
            "mounts_uploader",
            "has_paper_trail",
            "has_closure_tree",
            "counter_cultures",
            "auto_strips",
            "uses_refinement",
            "association_kind",
            // Inferred (per-edge override)
            "defines_method",
            // Structural (block markers)
            "concern_class_methods",
            "concern_included_block",
        ] {
            assert!(
                predicates_seen.contains(p),
                "AR-shape predicate `{p}` was not emitted by the fixture — \
                 D-AR-2 expand match arm missing",
            );
        }
    }

    // ────────────────── C++ machine-plane tests ──────────────────

    /// A `Tesseract::Recognizer`-shaped model exercising every C++ match
    /// arm. The expand half of the `CppClass → Triple` round-trip the
    /// `ruff_cpp_spo` locked-shape test mirrors.
    fn cpp_fixture() -> ModelGraph {
        let mut rec = Model::new("Tesseract::Recognizer");
        rec.bases.push(CppBase {
            name: "Tesseract::Classify".to_string(),
            access: CppAccess::Public,
            virtual_base: false,
        });
        rec.member_fields.push(CppField {
            name: "recognizer_".to_string(),
            type_name: "std::unique_ptr<LSTMRecognizer>".to_string(),
        });
        rec.methods.push(CppMethod {
            name: "Recognize".to_string(),
            is_pure_virtual: false,
            constexpr_kind: None,
            is_noexcept: true,
            overrides: Some("Tesseract::Classify.Recognize".to_string()),
            operator_kind: None,
            requires_clause: None,
        });
        rec.methods.push(CppMethod {
            name: "Clear".to_string(),
            is_pure_virtual: true,
            constexpr_kind: None,
            is_noexcept: false,
            overrides: None,
            operator_kind: None,
            requires_clause: None,
        });
        rec.methods.push(CppMethod {
            name: "kMaxRating".to_string(),
            is_pure_virtual: false,
            constexpr_kind: Some(ConstexprKind::Constexpr),
            is_noexcept: false,
            overrides: None,
            operator_kind: None,
            requires_clause: None,
        });
        rec.methods.push(CppMethod {
            name: "operator==".to_string(),
            is_pure_virtual: false,
            constexpr_kind: None,
            is_noexcept: false,
            overrides: None,
            operator_kind: Some("operator==".to_string()),
            requires_clause: Some("std::equality_comparable<T>".to_string()),
        });
        rec.templates.push(CppTemplate {
            kind: CppTemplateKind::Specialisation,
            name: "GenericVector<int>".to_string(),
        });
        rec.templates.push(CppTemplate {
            kind: CppTemplateKind::Instantiation,
            name: "GenericVector<float>".to_string(),
        });
        rec.friends.push(CppFriend {
            name: "TessdataManager".to_string(),
        });
        rec.macro_uses.push(CppMacroUse {
            identifier: "BOOL_MEMBER".to_string(),
            macro_name: "INT_MEMBER".to_string(),
        });
        rec.static_asserts.push(CppStaticAssert {
            condition: "sizeof(int) == 4".to_string(),
        });
        ModelGraph {
            namespace: "cpp".to_string(),
            models: vec![rec],
        }
    }

    #[test]
    fn cpp_classifies_class_fields_and_methods() {
        let triples = expand(&cpp_fixture());
        let has =
            |s: &str, p: &str, o: &str| triples.iter().any(|t| t.s == s && t.p == p && t.o == o);
        // Class + member + method classification (reuses the core arms).
        assert!(has(
            "cpp:Tesseract::Recognizer",
            "rdf:type",
            "ogit:ObjectType"
        ));
        assert!(has(
            "cpp:Tesseract::Recognizer.recognizer_",
            "rdf:type",
            "ogit:Property"
        ));
        assert!(has(
            "cpp:Tesseract::Recognizer.Recognize",
            "rdf:type",
            "ogit:Function"
        ));
        assert!(has(
            "cpp:Tesseract::Recognizer",
            "has_function",
            "cpp:Tesseract::Recognizer.Recognize"
        ));
        assert!(has(
            "cpp:Tesseract::Recognizer",
            "has_field",
            "cpp:Tesseract::Recognizer.recognizer_"
        ));
    }

    #[test]
    fn cpp_emits_inheritance_as_clean_base_iri() {
        let triples = expand(&cpp_fixture());
        assert!(triples.iter().any(|t| {
            t.s == "cpp:Tesseract::Recognizer"
                && t.p == "inherits_from"
                && t.o == "cpp:Tesseract::Classify"
        }));
    }

    #[test]
    fn cpp_emits_every_method_property_predicate() {
        let triples = expand(&cpp_fixture());
        let has = |p: &str, o: &str| triples.iter().any(|t| t.p == p && t.o == o);
        assert!(has("is_noexcept", "true"));
        assert!(has("is_pure_virtual", "true"));
        assert!(has("is_constexpr", "constexpr"));
        assert!(has(
            "virtually_overrides",
            "cpp:Tesseract::Classify.Recognize"
        ));
        assert!(has("defines_operator", "operator=="));
        assert!(has("requires_concept", "std::equality_comparable<T>"));
    }

    #[test]
    fn cpp_emits_templates_friends_macros_static_asserts() {
        let triples = expand(&cpp_fixture());
        let has = |p: &str, o: &str| triples.iter().any(|t| t.p == p && t.o == o);
        assert!(has("template_specialises", "GenericVector<int>"));
        assert!(has("template_instantiates", "GenericVector<float>"));
        assert!(has("is_friend_of", "TessdataManager"));
        assert!(has("uses_macro_expansion", "BOOL_MEMBER<=INT_MEMBER"));
        assert!(has("static_asserts", "sizeof(int) == 4"));
    }

    #[test]
    fn cpp_truth_tiers_match_calibration() {
        let triples = expand(&cpp_fixture());
        let truth = |p: &str| triples.iter().find(|t| t.p == p).map(|t| (t.f, t.c));
        // CppExtracted default
        assert_eq!(truth("inherits_from"), Some((0.95, 0.82)));
        assert_eq!(truth("has_field"), Some((0.95, 0.82)));
        // Structural per-edge override
        assert_eq!(truth("is_friend_of"), Some((1.0, 1.0)));
        // Inferred per-edge overrides
        assert_eq!(truth("uses_macro_expansion"), Some((0.85, 0.75)));
        assert_eq!(truth("template_instantiates"), Some((0.85, 0.75)));
    }

    /// Every C++ machine-plane predicate fires from the fixture — the C++
    /// analog of `ar_shape_emits_every_ar_predicate`.
    #[test]
    fn cpp_emits_every_cpp_predicate() {
        let triples = expand(&cpp_fixture());
        let seen: BTreeSet<&str> = triples.iter().map(|t| t.p.as_str()).collect();
        for p in [
            "inherits_from",
            "has_field",
            "template_specialises",
            "template_instantiates",
            "virtually_overrides",
            "is_friend_of",
            "defines_operator",
            "uses_macro_expansion",
            "is_pure_virtual",
            "is_constexpr",
            "is_noexcept",
            "requires_concept",
            "static_asserts",
        ] {
            assert!(
                seen.contains(p),
                "C++ predicate `{p}` was not emitted by the fixture — expand arm missing",
            );
        }
    }

    /// The Python/Ruby fixtures must emit ZERO C++ predicates — the C++
    /// sibling Vecs default empty, so no cross-language leakage occurs.
    #[test]
    fn non_cpp_fixtures_emit_no_cpp_predicates() {
        let cpp_predicates = [
            "inherits_from",
            "has_field",
            "template_specialises",
            "template_instantiates",
            "virtually_overrides",
            "is_friend_of",
            "defines_operator",
            "uses_macro_expansion",
            "is_pure_virtual",
            "is_constexpr",
            "is_noexcept",
            "requires_concept",
            "static_asserts",
        ];
        for graph in [fixture(), ar_fixture()] {
            let triples = expand(&graph);
            for t in &triples {
                assert!(
                    !cpp_predicates.contains(&t.p.as_str()),
                    "non-C++ fixture leaked C++ predicate `{}`",
                    t.p,
                );
            }
        }
    }

    /// **D-AR-5.2** — when an `AttrDecl` carries a type annotation in
    /// its `options` (key="type"), the expander emits a companion
    /// `field_type` triple alongside the `has_attribute` one. The
    /// triple's subject is the field IRI (`ns:model.field`) so the
    /// downstream Schema consumer can apply the type to the right
    /// `FieldDefinition`.
    #[test]
    fn ar_shape_emits_field_type_for_typed_attribute() {
        let mut g = ModelGraph::new("openproject");
        let mut wp = Model::new("WorkPackage");
        wp.attributes.push(AttrDecl {
            kind: AttrKind::Attribute,
            name: "estimated_hours".to_string(),
            options: vec![("type".to_string(), "decimal".to_string())],
        });
        wp.attributes.push(AttrDecl {
            kind: AttrKind::Attribute,
            name: "subject".to_string(),
            options: vec![], // no type annotation
        });
        g.models.push(wp);
        let triples = expand(&g);
        let has =
            |s: &str, p: &str, o: &str| triples.iter().any(|t| t.s == s && t.p == p && t.o == o);
        assert!(has(
            "openproject:WorkPackage",
            "has_attribute",
            "estimated_hours"
        ));
        assert!(has(
            "openproject:WorkPackage.estimated_hours",
            "field_type",
            "decimal"
        ));
        // No `field_type` triple for the untyped attribute.
        assert!(
            !triples
                .iter()
                .any(|t| t.p == "field_type" && t.o.contains("subject")),
            "untyped attribute must not emit field_type",
        );
    }
}
