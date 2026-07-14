use std::fmt::Write;

use crate::types::ide_support::{
    ImportAliasResolution, ResolvedDefinition, find_symbol_in_scope, resolve_definition,
};
use crate::{
    Db,
    types::{
        ApplyTypeMappingVisitor, BoundTypeVarIdentity, GenericContext, Type, TypeContext,
        TypeMapping, TypeVarVariance, definition_expression_type,
        display::qualified_name_components_from_scope,
        generics::{ApplySpecialization, Specialization},
        variance::VarianceInferable,
        visitor,
    },
};
use rustc_hash::FxHashSet;
use ty_module_resolver::{ModuleName, file_to_module, resolve_module};
use ty_python_core::{
    definition::{Definition, DefinitionKind},
    global_scope,
    scope::{FileScopeId, NodeWithScopeRef, ScopeId},
    semantic_index,
};

use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_python_ast::visitor::{self as ast_visitor, Visitor};
use ruff_python_parser::parse_expression;

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
        returns(copy),
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

    #[salsa::tracked(returns(copy), cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
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
    #[returns(copy)]
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
        returns(copy),
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

    /// Returns whether expanding this type alias can reach its own definition again.
    pub(crate) fn is_recursively_defined(self, db: &'db dyn Db) -> bool {
        let definition = self.definition(db);
        type_alias_definition_reaches_definition(db, definition, definition)
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

/// Returns whether expanding `source` can reach `target`.
///
/// A type alias is recursive only if its own definition is reachable. Reaching a distinct
/// recursive alias is not enough: that alias's recursion guard will handle its own cycle.
/// This query must not inspect inferred types because it can run while alias-value inference is
/// recovering a Salsa cycle.
#[salsa::tracked(
    returns(copy),
    cycle_initial=|_, _, _, _| false,
    heap_size=ruff_memory_usage::heap_size
)]
fn type_alias_definition_reaches_definition<'db>(
    db: &'db dyn Db,
    source: Definition<'db>,
    target: Definition<'db>,
) -> bool {
    let file = source.file(db);
    let module = parsed_module(db, file).load(db);
    let mut visitor = TypeAliasReferenceVisitor {
        db,
        file,
        target,
        string_scope: None,
        resolving_string_annotation_context: FxHashSet::default(),
        resolving_type_alias_type_constructor: FxHashSet::default(),
        found: false,
    };
    let value = match source.kind(db) {
        DefinitionKind::TypeAlias(type_alias) => type_alias.node(&module).value.as_ref(),
        DefinitionKind::Assignment(assignment) => {
            let Some(value) = visitor.manual_type_alias_value(assignment.value(&module)) else {
                return false;
            };
            value
        }
        _ => return false,
    };

    visitor.visit_expr(value);
    visitor.found
}

struct TypeAliasReferenceVisitor<'db> {
    db: &'db dyn Db,
    file: File,
    target: Definition<'db>,
    string_scope: Option<FileScopeId>,
    resolving_string_annotation_context: FxHashSet<Definition<'db>>,
    resolving_type_alias_type_constructor: FxHashSet<Definition<'db>>,
    found: bool,
}

impl<'db> TypeAliasReferenceVisitor<'db> {
    fn scope_for_expr(&self, expr: &ast::Expr) -> FileScopeId {
        self.string_scope.unwrap_or_else(|| {
            let index = semantic_index(self.db, self.file);
            index.expression_scope_id(expr)
        })
    }

    fn scope_for_expr_in_file(&self, file: File, expr: &ast::Expr) -> FileScopeId {
        let index = semantic_index(self.db, file);
        index.expression_scope_id(expr)
    }

    fn is_typing_module(module: &str) -> bool {
        matches!(module, "typing" | "typing_extensions")
    }

    fn known_string_annotation_context(name: &str) -> Option<StringAnnotationContext> {
        match name {
            "Literal" => Some(StringAnnotationContext::Literal),
            "Annotated" => Some(StringAnnotationContext::Annotated),
            _ => None,
        }
    }

    fn import_from_string_annotation_context(
        &mut self,
        definition: Definition<'db>,
        name: &str,
    ) -> Option<StringAnnotationContext> {
        if !self.resolving_string_annotation_context.insert(definition) {
            return None;
        }

        let result = match definition.kind(self.db) {
            DefinitionKind::ImportFrom(import_from) => {
                let module = parsed_module(self.db, definition.file(self.db)).load(self.db);
                let import = import_from.import(&module);
                import.module.as_ref().and_then(|module_name| {
                    Self::is_typing_module(module_name.as_str())
                        .then(|| {
                            Self::known_string_annotation_context(
                                import_from.alias(&module).name.as_str(),
                            )
                        })
                        .flatten()
                })
            }
            DefinitionKind::StarImport(star_import) => {
                let module = parsed_module(self.db, definition.file(self.db)).load(self.db);
                let import = star_import.import(&module);
                import.module.as_ref().and_then(|module_name| {
                    Self::is_typing_module(module_name.as_str())
                        .then(|| Self::known_string_annotation_context(name))
                        .flatten()
                })
            }
            DefinitionKind::Assignment(assignment) => {
                let file = definition.file(self.db);
                let module = parsed_module(self.db, file).load(self.db);
                match self.string_annotation_context_in_file(file, assignment.value(&module)) {
                    StringAnnotationContext::TypeExpression => None,
                    context => Some(context),
                }
            }
            _ => None,
        };

        let result = result.or_else(|| {
            resolve_definition(
                self.db,
                definition,
                Some(name),
                ImportAliasResolution::ResolveAliases,
            )
            .into_iter()
            .filter_map(|resolved| match resolved {
                ResolvedDefinition::Definition(resolved) if resolved != definition => {
                    Some(resolved)
                }
                ResolvedDefinition::Definition(_)
                | ResolvedDefinition::Module(_)
                | ResolvedDefinition::FileWithRange(_) => None,
            })
            .find_map(|resolved| self.import_from_string_annotation_context(resolved, name))
        });
        self.resolving_string_annotation_context.remove(&definition);
        result
    }

    fn import_is_type_alias_type(&mut self, definition: Definition<'db>, name: &str) -> bool {
        if !self
            .resolving_type_alias_type_constructor
            .insert(definition)
        {
            return false;
        }

        let result = match definition.kind(self.db) {
            DefinitionKind::ImportFrom(import_from) => {
                let module = parsed_module(self.db, definition.file(self.db)).load(self.db);
                let import = import_from.import(&module);
                Self::is_typing_module(import.module.as_ref().map_or("", |module| module.as_str()))
                    && import_from.alias(&module).name.as_str() == "TypeAliasType"
            }
            DefinitionKind::StarImport(star_import) => {
                let module = parsed_module(self.db, definition.file(self.db)).load(self.db);
                let import = star_import.import(&module);
                Self::is_typing_module(import.module.as_ref().map_or("", |module| module.as_str()))
                    && name == "TypeAliasType"
            }
            DefinitionKind::Assignment(assignment) => {
                let file = definition.file(self.db);
                let module = parsed_module(self.db, file).load(self.db);
                self.is_type_alias_type_constructor_in_file(file, assignment.value(&module))
            }
            _ => false,
        };

        let result = result
            || resolve_definition(
                self.db,
                definition,
                Some(name),
                ImportAliasResolution::ResolveAliases,
            )
            .into_iter()
            .filter_map(|resolved| match resolved {
                ResolvedDefinition::Definition(resolved) if resolved != definition => {
                    Some(resolved)
                }
                ResolvedDefinition::Definition(_)
                | ResolvedDefinition::Module(_)
                | ResolvedDefinition::FileWithRange(_) => None,
            })
            .any(|resolved| self.import_is_type_alias_type(resolved, name));
        self.resolving_type_alias_type_constructor
            .remove(&definition);
        result
    }

    fn name_string_annotation_context(
        &mut self,
        file: File,
        scope: FileScopeId,
        name: &str,
    ) -> StringAnnotationContext {
        let index = semantic_index(self.db, file);
        for (scope, _) in index.visible_ancestor_scopes(scope) {
            let place_table = index.place_table(scope);
            let Some(symbol) = place_table.symbol_id(name) else {
                continue;
            };

            let mut bindings = index
                .use_def_map(scope)
                .end_of_scope_symbol_bindings(symbol)
                .filter_map(|binding| binding.binding.definition())
                .peekable();
            if bindings.peek().is_some() {
                return bindings
                    .find_map(|definition| {
                        self.import_from_string_annotation_context(definition, name)
                    })
                    .unwrap_or(StringAnnotationContext::TypeExpression);
            }
        }

        StringAnnotationContext::TypeExpression
    }

    fn name_is_typing_module(&self, file: File, scope: FileScopeId, name: &str) -> bool {
        let index = semantic_index(self.db, file);
        for (scope, _) in index.visible_ancestor_scopes(scope) {
            let place_table = index.place_table(scope);
            let Some(symbol) = place_table.symbol_id(name) else {
                continue;
            };

            let mut bindings = index
                .use_def_map(scope)
                .end_of_scope_symbol_bindings(symbol)
                .filter_map(|binding| binding.binding.definition())
                .peekable();
            if bindings.peek().is_some() {
                return bindings.any(|definition| {
                    let DefinitionKind::Import(import) = definition.kind(self.db) else {
                        return false;
                    };
                    let module = parsed_module(self.db, definition.file(self.db)).load(self.db);
                    Self::is_typing_module(import.alias(&module).name.as_str())
                });
            }
        }

        false
    }

    fn name_is_type_alias_type(&mut self, file: File, scope: FileScopeId, name: &str) -> bool {
        let index = semantic_index(self.db, file);
        for (scope, _) in index.visible_ancestor_scopes(scope) {
            let place_table = index.place_table(scope);
            let Some(symbol) = place_table.symbol_id(name) else {
                continue;
            };

            let mut bindings = index
                .use_def_map(scope)
                .end_of_scope_symbol_bindings(symbol)
                .filter_map(|binding| binding.binding.definition())
                .peekable();
            if bindings.peek().is_some() {
                return bindings.any(|definition| self.import_is_type_alias_type(definition, name));
            }
        }

        false
    }

    fn string_annotation_context_in_scope(
        &mut self,
        file: File,
        scope: FileScopeId,
        expr: &ast::Expr,
    ) -> StringAnnotationContext {
        if let Some(name) = expr.as_name_expr() {
            return self.name_string_annotation_context(file, scope, name.id.as_str());
        }

        let Some(attribute) = expr.as_attribute_expr() else {
            return StringAnnotationContext::TypeExpression;
        };
        let Some(base_name) = attribute.value.as_name_expr() else {
            return StringAnnotationContext::TypeExpression;
        };
        if self.name_is_typing_module(file, scope, base_name.id.as_str()) {
            return Self::known_string_annotation_context(attribute.attr.as_str())
                .unwrap_or(StringAnnotationContext::TypeExpression);
        }

        StringAnnotationContext::TypeExpression
    }

    fn string_annotation_context_in_file(
        &mut self,
        file: File,
        expr: &ast::Expr,
    ) -> StringAnnotationContext {
        self.string_annotation_context_in_scope(file, self.scope_for_expr_in_file(file, expr), expr)
    }

    fn string_annotation_context(&mut self, expr: &ast::Expr) -> StringAnnotationContext {
        self.string_annotation_context_in_scope(self.file, self.scope_for_expr(expr), expr)
    }

    fn is_type_alias_type_constructor_in_scope(
        &mut self,
        file: File,
        scope: FileScopeId,
        expr: &ast::Expr,
    ) -> bool {
        if let Some(name) = expr.as_name_expr() {
            return self.name_is_type_alias_type(file, scope, name.id.as_str());
        }

        let Some(attribute) = expr.as_attribute_expr() else {
            return false;
        };
        attribute.attr.as_str() == "TypeAliasType"
            && attribute
                .value
                .as_name_expr()
                .is_some_and(|module| self.name_is_typing_module(file, scope, module.id.as_str()))
    }

    fn is_type_alias_type_constructor_in_file(&mut self, file: File, expr: &ast::Expr) -> bool {
        self.is_type_alias_type_constructor_in_scope(
            file,
            self.scope_for_expr_in_file(file, expr),
            expr,
        )
    }

    fn is_type_alias_type_constructor(&mut self, expr: &ast::Expr) -> bool {
        self.is_type_alias_type_constructor_in_scope(self.file, self.scope_for_expr(expr), expr)
    }

    fn manual_type_alias_value<'ast>(&mut self, value: &'ast ast::Expr) -> Option<&'ast ast::Expr> {
        let ast::Expr::Call(call) = value else {
            return None;
        };
        if !self.is_type_alias_type_constructor(&call.func) {
            return None;
        }
        call.arguments.find_argument_value("value", 1)
    }

    fn definition_reaches_target(&self, definition: Definition<'db>, name: &str) -> bool {
        if definition == self.target {
            return true;
        }

        match definition.kind(self.db) {
            DefinitionKind::TypeAlias(_) | DefinitionKind::Assignment(_) => {
                type_alias_definition_reaches_definition(self.db, definition, self.target)
            }
            DefinitionKind::ImportFrom(_) | DefinitionKind::StarImport(_) => resolve_definition(
                self.db,
                definition,
                Some(name),
                ImportAliasResolution::ResolveAliases,
            )
            .into_iter()
            .filter_map(|resolved| match resolved {
                ResolvedDefinition::Definition(resolved) if resolved != definition => {
                    Some(resolved)
                }
                ResolvedDefinition::Definition(_)
                | ResolvedDefinition::Module(_)
                | ResolvedDefinition::FileWithRange(_) => None,
            })
            .any(|resolved| self.definition_reaches_target(resolved, name)),
            _ => false,
        }
    }

    fn visit_name(&mut self, expr: &ast::Expr, name: &ast::ExprName) {
        self.visit_name_in_scope(self.scope_for_expr(expr), name.id.as_str());
    }

    fn definition_module_attribute_reaches_target(
        &self,
        definition: Definition<'db>,
        module_name: &str,
        attributes: &[&str],
    ) -> bool {
        let attributes = match definition.kind(self.db) {
            DefinitionKind::Import(import) => {
                let module = parsed_module(self.db, definition.file(self.db)).load(self.db);
                let alias = import.alias(&module);
                let module_prefix_length = if alias.asname.is_none() {
                    let import_path: Vec<_> = alias.name.as_str().split('.').collect();
                    let module_prefix = &import_path[1..];
                    if attributes.len() < module_prefix.len()
                        || !attributes
                            .iter()
                            .zip(module_prefix)
                            .all(|(attribute, component)| attribute == component)
                    {
                        return false;
                    }
                    module_prefix.len()
                } else {
                    0
                };
                &attributes[module_prefix_length..]
            }
            _ => attributes,
        };

        if let DefinitionKind::Class(class) = definition.kind(self.db) {
            let Some((attribute_name, remaining)) = attributes.split_first() else {
                return false;
            };
            let file = definition.file(self.db);
            let module = parsed_module(self.db, file).load(self.db);
            let class_scope = semantic_index(self.db, file)
                .node_scope(NodeWithScopeRef::Class(class.node(&module)))
                .to_scope_id(self.db, file);

            return find_symbol_in_scope(self.db, class_scope, attribute_name)
                .into_iter()
                .any(|definition| {
                    if remaining.is_empty() {
                        self.definition_reaches_target(definition, attribute_name)
                    } else {
                        self.definition_module_attribute_reaches_target(
                            definition,
                            attribute_name,
                            remaining,
                        )
                    }
                });
        }

        resolve_definition(
            self.db,
            definition,
            Some(module_name),
            ImportAliasResolution::ResolveAliases,
        )
        .into_iter()
        .filter_map(|resolved| match resolved {
            ResolvedDefinition::Module(file) => Some(file),
            ResolvedDefinition::Definition(_) | ResolvedDefinition::FileWithRange(_) => None,
        })
        .any(|file| self.module_file_attribute_reaches_target(file, attributes))
    }

    fn module_file_attribute_reaches_target(&self, file: File, attributes: &[&str]) -> bool {
        let Some((attribute_name, remaining)) = attributes.split_first() else {
            return false;
        };

        if let Some(module) = file_to_module(self.db, file)
            && let Some(component) = ModuleName::new(attribute_name)
        {
            let mut submodule_name = module.name(self.db).clone();
            submodule_name.extend(&component);
            if semantic_index(self.db, self.file)
                .imported_modules()
                .any(|imported| imported == &submodule_name)
                && let Some(submodule) = resolve_module(self.db, self.file, &submodule_name)
                && let Some(submodule_file) = submodule.file(self.db)
            {
                return self.module_file_attribute_reaches_target(submodule_file, remaining);
            }
        }

        find_symbol_in_scope(self.db, global_scope(self.db, file), attribute_name)
            .into_iter()
            .any(|definition| {
                if remaining.is_empty() {
                    self.definition_reaches_target(definition, attribute_name)
                } else {
                    self.definition_module_attribute_reaches_target(
                        definition,
                        attribute_name,
                        remaining,
                    )
                }
            })
    }

    fn module_attribute_reaches_target(
        &self,
        scope: FileScopeId,
        module_name: &str,
        attributes: &[&str],
    ) -> bool {
        let index = semantic_index(self.db, self.file);
        for (scope, _) in index.visible_ancestor_scopes(scope) {
            let place_table = index.place_table(scope);
            let Some(symbol) = place_table.symbol_id(module_name) else {
                continue;
            };

            let mut bindings = index
                .use_def_map(scope)
                .end_of_scope_symbol_bindings(symbol)
                .filter_map(|binding| binding.binding.definition())
                .peekable();
            if bindings.peek().is_some() {
                return bindings.any(|definition| {
                    self.definition_module_attribute_reaches_target(
                        definition,
                        module_name,
                        attributes,
                    )
                });
            }
        }

        false
    }

    fn nested_module_attribute_reaches_target(
        &self,
        scope: FileScopeId,
        attribute: &ast::ExprAttribute,
    ) -> bool {
        let mut attributes = Vec::new();
        let mut current = attribute;
        loop {
            attributes.push(current.attr.as_str());
            match current.value.as_ref() {
                ast::Expr::Attribute(attribute) => current = attribute,
                ast::Expr::Name(module) => {
                    attributes.reverse();
                    return self.module_attribute_reaches_target(
                        scope,
                        module.id.as_str(),
                        &attributes,
                    );
                }
                _ => return false,
            }
        }
    }

    fn visit_name_in_scope(&mut self, scope: FileScopeId, name: &str) {
        let index = semantic_index(self.db, self.file);
        for (scope, _) in index.visible_ancestor_scopes(scope) {
            let place_table = index.place_table(scope);
            let Some(symbol) = place_table.symbol_id(name) else {
                continue;
            };

            let mut bindings = index
                .use_def_map(scope)
                .end_of_scope_symbol_bindings(symbol)
                .filter_map(|binding| binding.binding.definition())
                .peekable();
            if bindings.peek().is_some() {
                self.found =
                    bindings.any(|definition| self.definition_reaches_target(definition, name));
                return;
            }
        }
    }

    fn visit_string_annotation(&mut self, string: &ast::ExprStringLiteral, scope: FileScopeId) {
        let Ok(parsed) = parse_expression(string.value.to_str()) else {
            return;
        };

        let previous_scope = self.string_scope.replace(scope);
        self.visit_expr(parsed.expr());
        self.string_scope = previous_scope;
    }

    fn visit_annotated_slice(&mut self, slice: &ast::Expr) {
        let ast::Expr::Tuple(tuple) = slice else {
            self.visit_expr(slice);
            return;
        };

        if let Some(first) = tuple.elts.first() {
            self.visit_expr(first);
        }
    }

    fn visit_subscript(&mut self, subscript: &ast::ExprSubscript) {
        self.visit_expr(&subscript.value);
        if self.found {
            return;
        }

        match self.string_annotation_context(&subscript.value) {
            StringAnnotationContext::Literal => {}
            StringAnnotationContext::Annotated => self.visit_annotated_slice(&subscript.slice),
            StringAnnotationContext::TypeExpression => self.visit_expr(&subscript.slice),
        }
    }
}

#[derive(Copy, Clone)]
enum StringAnnotationContext {
    TypeExpression,
    Literal,
    Annotated,
}

impl<'ast> Visitor<'ast> for TypeAliasReferenceVisitor<'_> {
    fn visit_expr(&mut self, expr: &'ast ast::Expr) {
        if self.found {
            return;
        }

        if let ast::Expr::Name(name) = expr {
            self.visit_name(expr, name);
            return;
        }

        if let ast::Expr::StringLiteral(string) = expr {
            self.visit_string_annotation(string, self.scope_for_expr(expr));
            return;
        }

        if let ast::Expr::Subscript(subscript) = expr {
            self.visit_subscript(subscript);
            return;
        }

        if let ast::Expr::Attribute(attribute) = expr {
            if self.nested_module_attribute_reaches_target(self.scope_for_expr(expr), attribute) {
                self.found = true;
                return;
            }
        }

        ast_visitor::walk_expr(self, expr);
    }
}

#[salsa::tracked]
impl<'db> VarianceInferable<'db> for TypeAliasType<'db> {
    #[salsa::tracked(
        returns(copy),
        cycle_initial=|_, _, _, _| TypeVarVariance::Bivariant,
        heap_size=ruff_memory_usage::heap_size
    )]
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarIdentity<'db>) -> TypeVarVariance {
        let Some(generic_context) = self.generic_context(db) else {
            return self.value_type(db).variance_of(db, typevar);
        };

        // Infer an alias's own type-parameter variance from the raw RHS. Applying specialization
        // here would recursively request the same `variance_of` query.
        if generic_context
            .variables(db)
            .any(|alias_typevar| alias_typevar.identity(db) == typevar)
        {
            return self.raw_value_type(db).variance_of(db, typevar);
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
                    .variance_of(db, alias_typevar.identity(db))
                    .compose_thunk(|| argument_ty.variance_of(db, typevar))
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
