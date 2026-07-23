use crate::SemanticContext;
use std::fmt::Write;

use crate::{
    Db, FxOrderSet,
    types::{
        ApplyTypeMappingVisitor, BindingContext, BoundTypeVarIdentity, GenericContext,
        KnownInstanceType, Type, TypeContext, TypeMapping, TypeVarVariance,
        definition_expression_type,
        display::qualified_name_components_from_scope,
        generics::{ApplySpecialization, Specialization, bind_typevar},
        variance::VarianceInferable,
        visitor,
    },
};
use ty_python_core::{
    definition::{Definition, DefinitionKind},
    scope::ScopeId,
    semantic_index,
};

use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast};

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct PEP695TypeAliasType<'db> {
    #[returns(ref)]
    pub name: Name,

    #[returns(copy)]
    rhs_scope: ScopeId<'db>,

    #[returns(copy)]
    pub(super) specialization: Option<Specialization<'db>>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for PEP695TypeAliasType<'_> {}

pub(super) fn walk_pep_695_type_alias<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    ctx: &SemanticContext<'db>,
    type_alias: PEP695TypeAliasType<'db>,
    visitor: &V,
) {
    visitor.visit_type(ctx, type_alias.value_type(ctx));
}

#[salsa::tracked]
impl<'db> PEP695TypeAliasType<'db> {
    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        let scope = self.rhs_scope(db);
        let type_alias_stmt_node = scope.node(db).expect_type_alias();
        semantic_index(db, scope.python_file(db)).expect_single_definition(type_alias_stmt_node)
    }

    /// The RHS type of a PEP-695 style type alias with specialization applied.
    pub(crate) fn value_type(self, ctx: &SemanticContext<'db>) -> Type<'db> {
        let db = ctx.db();
        apply_type_alias_specialization(
            ctx,
            self.raw_value_type(ctx),
            self.generic_context(ctx),
            self.specialization(db),
        )
    }

    /// The RHS type of a PEP-695 style type alias with *no* specialization applied.
    /// Returns `Divergent` if the type alias is defined cyclically.
    pub(super) fn raw_value_type(self, ctx: &SemanticContext<'db>) -> Type<'db> {
        let db = ctx.db();
        debug_assert_eq!(ctx.program(), self.rhs_scope(db).program(db));
        self.raw_value_type_inner(db)
    }

    #[salsa::tracked(
        returns(copy),
        cycle_initial=|_, id, _| Type::divergent(id),
        cycle_fn=|db: &'db dyn Db, cycle, previous: &Type<'db>, value: Type<'db>, alias: PEP695TypeAliasType<'db>| {
            let ctx = SemanticContext::from_file(db, alias.rhs_scope(db).python_file(db));
            value.cycle_normalized(&ctx, *previous, cycle)
        },
        heap_size=ruff_memory_usage::heap_size
    )]
    fn raw_value_type_inner(self, db: &'db dyn Db) -> Type<'db> {
        let scope = self.rhs_scope(db);
        let ctx = SemanticContext::from_file(db, scope.python_file(db));
        let module = parsed_module(db, scope.python_file(db)).load(db);
        let type_alias_stmt_node = scope.node(db).expect_type_alias();
        let definition = self.definition(db);

        definition_expression_type(&ctx, definition, &type_alias_stmt_node.node(&module).value)
    }

    pub(crate) fn apply_specialization(
        self,
        ctx: &SemanticContext<'db>,
        f: impl FnOnce(GenericContext<'db>) -> Specialization<'db>,
    ) -> PEP695TypeAliasType<'db> {
        let db = ctx.db();
        match self.generic_context(ctx) {
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

    pub(crate) fn generic_context(self, ctx: &SemanticContext<'db>) -> Option<GenericContext<'db>> {
        let db = ctx.db();
        debug_assert_eq!(ctx.program(), self.rhs_scope(db).program(db));
        self.generic_context_inner(db)
    }

    #[salsa::tracked(returns(copy), cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
    fn generic_context_inner(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        let scope = self.rhs_scope(db);
        let ctx = SemanticContext::from_file(db, scope.python_file(db));
        let parsed = parsed_module(db, scope.python_file(db)).load(db);
        let type_alias_stmt_node = scope.node(db).expect_type_alias();

        type_alias_stmt_node
            .node(&parsed)
            .type_params
            .as_ref()
            .map(|type_params| {
                let index = semantic_index(db, scope.python_file(db));
                let definition = index.expect_single_definition(type_alias_stmt_node);
                GenericContext::from_type_params(&ctx, index, definition, type_params)
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
    #[returns(copy)]
    pub definition: Definition<'db>,

    #[returns(copy)]
    pub(super) specialization: Option<Specialization<'db>>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ManualPEP695TypeAliasType<'_> {}

pub(super) fn walk_manual_pep_695_type_alias<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    ctx: &SemanticContext<'db>,
    type_alias: ManualPEP695TypeAliasType<'db>,
    visitor: &V,
) {
    visitor.visit_type(ctx, type_alias.value_type(ctx));
}

#[salsa::tracked]
impl<'db> ManualPEP695TypeAliasType<'db> {
    /// The value type of this manual type alias.
    ///
    /// Computed lazily from the definition with specialization applied.
    pub(crate) fn value_type(self, ctx: &SemanticContext<'db>) -> Type<'db> {
        let db = ctx.db();
        apply_type_alias_specialization(
            ctx,
            self.raw_value_type(ctx),
            self.generic_context(ctx),
            self.specialization(db),
        )
    }

    /// The value type of this manual type alias with no specialization applied.
    ///
    /// Computed lazily from the definition to avoid including the value in the interned
    /// struct's identity. Returns `Divergent` if the type alias is defined cyclically.
    pub(crate) fn raw_value_type(self, ctx: &SemanticContext<'db>) -> Type<'db> {
        let db = ctx.db();
        debug_assert_eq!(ctx.program(), self.definition(db).program(db));
        self.raw_value_type_inner(db)
    }

    #[salsa::tracked(
        returns(copy),
        cycle_initial=|_, id, _| Type::divergent(id),
        cycle_fn=|db: &'db dyn Db, cycle, previous: &Type<'db>, value: Type<'db>, alias: ManualPEP695TypeAliasType<'db>| {
            let ctx = SemanticContext::from_file(db, alias.definition(db).python_file(db));
            value.cycle_normalized(&ctx, *previous, cycle)
        },
        heap_size=ruff_memory_usage::heap_size
    )]
    fn raw_value_type_inner(self, db: &'db dyn Db) -> Type<'db> {
        let definition = self.definition(db);
        let module = parsed_module(db, definition.python_file(db)).load(db);
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
        let ctx = SemanticContext::from_file(db, definition.python_file(db));
        definition_expression_type(&ctx, definition, value_arg)
    }

    pub(crate) fn apply_specialization(
        self,
        ctx: &SemanticContext<'db>,
        f: impl FnOnce(GenericContext<'db>) -> Specialization<'db>,
    ) -> Self {
        let db = ctx.db();
        let Some(generic_context) = self.generic_context(ctx) else {
            return self;
        };

        Self::new(
            db,
            self.name(db),
            self.definition(db),
            Some(f(generic_context)),
        )
    }

    pub(crate) fn generic_context(self, ctx: &SemanticContext<'db>) -> Option<GenericContext<'db>> {
        let db = ctx.db();
        debug_assert_eq!(ctx.program(), self.definition(db).program(db));
        self.generic_context_inner(db)
    }

    #[salsa::tracked(returns(copy), cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
    fn generic_context_inner(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        let definition = self.definition(db);
        let file = definition.python_file(db);
        let ctx = SemanticContext::from_file(db, file);
        let module = parsed_module(db, file).load(db);
        let DefinitionKind::Assignment(assignment) = definition.kind(db) else {
            return None;
        };
        let ast::Expr::Call(call) = assignment.value(&module) else {
            return None;
        };
        let type_params = call
            .arguments
            .find_argument_value("type_params", 2)?
            .as_tuple_expr()?;
        let index = semantic_index(db, file);

        let mut variables = FxOrderSet::default();
        for element in &type_params.elts {
            let typevar = match definition_expression_type(&ctx, definition, element) {
                Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) => bind_typevar(
                    db,
                    index,
                    definition.file_scope(db),
                    Some(definition),
                    typevar,
                )?,
                _ => return None,
            };
            if typevar.binding_context(db) != BindingContext::Definition(definition) {
                return None;
            }
            variables.insert(typevar);
        }

        (!variables.is_empty()).then(|| GenericContext::from_typevar_instances(db, variables))
    }
}

fn apply_type_alias_specialization<'db>(
    ctx: &SemanticContext<'db>,
    ty: Type<'db>,
    generic_context: Option<GenericContext<'db>>,
    specialization: Option<Specialization<'db>>,
) -> Type<'db> {
    let db = ctx.db();
    let Some(generic_context) = generic_context else {
        return ty;
    };

    let specialization =
        specialization.unwrap_or_else(|| generic_context.default_specialization(ctx, None));
    let type_mapping = match specialization.materialization_kind(db) {
        None => TypeMapping::ApplySpecialization(ApplySpecialization::TypeAlias(specialization)),
        Some(materialization_kind) => TypeMapping::ApplySpecializationWithMaterialization {
            specialization: ApplySpecialization::TypeAlias(specialization),
            materialization_kind,
        },
    };

    ty.apply_type_mapping_impl(
        ctx,
        &type_mapping,
        TypeContext::default(),
        &ApplyTypeMappingVisitor::default(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub enum TypeAliasType<'db> {
    /// A type alias defined using the PEP 695 `type` statement.
    PEP695(PEP695TypeAliasType<'db>),
    /// A type alias defined by manually instantiating the PEP 695 `types.TypeAliasType`.
    ManualPEP695(ManualPEP695TypeAliasType<'db>),
}

pub(super) fn walk_type_alias_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    ctx: &SemanticContext<'db>,
    type_alias: TypeAliasType<'db>,
    visitor: &V,
) {
    if !visitor.should_visit_lazy_type_attributes() {
        return;
    }
    match type_alias {
        TypeAliasType::PEP695(type_alias) => {
            walk_pep_695_type_alias(ctx, type_alias, visitor);
        }
        TypeAliasType::ManualPEP695(type_alias) => {
            walk_manual_pep_695_type_alias(ctx, type_alias, visitor);
        }
    }
}

#[salsa::tracked]
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

    pub fn value_type(self, ctx: &SemanticContext<'db>) -> Type<'db> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.value_type(ctx),
            TypeAliasType::ManualPEP695(type_alias) => type_alias.value_type(ctx),
        }
    }

    pub(crate) fn raw_value_type(self, ctx: &SemanticContext<'db>) -> Type<'db> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.raw_value_type(ctx),
            TypeAliasType::ManualPEP695(type_alias) => type_alias.raw_value_type(ctx),
        }
    }

    /// Returns the alias without an applied specialization.
    pub(super) fn unspecialized(self, db: &'db dyn Db) -> Self {
        match self {
            TypeAliasType::PEP695(alias) => TypeAliasType::PEP695(PEP695TypeAliasType::new(
                db,
                alias.name(db),
                alias.rhs_scope(db),
                None,
            )),
            TypeAliasType::ManualPEP695(alias) => TypeAliasType::ManualPEP695(
                ManualPEP695TypeAliasType::new(db, alias.name(db), alias.definition(db), None),
            ),
        }
    }

    pub(crate) fn as_pep_695_type_alias(self) -> Option<PEP695TypeAliasType<'db>> {
        match self {
            TypeAliasType::PEP695(type_alias) => Some(type_alias),
            TypeAliasType::ManualPEP695(_) => None,
        }
    }

    pub(crate) fn generic_context(self, ctx: &SemanticContext<'db>) -> Option<GenericContext<'db>> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.generic_context(ctx),
            TypeAliasType::ManualPEP695(type_alias) => type_alias.generic_context(ctx),
        }
    }

    pub(crate) fn specialization(self, db: &'db dyn Db) -> Option<Specialization<'db>> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.specialization(db),
            TypeAliasType::ManualPEP695(type_alias) => type_alias.specialization(db),
        }
    }

    pub(crate) fn apply_specialization(
        self,
        ctx: &SemanticContext<'db>,
        f: impl FnOnce(GenericContext<'db>) -> Specialization<'db>,
    ) -> Self {
        match self {
            TypeAliasType::PEP695(type_alias) => {
                TypeAliasType::PEP695(type_alias.apply_specialization(ctx, f))
            }
            TypeAliasType::ManualPEP695(type_alias) => {
                TypeAliasType::ManualPEP695(type_alias.apply_specialization(ctx, f))
            }
        }
    }

    /// Returns a struct that can display the fully qualified name of this type alias.
    pub(crate) fn qualified_name(self, db: &'db dyn Db) -> QualifiedTypeAliasName<'db> {
        QualifiedTypeAliasName::from_type_alias(db, self)
    }
}

impl<'db> VarianceInferable<'db> for TypeAliasType<'db> {
    fn variance_of(
        self,
        ctx: &SemanticContext<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> TypeVarVariance {
        let db = ctx.db();
        debug_assert_eq!(ctx.program(), self.definition(db).program(db));
        self.variance_of_owner(db, typevar)
    }
}

#[salsa::tracked]
impl<'db> TypeAliasType<'db> {
    #[salsa::tracked(
        returns(copy),
        cycle_initial=|_, _, _, _| TypeVarVariance::Bivariant,
        heap_size=ruff_memory_usage::heap_size
    )]
    fn variance_of_owner(
        self,
        db: &'db dyn Db,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> TypeVarVariance {
        let ctx = SemanticContext::from_file(db, self.definition(db).python_file(db));
        let Some(generic_context) = self.generic_context(&ctx) else {
            return self.value_type(&ctx).variance_of(&ctx, typevar);
        };

        // Infer an alias's own type-parameter variance from the raw RHS. Applying specialization
        // here would recursively request the same `variance_of` query.
        if generic_context
            .variables(db)
            .any(|alias_typevar| alias_typevar.identity(db) == typevar)
        {
            return self.raw_value_type(&ctx).variance_of(&ctx, typevar);
        }

        let raw_value_type = self.raw_value_type(&ctx);
        let specialization = self
            .specialization(db)
            .unwrap_or_else(|| generic_context.default_specialization(&ctx, None));

        // For external typevars, variance flows through the specialization arguments. Expanding
        // the specialized alias body here can create ever-larger recursive alias applications.
        generic_context
            .variables(db)
            .zip(specialization.types(db))
            .map(|(alias_typevar, argument_ty)| {
                raw_value_type
                    .variance_of(&ctx, alias_typevar.identity(db))
                    .compose_thunk(|| argument_ty.variance_of(&ctx, typevar))
            })
            .collect()
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
        let file = definition.python_file(self.db);
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
