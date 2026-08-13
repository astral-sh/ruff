use crate::ProgramEnvironment;
use std::fmt::Write;

use crate::{
    Db, FxOrderSet,
    types::{
        ApplyTypeMappingVisitor, BindingContext, BoundTypeVarIdentity, GenericContext,
        KnownInstanceType, MaterializationKind, Type, TypeContext, TypeMapping, TypeVarVariance,
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

    /// Keeps recursive references stable while their alias body is materialized lazily.
    #[returns(copy)]
    pub(super) materialization_kind: Option<MaterializationKind>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for PEP695TypeAliasType<'_> {}

pub(super) fn walk_pep_695_type_alias<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    type_alias: PEP695TypeAliasType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, TypeAliasType::PEP695(type_alias).value_type(db));
}

#[salsa::tracked]
impl<'db> PEP695TypeAliasType<'db> {
    fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        let scope = self.rhs_scope(db);
        let type_alias_stmt_node = scope.node(db).expect_type_alias();
        semantic_index(db, scope.program_file(db)).expect_single_definition(type_alias_stmt_node)
    }

    /// The RHS type of a PEP-695 style type alias with specialization applied.
    fn value_type(self, db: &'db dyn Db) -> Type<'db> {
        apply_type_alias_specialization(
            db,
            self.raw_value_type(db),
            self.generic_context(db),
            self.specialization(db),
        )
    }

    /// The RHS type of a PEP-695 style type alias with *no* specialization applied.
    /// Returns `Divergent` if the type alias is defined cyclically.
    #[salsa::tracked(
        returns(copy),
        cycle_initial=|_, id, _| Type::divergent(id),
        cycle_fn=|db: &'db dyn Db, cycle, previous: &Type<'db>, value: Type<'db>, alias: PEP695TypeAliasType<'db>| {
            let env = ProgramEnvironment::from_scope(alias.rhs_scope(db));
            value.cycle_normalized(db, &env, *previous, cycle)
        },
        heap_size=ruff_memory_usage::heap_size
    )]
    pub(super) fn raw_value_type(self, db: &'db dyn Db) -> Type<'db> {
        let scope = self.rhs_scope(db);
        let program_file = scope.program_file(db);
        let python_file = program_file.python_file(db);
        let module = parsed_module(db, python_file).load(db);
        let type_alias_stmt_node = scope.node(db).expect_type_alias();
        let definition = self.definition(db);

        definition_expression_type(db, definition, &type_alias_stmt_node.node(&module).value)
    }

    fn apply_specialization(
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
                    self.materialization_kind(db),
                )
            }
        }
    }

    #[salsa::tracked(returns(copy), cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        let scope = self.rhs_scope(db);
        let program_file = scope.program_file(db);
        let python_file = program_file.python_file(db);
        let parsed = parsed_module(db, python_file).load(db);
        let type_alias_stmt_node = scope.node(db).expect_type_alias();

        type_alias_stmt_node
            .node(&parsed)
            .type_params
            .as_ref()
            .map(|type_params| {
                let index = semantic_index(db, program_file);
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
    #[returns(copy)]
    pub definition: Definition<'db>,

    #[returns(copy)]
    pub(super) specialization: Option<Specialization<'db>>,

    /// Keeps recursive references stable while their alias body is materialized lazily.
    #[returns(copy)]
    pub(super) materialization_kind: Option<MaterializationKind>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ManualPEP695TypeAliasType<'_> {}

pub(super) fn walk_manual_pep_695_type_alias<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    type_alias: ManualPEP695TypeAliasType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, TypeAliasType::ManualPEP695(type_alias).value_type(db));
}

#[salsa::tracked]
impl<'db> ManualPEP695TypeAliasType<'db> {
    /// The value type of this manual type alias.
    ///
    /// Computed lazily from the definition with specialization applied.
    fn value_type(self, db: &'db dyn Db) -> Type<'db> {
        apply_type_alias_specialization(
            db,
            self.raw_value_type(db),
            self.generic_context(db),
            self.specialization(db),
        )
    }

    /// The value type of this manual type alias with no specialization applied.
    ///
    /// Computed lazily from the definition to avoid including the value in the interned
    /// struct's identity. Returns `Divergent` if the type alias is defined cyclically.
    #[salsa::tracked(
        returns(copy),
        cycle_initial=|_, id, _| Type::divergent(id),
        cycle_fn=|db: &'db dyn Db, cycle, previous: &Type<'db>, value: Type<'db>, alias: ManualPEP695TypeAliasType<'db>| {
            let env = ProgramEnvironment::from_definition(alias.definition(db));
            value.cycle_normalized(db, &env, *previous, cycle)
        },
        heap_size=ruff_memory_usage::heap_size
    )]
    pub(crate) fn raw_value_type(self, db: &'db dyn Db) -> Type<'db> {
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
        definition_expression_type(db, definition, value_arg)
    }

    fn apply_specialization(
        self,
        db: &'db dyn Db,
        f: impl FnOnce(GenericContext<'db>) -> Specialization<'db>,
    ) -> Self {
        let Some(generic_context) = self.generic_context(db) else {
            return self;
        };

        Self::new(
            db,
            self.name(db),
            self.definition(db),
            Some(f(generic_context)),
            self.materialization_kind(db),
        )
    }

    #[salsa::tracked(returns(copy), cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        let definition = self.definition(db);
        let file = definition.program_file(db);
        let env = ProgramEnvironment::from_file(file);
        let module = parsed_module(db, file.python_file(db)).load(db);
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
            let typevar = match definition_expression_type(db, definition, element) {
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

        (!variables.is_empty()).then(|| GenericContext::from_typevar_instances(db, &env, variables))
    }
}

fn apply_type_alias_specialization<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    generic_context: Option<GenericContext<'db>>,
    specialization: Option<Specialization<'db>>,
) -> Type<'db> {
    let Some(generic_context) = generic_context else {
        return ty;
    };

    let env = ProgramEnvironment::from_program(generic_context.program(db));
    let specialization =
        specialization.unwrap_or_else(|| generic_context.default_specialization(db, None));
    let type_mapping = match specialization.materialization_kind(db) {
        None => TypeMapping::ApplySpecialization(ApplySpecialization::TypeAlias(specialization)),
        Some(materialization_kind) => TypeMapping::ApplySpecializationWithMaterialization {
            specialization: ApplySpecialization::TypeAlias(specialization),
            materialization_kind,
        },
    };

    ty.apply_type_mapping_impl(
        db,
        &type_mapping,
        TypeContext::default(),
        &ApplyTypeMappingVisitor::new(&env),
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
    db: &'db dyn Db,
    type_alias: TypeAliasType<'db>,
    visitor: &V,
) {
    if !visitor.should_visit_lazy_type_attributes() {
        visitor.notify_skipped_lazy_type_attributes();
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

    pub fn value_type(self, db: &'db dyn Db) -> Type<'db> {
        if let Some(materialization_kind) = self.materialization_kind(db) {
            return self.materialized_value_type(db, materialization_kind);
        }

        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.value_type(db),
            TypeAliasType::ManualPEP695(type_alias) => type_alias.value_type(db),
        }
    }

    /// Materialize the alias body lazily, keeping this alias as the recursive fallback.
    ///
    /// Comparing a recursive specialization with its materialization can request this same body
    /// before it has finished materializing. Returning the already-marked alias closes that cycle
    /// without losing its materialization polarity.
    #[salsa::tracked(
        returns(copy),
        cycle_initial=|_, _, alias: TypeAliasType<'db>, _| Type::TypeAlias(alias),
        heap_size=ruff_memory_usage::heap_size
    )]
    fn materialized_value_type(
        self,
        db: &'db dyn Db,
        materialization_kind: MaterializationKind,
    ) -> Type<'db> {
        let value_type = self.with_materialization_kind(db, None).value_type(db);
        let env = ProgramEnvironment::from_definition(self.definition(db));
        value_type.materialize(
            db,
            materialization_kind,
            &ApplyTypeMappingVisitor::new(&env),
        )
    }

    pub(crate) fn raw_value_type(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.raw_value_type(db),
            TypeAliasType::ManualPEP695(type_alias) => type_alias.raw_value_type(db),
        }
    }

    /// Returns the alias without an applied specialization or pending materialization.
    pub(super) fn unspecialized(self, db: &'db dyn Db) -> Self {
        match self {
            TypeAliasType::PEP695(alias) => TypeAliasType::PEP695(PEP695TypeAliasType::new(
                db,
                alias.name(db),
                alias.rhs_scope(db),
                None,
                None,
            )),
            TypeAliasType::ManualPEP695(alias) => {
                TypeAliasType::ManualPEP695(ManualPEP695TypeAliasType::new(
                    db,
                    alias.name(db),
                    alias.definition(db),
                    None,
                    None,
                ))
            }
        }
    }

    pub(super) fn materialization_kind(self, db: &'db dyn Db) -> Option<MaterializationKind> {
        match self {
            TypeAliasType::PEP695(alias) => alias.materialization_kind(db),
            TypeAliasType::ManualPEP695(alias) => alias.materialization_kind(db),
        }
    }

    pub(super) fn with_materialization_kind(
        self,
        db: &'db dyn Db,
        materialization_kind: Option<MaterializationKind>,
    ) -> Self {
        if self.materialization_kind(db) == materialization_kind {
            return self;
        }

        match self {
            TypeAliasType::PEP695(alias) => TypeAliasType::PEP695(PEP695TypeAliasType::new(
                db,
                alias.name(db),
                alias.rhs_scope(db),
                alias.specialization(db),
                materialization_kind,
            )),
            TypeAliasType::ManualPEP695(alias) => {
                TypeAliasType::ManualPEP695(ManualPEP695TypeAliasType::new(
                    db,
                    alias.name(db),
                    alias.definition(db),
                    alias.specialization(db),
                    materialization_kind,
                ))
            }
        }
    }

    pub(crate) fn as_pep_695_type_alias(self) -> Option<PEP695TypeAliasType<'db>> {
        match self {
            TypeAliasType::PEP695(type_alias) => Some(type_alias),
            TypeAliasType::ManualPEP695(_) => None,
        }
    }

    pub(crate) fn generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.generic_context(db),
            TypeAliasType::ManualPEP695(type_alias) => type_alias.generic_context(db),
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
        db: &'db dyn Db,
        f: impl FnOnce(GenericContext<'db>) -> Specialization<'db>,
    ) -> Self {
        match self {
            TypeAliasType::PEP695(type_alias) => {
                TypeAliasType::PEP695(type_alias.apply_specialization(db, f))
            }
            TypeAliasType::ManualPEP695(type_alias) => {
                TypeAliasType::ManualPEP695(type_alias.apply_specialization(db, f))
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
        db: &'db dyn Db,
        _: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> TypeVarVariance {
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
        let env = ProgramEnvironment::from_definition(self.definition(db));
        let Some(generic_context) = self.generic_context(db) else {
            return self.value_type(db).variance_of(db, &env, typevar);
        };

        // Infer an alias's own type-parameter variance from the raw RHS. Applying specialization
        // here would recursively request the same `variance_of` query.
        if generic_context
            .variables(db)
            .any(|alias_typevar| alias_typevar.identity(db) == typevar)
        {
            return self.raw_value_type(db).variance_of(db, &env, typevar);
        }

        let raw_value_type = self.raw_value_type(db);
        let specialization = self
            .specialization(db)
            .unwrap_or_else(|| generic_context.default_specialization(db, None));

        // For external typevars, variance flows through the specialization arguments. Expanding
        // the specialized alias body here can create ever-larger recursive alias applications.
        generic_context
            .variables(db)
            .zip(specialization.types(db))
            .map(|(alias_typevar, argument_ty)| {
                raw_value_type
                    .variance_of(db, &env, alias_typevar.identity(db))
                    .compose_thunk(|| argument_ty.variance_of(db, &env, typevar))
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
    fn from_type_alias(db: &'db dyn Db, type_alias: TypeAliasType<'db>) -> Self {
        Self { db, type_alias }
    }

    /// Returns the components of the qualified name of this type alias, excluding the alias itself.
    ///
    /// For example, calling this method on a type alias `D` inside a class `C` in module `a.b`
    /// would return `["a", "b", "C"]`.
    pub(crate) fn components_excluding_self(&self) -> Vec<String> {
        let definition = self.type_alias.definition(self.db);
        let file = definition.program_file(self.db);
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
