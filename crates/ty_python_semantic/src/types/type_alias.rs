use std::fmt::Write;

use crate::{
    Db,
    types::{
        GenericContext, Type, UnionType, definition_expression_type,
        display::qualified_name_components_from_scope, generics::Specialization, visitor,
    },
};
use ty_python_core::{
    definition::{Definition, DefinitionKind},
    scope::ScopeId,
    semantic_index,
};

use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;

/// Strip bare `Type::Divergent(binder_id)` leaves that sit as direct members of
/// the top-level union in `ty`.
///
/// Mirrors the `Divergent`-stripping that the legacy `raw_value_type` cycle
/// recovery path performs via `Union::recursive_type_normalized_impl`, but
/// applied only at the body's outermost union level so that recursive markers
/// nested inside generics (e.g. `list[Divergent | str]`,
/// `tuple[Divergent, ...]`) are preserved.
///
/// - `int | Divergent(binder)` → `int`
/// - `int | tuple[Divergent(binder), ...]` → unchanged
/// - `list[Divergent(binder) | str]` → unchanged
fn strip_top_level_divergent<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    binder_id: salsa::Id,
) -> Type<'db> {
    let Type::Union(union) = ty else {
        return ty;
    };
    let kept: Vec<Type<'db>> = union
        .elements(db)
        .iter()
        .copied()
        .filter(|elem| !matches!(elem, Type::Divergent(d) if d.id() == binder_id))
        .collect();
    if kept.len() == union.elements(db).len() {
        return ty;
    }
    match kept.len() {
        0 => Type::divergent(binder_id),
        1 => kept[0],
        _ => UnionType::from_elements(db, kept),
    }
}

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct PEP695TypeAliasType<'db> {
    #[returns(ref)]
    pub name: Name,

    rhs_scope: ScopeId<'db>,

    pub(super) specialization: Option<Specialization<'db>>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for PEP695TypeAliasType<'_> {}

pub(super) fn walk_pep_695_type_alias<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    type_alias: PEP695TypeAliasType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, type_alias.value_type(db));
}

#[salsa::tracked]
impl<'db> PEP695TypeAliasType<'db> {
    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        let scope = self.rhs_scope(db);
        let type_alias_stmt_node = scope.node(db).expect_type_alias();
        semantic_index(db, scope.file(db)).expect_single_definition(type_alias_stmt_node)
    }

    /// The RHS type of a PEP-695 style type alias with specialization applied.
    ///
    /// For self-referential aliases, the body is folded so that each recursive
    /// position becomes `Type::Divergent(binder_id)`, then any bare
    /// `Divergent` leaf that sits as a direct member of the body's top-level
    /// union is stripped (so `int | Divergent` collapses to `int` for
    /// `IntOr`-style aliases). If the body still contains the binder's
    /// `Divergent` anywhere — nested inside `tuple[Divergent, ...]`,
    /// `list[Divergent | str]`, etc. — wrap the result in `Type::Recursive`;
    /// otherwise return the simplified body directly.
    pub(crate) fn value_type(self, db: &'db dyn Db) -> Type<'db> {
        let raw = self.raw_value_type(db);
        let body = self.apply_function_specialization(db, raw);
        if self.is_self_referential(db) {
            use salsa::plumbing::AsId;
            let binder_id = self.as_id();
            let folded = crate::types::recursive::substitute_self_alias_with_divergent(
                db,
                body,
                self.definition(db),
                binder_id,
            );
            let simplified = strip_top_level_divergent(db, folded, binder_id);
            if simplified.contains_divergent_with_id(db, binder_id) {
                Type::recursive(
                    db,
                    binder_id,
                    Some(crate::types::TypeAliasType::PEP695(self)),
                    simplified,
                )
            } else {
                simplified
            }
        } else {
            body
        }
    }

    /// Whether this alias is *directly* self-referential — its raw body
    /// literally mentions itself as `Type::TypeAlias(a)` with the same
    /// definition. Used to decide when `value_type` should wrap the result in
    /// `Type::Recursive`, and to drive α-binding short-circuits in
    /// `Type::is_redundant_with` and `Type::apply_type_mapping_impl`.
    ///
    /// Limitation: this does not follow mutually-recursive chains
    /// (e.g. `type R = R2 | int; type R2 = R`). Transitive detection would
    /// require either an AST-based walker or a fixpoint over the
    /// type-alias graph.
    ///
    /// `cycle_initial = true` is required because callers (e.g. the `TypeAlias`
    /// arm of `Type::apply_type_mapping_impl`) invoke this method
    /// transitively from inside its own `any_over_type` traversal. Re-entering
    /// for the same alias means the body refers to itself by definition, so
    /// `true` is the sound co-inductive fallback.
    #[allow(dead_code)]
    #[salsa::tracked(
        cycle_initial = |_, _, _| true,
        heap_size = ruff_memory_usage::heap_size,
    )]
    pub(crate) fn is_self_referential(self, db: &'db dyn Db) -> bool {
        let raw = self.raw_value_type(db);
        let self_def = self.definition(db);
        crate::types::visitor::any_over_type(db, raw, false, |ty| match ty {
            Type::TypeAlias(crate::types::TypeAliasType::PEP695(other)) => {
                other.definition(db) == self_def
            }
            _ => false,
        })
    }

    /// The RHS type of a PEP-695 style type alias with *no* specialization applied.
    /// Returns `Divergent` if the type alias is defined cyclically.
    #[salsa::tracked(
        cycle_initial=|_, id, _| Type::divergent(id),
        cycle_fn=|db, cycle, previous: &Type<'db>, value: Type<'db>, _| {
            value.cycle_normalized(db, *previous, cycle)
        },
        heap_size=ruff_memory_usage::heap_size
    )]
    fn raw_value_type(self, db: &'db dyn Db) -> Type<'db> {
        let scope = self.rhs_scope(db);
        let module = parsed_module(db, scope.file(db)).load(db);
        let type_alias_stmt_node = scope.node(db).expect_type_alias();
        let definition = self.definition(db);

        definition_expression_type(db, definition, &type_alias_stmt_node.node(&module).value)
    }

    fn apply_function_specialization(self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        if let Some(generic_context) = self.generic_context(db) {
            let specialization = self
                .specialization(db)
                .unwrap_or_else(|| generic_context.default_specialization(db, None));
            ty.apply_specialization(db, specialization)
        } else {
            ty
        }
    }

    pub(crate) fn apply_specialization(
        self,
        db: &'db dyn Db,
        f: impl FnOnce(GenericContext<'db>) -> Specialization<'db>,
    ) -> PEP695TypeAliasType<'db> {
        match self.generic_context(db) {
            None => self,

            Some(generic_context) => {
                // Note that at runtime, a specialized type alias is an instance of `typing.GenericAlias`.
                // However, the `GenericAlias` type in ty is heavily special cased to refer to specialized
                // class literals, so we instead represent specialized type aliases as instances of
                // `typing.TypeAliasType` internally, and pass the specialization through to the value type,
                // except when resolving to an instance of the type alias, or its display representation.
                let specialization = f(generic_context);
                PEP695TypeAliasType::new(
                    db,
                    self.name(db),
                    self.rhs_scope(db),
                    Some(specialization),
                )
            }
        }
    }

    pub(crate) fn is_specialized(self, db: &'db dyn Db) -> bool {
        self.specialization(db).is_some()
    }

    #[salsa::tracked(cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        let scope = self.rhs_scope(db);
        let file = scope.file(db);
        let parsed = parsed_module(db, file).load(db);
        let type_alias_stmt_node = scope.node(db).expect_type_alias();

        type_alias_stmt_node
            .node(&parsed)
            .type_params
            .as_ref()
            .map(|type_params| {
                let index = semantic_index(db, scope.file(db));
                let definition = index.expect_single_definition(type_alias_stmt_node);
                GenericContext::from_type_params(db, index, definition, type_params)
            })
    }
}

/// A PEP 695 `types.TypeAliasType` created by manually calling the constructor.
///
/// The value type is computed lazily via [`ManualPEP695TypeAliasType::value_type()`]
/// to avoid cycle non-convergence for mutually recursive definitions.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct ManualPEP695TypeAliasType<'db> {
    #[returns(ref)]
    pub name: Name,
    pub definition: Definition<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ManualPEP695TypeAliasType<'_> {}

pub(super) fn walk_manual_pep_695_type_alias<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    type_alias: ManualPEP695TypeAliasType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, type_alias.value_type(db));
}

#[salsa::tracked]
impl<'db> ManualPEP695TypeAliasType<'db> {
    /// The value type of this manual type alias.
    ///
    /// Computed lazily from the definition to avoid including the value in the interned
    /// struct's identity. Returns `Divergent` if the type alias is defined cyclically.
    #[salsa::tracked(
        cycle_initial=|_, id, _| Type::divergent(id),
        cycle_fn=|db, cycle, previous: &Type<'db>, value: Type<'db>, _| {
            value.cycle_normalized(db, *previous, cycle)
        },
        heap_size=ruff_memory_usage::heap_size
    )]
    pub(crate) fn value_type(self, db: &'db dyn Db) -> Type<'db> {
        let definition = self.definition(db);
        let file = definition.file(db);
        let module = parsed_module(db, file).load(db);
        let DefinitionKind::Assignment(assignment) = definition.kind(db) else {
            return Type::unknown();
        };
        let value_node = assignment.value(&module);
        let ast::Expr::Call(call) = value_node else {
            return Type::unknown();
        };
        // The value is the second positional argument to TypeAliasType(name, value).
        let Some(value_arg) = call.arguments.find_argument_value("value", 1) else {
            return Type::unknown();
        };
        definition_expression_type(db, definition, value_arg)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub enum TypeAliasType<'db> {
    /// A type alias defined using the PEP 695 `type` statement.
    PEP695(PEP695TypeAliasType<'db>),
    /// A type alias defined by manually instantiating the PEP 695 `types.TypeAliasType`.
    ManualPEP695(ManualPEP695TypeAliasType<'db>),
}

pub(super) fn walk_type_alias_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    type_alias: TypeAliasType<'db>,
    visitor: &V,
) {
    if !visitor.should_visit_lazy_type_attributes() {
        return;
    }
    match type_alias {
        TypeAliasType::PEP695(type_alias) => {
            walk_pep_695_type_alias(db, type_alias, visitor);
        }
        TypeAliasType::ManualPEP695(type_alias) => {
            walk_manual_pep_695_type_alias(db, type_alias, visitor);
        }
    }
}

impl<'db> TypeAliasType<'db> {
    pub(crate) fn name(self, db: &'db dyn Db) -> &'db str {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.name(db),
            TypeAliasType::ManualPEP695(type_alias) => type_alias.name(db),
        }
    }

    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.definition(db),
            TypeAliasType::ManualPEP695(type_alias) => type_alias.definition(db),
        }
    }

    pub fn value_type(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.value_type(db),
            TypeAliasType::ManualPEP695(type_alias) => type_alias.value_type(db),
        }
    }

    pub(crate) fn raw_value_type(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.raw_value_type(db),
            TypeAliasType::ManualPEP695(type_alias) => type_alias.value_type(db),
        }
    }

    pub(crate) fn as_pep_695_type_alias(self) -> Option<PEP695TypeAliasType<'db>> {
        match self {
            TypeAliasType::PEP695(type_alias) => Some(type_alias),
            TypeAliasType::ManualPEP695(_) => None,
        }
    }

    pub(crate) fn generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        // TODO: Add support for generic non-PEP695 type aliases.
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.generic_context(db),
            TypeAliasType::ManualPEP695(_) => None,
        }
    }

    pub(crate) fn specialization(self, db: &'db dyn Db) -> Option<Specialization<'db>> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.specialization(db),
            TypeAliasType::ManualPEP695(_) => None,
        }
    }

    pub(super) fn apply_function_specialization(self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.apply_function_specialization(db, ty),
            TypeAliasType::ManualPEP695(_) => ty,
        }
    }

    pub(crate) fn apply_specialization(
        self,
        db: &'db dyn Db,
        f: impl FnOnce(GenericContext<'db>) -> Specialization<'db>,
    ) -> Self {
        match self {
            TypeAliasType::PEP695(type_alias) => {
                TypeAliasType::PEP695(type_alias.apply_specialization(db, f))
            }
            TypeAliasType::ManualPEP695(_) => self,
        }
    }

    /// Returns a struct that can display the fully qualified name of this type alias.
    pub(crate) fn qualified_name(self, db: &'db dyn Db) -> QualifiedTypeAliasName<'db> {
        QualifiedTypeAliasName::from_type_alias(db, self)
    }
}

// N.B. It would be incorrect to derive `Eq`, `PartialEq`, or `Hash` for this struct,
// because two `QualifiedTypeAliasName` instances might refer to different type aliases but
// have the same components. You'd expect them to compare equal, but they'd compare
// unequal if `PartialEq`/`Eq` were naively derived.
#[derive(Clone, Copy)]
pub(crate) struct QualifiedTypeAliasName<'db> {
    db: &'db dyn Db,
    type_alias: TypeAliasType<'db>,
}

impl<'db> QualifiedTypeAliasName<'db> {
    pub(crate) fn from_type_alias(db: &'db dyn Db, type_alias: TypeAliasType<'db>) -> Self {
        Self { db, type_alias }
    }

    /// Returns the components of the qualified name of this type alias, excluding the alias itself.
    ///
    /// For example, calling this method on a type alias `D` inside a class `C` in module `a.b`
    /// would return `["a", "b", "C"]`.
    pub(crate) fn components_excluding_self(&self) -> Vec<String> {
        let definition = self.type_alias.definition(self.db);
        let file = definition.file(self.db);
        let file_scope_id = definition.file_scope(self.db);

        // Type aliases are defined directly in their enclosing scope (no body scope like classes),
        // so we don't skip any ancestor scopes.
        qualified_name_components_from_scope(self.db, file, file_scope_id, 0)
    }
}

impl std::fmt::Display for QualifiedTypeAliasName<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for parent in self.components_excluding_self() {
            f.write_str(&parent)?;
            f.write_char('.')?;
        }
        f.write_str(self.type_alias.name(self.db))
    }
}
