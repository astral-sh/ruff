use std::fmt::Write;

use crate::{
    Db, FxOrderSet,
    types::{
        ApplyTypeMappingVisitor, BoundTypeVarInstance, GenericContext, KnownInstanceType, Type,
        TypeContext, TypeMapping, TypeVarKind, TypeVarVariance, binding_type,
        definition_expression_type,
        display::qualified_name_components_from_scope,
        generics::{ApplySpecialization, Specialization},
        infer_implicit_type_alias_runtime_value_type, infer_implicit_type_alias_value_type,
        variance::VarianceInferable,
        visitor,
    },
};
use ty_python_core::{
    ast_ids::HasScopedUseId,
    definition::{Definition, DefinitionKind, DefinitionState},
    scope::{FileScopeId, ScopeId},
    semantic_index,
};

use ruff_db::parsed::{parsed_module, parsed_string_annotation};
use ruff_db::source::source_text;
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_python_ast::visitor::{self as ast_visitor, Visitor};

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
    pub(crate) fn value_type(self, db: &'db dyn Db) -> Type<'db> {
        self.apply_function_specialization(db, self.raw_value_type(db))
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
    pub(super) fn raw_value_type(self, db: &'db dyn Db) -> Type<'db> {
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
            let type_mapping = match specialization.materialization_kind(db) {
                None => {
                    TypeMapping::ApplySpecialization(ApplySpecialization::TypeAlias(specialization))
                }
                Some(materialization_kind) => TypeMapping::ApplySpecializationWithMaterialization {
                    specialization: ApplySpecialization::TypeAlias(specialization),
                    materialization_kind,
                },
            };

            ty.apply_type_mapping_impl(
                db,
                &type_mapping,
                TypeContext::default(),
                &ApplyTypeMappingVisitor::default(),
            )
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

/// A type alias defined implicitly by assigning a type expression to a name.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct ImplicitTypeAliasType<'db> {
    #[returns(ref)]
    pub name: Name,
    pub definition: Definition<'db>,
    pub(super) specialization: Option<Specialization<'db>>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ImplicitTypeAliasType<'_> {}

pub(super) fn walk_implicit_type_alias<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    type_alias: ImplicitTypeAliasType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, type_alias.value_type(db));
}

struct ImplicitAliasLegacyTypeVarCollector<'a, 'db> {
    db: &'db dyn Db,
    alias_definition: Definition<'db>,
    index: &'a ty_python_core::SemanticIndex<'db>,
    string_annotation_scope: Option<FileScopeId>,
    variables: FxOrderSet<BoundTypeVarInstance<'db>>,
}

impl<'db> ImplicitAliasLegacyTypeVarCollector<'_, 'db> {
    fn collect_name_from_scope(&mut self, name: &str, scope: FileScopeId) {
        for (visible_scope_id, _) in self.index.visible_ancestor_scopes(scope) {
            let place_table = self.index.place_table(visible_scope_id);
            let Some(symbol_id) = place_table.symbol_id(name) else {
                continue;
            };

            let use_def = self.index.use_def_map(visible_scope_id);
            for binding in use_def.end_of_scope_symbol_bindings(symbol_id) {
                self.collect_definition_state(binding.binding);
            }

            return;
        }
    }

    fn collect_definition_state(&mut self, definition_state: DefinitionState<'db>) {
        let Some(definition) = definition_state.definition() else {
            return;
        };
        let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) =
            binding_type(self.db, definition)
        else {
            return;
        };
        if matches!(
            typevar.kind(self.db),
            TypeVarKind::LegacyTypeVar
                | TypeVarKind::LegacyParamSpec
                | TypeVarKind::Pep613Alias
                | TypeVarKind::TypingSelf
        ) {
            self.variables
                .insert(typevar.with_binding_context(self.db, self.alias_definition));
        }
    }
}

impl<'ast> Visitor<'ast> for ImplicitAliasLegacyTypeVarCollector<'_, '_> {
    fn visit_expr(&mut self, expr: &'ast ast::Expr) {
        if let ast::Expr::StringLiteral(string) = expr
            && let Some(string_literal) = string.as_single_part_string()
        {
            let file = self.alias_definition.file(self.db);
            let source = source_text(self.db, file);
            if let Ok(parsed) = parsed_string_annotation(source.as_str(), string_literal) {
                let string_scope = self
                    .string_annotation_scope
                    .unwrap_or_else(|| self.index.expression_scope_id(expr));
                let previous_scope = self.string_annotation_scope.replace(string_scope);
                self.visit_expr(parsed.expr());
                self.string_annotation_scope = previous_scope;
                return;
            }
        }

        if let ast::Expr::Name(name) = expr
            && name.ctx.is_load()
        {
            if let Some(scope) = self.string_annotation_scope {
                self.collect_name_from_scope(&name.id, scope);
            } else {
                let file = self.alias_definition.file(self.db);
                let use_id = name.scoped_use_id(self.db, file);
                let use_def = self.index.use_def_map(self.index.expression_scope_id(expr));

                for binding in use_def.bindings_at_use(use_id) {
                    self.collect_definition_state(binding.binding);
                }
            }
        }

        ast_visitor::walk_expr(self, expr);
    }
}

#[salsa::tracked]
impl<'db> ImplicitTypeAliasType<'db> {
    pub(crate) fn value_type(self, db: &'db dyn Db) -> Type<'db> {
        self.apply_function_specialization(db, self.raw_value_type(db))
    }

    pub(super) fn raw_value_type(self, db: &'db dyn Db) -> Type<'db> {
        infer_implicit_type_alias_value_type(db, self.definition(db))
    }

    pub(crate) fn runtime_value_type(self, db: &'db dyn Db) -> Type<'db> {
        // The runtime alias object represents the original RHS expression; specialization is
        // handled when the alias is used as a type expression.
        infer_implicit_type_alias_runtime_value_type(db, self.definition(db))
    }

    fn apply_function_specialization(self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        if let Some(generic_context) = self.generic_context(db) {
            let specialization = self
                .specialization(db)
                .unwrap_or_else(|| generic_context.default_specialization(db, None));
            let type_mapping = match specialization.materialization_kind(db) {
                None => {
                    TypeMapping::ApplySpecialization(ApplySpecialization::TypeAlias(specialization))
                }
                Some(materialization_kind) => TypeMapping::ApplySpecializationWithMaterialization {
                    specialization: ApplySpecialization::TypeAlias(specialization),
                    materialization_kind,
                },
            };

            ty.apply_type_mapping_impl(
                db,
                &type_mapping,
                TypeContext::default(),
                &ApplyTypeMappingVisitor::default(),
            )
        } else {
            ty
        }
    }

    pub(crate) fn apply_specialization(
        self,
        db: &'db dyn Db,
        f: impl FnOnce(GenericContext<'db>) -> Specialization<'db>,
    ) -> Self {
        match self.generic_context(db) {
            None => self,
            Some(generic_context) => {
                let specialization = f(generic_context);
                Self::new(db, self.name(db), self.definition(db), Some(specialization))
            }
        }
    }

    pub(crate) fn is_specialized(self, db: &'db dyn Db) -> bool {
        self.specialization(db).is_some()
    }

    #[salsa::tracked(cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        let definition = self.definition(db);
        let file = definition.file(db);
        let parsed = parsed_module(db, file).load(db);
        let value = definition.kind(db).value(&parsed)?;
        let index = semantic_index(db, file);
        let mut collector = ImplicitAliasLegacyTypeVarCollector {
            db,
            alias_definition: definition,
            index,
            string_annotation_scope: None,
            variables: FxOrderSet::default(),
        };
        collector.visit_expr(value);

        let mut variables = FxOrderSet::default();
        variables.extend(collector.variables);
        self.raw_value_type(db)
            .find_legacy_typevars(db, Some(definition), &mut variables);
        (!variables.is_empty()).then(|| GenericContext::from_typevar_instances(db, variables))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub enum TypeAliasType<'db> {
    /// A type alias defined using the PEP 695 `type` statement.
    PEP695(PEP695TypeAliasType<'db>),
    /// A type alias defined by manually instantiating the PEP 695 `types.TypeAliasType`.
    ManualPEP695(ManualPEP695TypeAliasType<'db>),
    /// A type alias defined implicitly by assigning a type expression to a name.
    Implicit(ImplicitTypeAliasType<'db>),
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
        TypeAliasType::Implicit(type_alias) => {
            walk_implicit_type_alias(db, type_alias, visitor);
        }
    }
}

impl<'db> TypeAliasType<'db> {
    pub(crate) fn name(self, db: &'db dyn Db) -> &'db str {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.name(db),
            TypeAliasType::ManualPEP695(type_alias) => type_alias.name(db),
            TypeAliasType::Implicit(type_alias) => type_alias.name(db),
        }
    }

    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.definition(db),
            TypeAliasType::ManualPEP695(type_alias) => type_alias.definition(db),
            TypeAliasType::Implicit(type_alias) => type_alias.definition(db),
        }
    }

    pub fn value_type(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.value_type(db),
            TypeAliasType::ManualPEP695(type_alias) => type_alias.value_type(db),
            TypeAliasType::Implicit(type_alias) => type_alias.value_type(db),
        }
    }

    pub(crate) fn raw_value_type(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.raw_value_type(db),
            TypeAliasType::ManualPEP695(type_alias) => type_alias.value_type(db),
            TypeAliasType::Implicit(type_alias) => type_alias.raw_value_type(db),
        }
    }

    pub(crate) fn as_pep_695_type_alias(self) -> Option<PEP695TypeAliasType<'db>> {
        match self {
            TypeAliasType::PEP695(type_alias) => Some(type_alias),
            TypeAliasType::ManualPEP695(_) | TypeAliasType::Implicit(_) => None,
        }
    }

    pub(crate) fn generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.generic_context(db),
            TypeAliasType::ManualPEP695(_) => None,
            TypeAliasType::Implicit(type_alias) => type_alias.generic_context(db),
        }
    }

    pub(crate) fn specialization(self, db: &'db dyn Db) -> Option<Specialization<'db>> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.specialization(db),
            TypeAliasType::ManualPEP695(_) => None,
            TypeAliasType::Implicit(type_alias) => type_alias.specialization(db),
        }
    }

    pub(super) fn apply_function_specialization(self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.apply_function_specialization(db, ty),
            TypeAliasType::ManualPEP695(_) => ty,
            TypeAliasType::Implicit(type_alias) => type_alias.apply_function_specialization(db, ty),
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
            TypeAliasType::Implicit(type_alias) => {
                TypeAliasType::Implicit(type_alias.apply_specialization(db, f))
            }
        }
    }

    pub(crate) fn is_specialized(self, db: &'db dyn Db) -> bool {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.is_specialized(db),
            TypeAliasType::ManualPEP695(_) => false,
            TypeAliasType::Implicit(type_alias) => type_alias.is_specialized(db),
        }
    }

    /// Returns a struct that can display the fully qualified name of this type alias.
    pub(crate) fn qualified_name(self, db: &'db dyn Db) -> QualifiedTypeAliasName<'db> {
        QualifiedTypeAliasName::from_type_alias(db, self)
    }
}

#[salsa::tracked]
impl<'db> VarianceInferable<'db> for TypeAliasType<'db> {
    #[salsa::tracked(
        cycle_initial=|_, _, _, _| TypeVarVariance::Bivariant,
        heap_size=ruff_memory_usage::heap_size
    )]
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance {
        self.value_type(db).variance_of(db, typevar)
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
