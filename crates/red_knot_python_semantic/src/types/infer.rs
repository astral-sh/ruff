//! We have Salsa queries for inferring types at three different granularities: scope-level,
//! definition-level, and expression-level.
//!
//! Scope-level inference is for when we are actually checking a file, and need to check types for
//! everything in that file's scopes, or give a linter access to types of arbitrary expressions
//! (via the [`HasTy`](crate::semantic_model::HasTy) trait).
//!
//! Definition-level inference allows us to look up the types of symbols in other scopes (e.g. for
//! imports) with the minimum inference necessary, so that if we're looking up one symbol from a
//! very large module, we can avoid a bunch of unnecessary work. Definition-level inference also
//! allows us to handle import cycles without getting into a cycle of scope-level inference
//! queries.
//!
//! The expression-level inference query is needed in only a few cases. Since some assignments can
//! have multiple targets (via `x = y = z` or unpacking `(x, y) = z`, they can be associated with
//! multiple definitions (one per assigned symbol). In order to avoid inferring the type of the
//! right-hand side once per definition, we infer it as a standalone query, so its result will be
//! cached by Salsa. We also need the expression-level query for inferring types in type guard
//! expressions (e.g. the test clause of an `if` statement.)
//!
//! Inferring types at any of the three region granularities returns a [`TypeInference`], which
//! holds types for every [`Definition`] and expression within the inferred region.
//!
//! Some type expressions can require deferred evaluation. This includes all type expressions in
//! stub files, or annotation expressions in modules with `from __future__ import annotations`, or
//! stringified annotations. We have a fourth Salsa query for inferring the deferred types
//! associated with a particular definition. Scope-level inference infers deferred types for all
//! definitions once the rest of the types in the scope have been inferred.
use itertools::Itertools;
use std::num::NonZeroU32;

use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, AnyNodeRef, ExprContext, UnaryOp};
use ruff_text_size::Ranged;
use rustc_hash::FxHashMap;
use salsa;
use salsa::plumbing::AsId;

use crate::module_name::ModuleName;
use crate::module_resolver::{file_to_module, resolve_module};
use crate::semantic_index::ast_ids::{HasScopedAstId, HasScopedUseId, ScopedExpressionId};
use crate::semantic_index::definition::{
    Definition, DefinitionKind, DefinitionNodeKey, ExceptHandlerDefinitionKind,
};
use crate::semantic_index::expression::Expression;
use crate::semantic_index::semantic_index;
use crate::semantic_index::symbol::{NodeWithScopeKind, NodeWithScopeRef, ScopeId};
use crate::semantic_index::SemanticIndex;
use crate::stdlib::builtins_module_scope;
use crate::types::diagnostic::{TypeCheckDiagnostic, TypeCheckDiagnostics};
use crate::types::{
    bindings_ty, builtins_symbol_ty, declarations_ty, global_symbol_ty, symbol_ty,
    typing_extensions_symbol_ty, BytesLiteralType, ClassType, FunctionType, KnownFunction,
    StringLiteralType, Truthiness, TupleType, Type, TypeArrayDisplay, UnionType,
};
use crate::Db;

use super::KnownClass;

/// Infer all types for a [`ScopeId`], including all definitions and expressions in that scope.
/// Use when checking a scope, or needing to provide a type for an arbitrary expression in the
/// scope.
#[salsa::tracked(return_ref)]
pub(crate) fn infer_scope_types<'db>(db: &'db dyn Db, scope: ScopeId<'db>) -> TypeInference<'db> {
    let file = scope.file(db);
    let _span =
        tracing::trace_span!("infer_scope_types", scope=?scope.as_id(), file=%file.path(db))
            .entered();

    // Using the index here is fine because the code below depends on the AST anyway.
    // The isolation of the query is by the return inferred types.
    let index = semantic_index(db, file);

    TypeInferenceBuilder::new(db, InferenceRegion::Scope(scope), index).finish()
}

/// Cycle recovery for [`infer_definition_types()`]: for now, just [`Type::Unknown`]
/// TODO fixpoint iteration
fn infer_definition_types_cycle_recovery<'db>(
    db: &'db dyn Db,
    _cycle: &salsa::Cycle,
    input: Definition<'db>,
) -> TypeInference<'db> {
    tracing::trace!("infer_definition_types_cycle_recovery");
    let mut inference = TypeInference::default();
    let category = input.category(db);
    if category.is_declaration() {
        inference.declarations.insert(input, Type::Unknown);
    }
    if category.is_binding() {
        inference.bindings.insert(input, Type::Unknown);
    }
    // TODO we don't fill in expression types for the cycle-participant definitions, which can
    // later cause a panic when looking up an expression type.
    inference
}

/// Infer all types for a [`Definition`] (including sub-expressions).
/// Use when resolving a symbol name use or public type of a symbol.
#[salsa::tracked(return_ref, recovery_fn=infer_definition_types_cycle_recovery)]
pub(crate) fn infer_definition_types<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> TypeInference<'db> {
    let file = definition.file(db);
    let _span = tracing::trace_span!(
        "infer_definition_types",
        definition = ?definition.as_id(),
        file = %file.path(db)
    )
    .entered();

    let index = semantic_index(db, file);

    TypeInferenceBuilder::new(db, InferenceRegion::Definition(definition), index).finish()
}

/// Infer types for all deferred type expressions in a [`Definition`].
///
/// Deferred expressions are type expressions (annotations, base classes, aliases...) in a stub
/// file, or in a file with `from __future__ import annotations`, or stringified annotations.
#[salsa::tracked(return_ref)]
pub(crate) fn infer_deferred_types<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> TypeInference<'db> {
    let file = definition.file(db);
    let _span = tracing::trace_span!(
        "infer_deferred_types",
        definition = ?definition.as_id(),
        file = %file.path(db)
    )
    .entered();

    let index = semantic_index(db, file);

    TypeInferenceBuilder::new(db, InferenceRegion::Deferred(definition), index).finish()
}

/// Infer all types for an [`Expression`] (including sub-expressions).
/// Use rarely; only for cases where we'd otherwise risk double-inferring an expression: RHS of an
/// assignment, which might be unpacking/multi-target and thus part of multiple definitions, or a
/// type narrowing guard expression (e.g. if statement test node).
#[allow(unused)]
#[salsa::tracked(return_ref)]
pub(crate) fn infer_expression_types<'db>(
    db: &'db dyn Db,
    expression: Expression<'db>,
) -> TypeInference<'db> {
    let file = expression.file(db);
    let _span =
        tracing::trace_span!("infer_expression_types", expression=?expression.as_id(), file=%file.path(db))
            .entered();

    let index = semantic_index(db, file);

    TypeInferenceBuilder::new(db, InferenceRegion::Expression(expression), index).finish()
}

/// A region within which we can infer types.
pub(crate) enum InferenceRegion<'db> {
    /// infer types for a standalone [`Expression`]
    Expression(Expression<'db>),
    /// infer types for a [`Definition`]
    Definition(Definition<'db>),
    /// infer deferred types for a [`Definition`]
    Deferred(Definition<'db>),
    /// infer types for an entire [`ScopeId`]
    Scope(ScopeId<'db>),
}

/// The inferred types for a single region.
#[derive(Debug, Eq, PartialEq, Default)]
pub(crate) struct TypeInference<'db> {
    /// The types of every expression in this region.
    expressions: FxHashMap<ScopedExpressionId, Type<'db>>,

    /// The types of every binding in this region.
    bindings: FxHashMap<Definition<'db>, Type<'db>>,

    /// The types of every declaration in this region.
    declarations: FxHashMap<Definition<'db>, Type<'db>>,

    /// The diagnostics for this region.
    diagnostics: TypeCheckDiagnostics,

    /// Are there deferred type expressions in this region?
    has_deferred: bool,
}

impl<'db> TypeInference<'db> {
    pub(crate) fn expression_ty(&self, expression: ScopedExpressionId) -> Type<'db> {
        self.expressions[&expression]
    }

    pub(crate) fn try_expression_ty(&self, expression: ScopedExpressionId) -> Option<Type<'db>> {
        self.expressions.get(&expression).copied()
    }

    pub(crate) fn binding_ty(&self, definition: Definition<'db>) -> Type<'db> {
        self.bindings[&definition]
    }

    pub(crate) fn declaration_ty(&self, definition: Definition<'db>) -> Type<'db> {
        self.declarations[&definition]
    }

    pub(crate) fn diagnostics(&self) -> &[std::sync::Arc<TypeCheckDiagnostic>] {
        &self.diagnostics
    }

    fn shrink_to_fit(&mut self) {
        self.expressions.shrink_to_fit();
        self.bindings.shrink_to_fit();
        self.declarations.shrink_to_fit();
        self.diagnostics.shrink_to_fit();
    }
}

/// Builder to infer all types in a region.
///
/// A builder is used by creating it with [`new()`](TypeInferenceBuilder::new), and then calling
/// [`finish()`](TypeInferenceBuilder::finish) on it, which returns the resulting
/// [`TypeInference`].
///
/// There are a few different kinds of methods in the type inference builder, and the naming
/// distinctions are a bit subtle.
///
/// The `finish` method calls [`infer_region`](TypeInferenceBuilder::infer_region), which delegates
/// to one of [`infer_region_scope`](TypeInferenceBuilder::infer_region_scope),
/// [`infer_region_definition`](TypeInferenceBuilder::infer_region_definition), or
/// [`infer_region_expression`](TypeInferenceBuilder::infer_region_expression), depending which
/// kind of [`InferenceRegion`] we are inferring types for.
///
/// Scope inference starts with the scope body, walking all statements and expressions and
/// recording the types of each expression in the [`TypeInference`] result. Most of the methods
/// here (with names like `infer_*_statement` or `infer_*_expression` or some other node kind) take
/// a single AST node and are called as part of this AST visit.
///
/// When the visit encounters a node which creates a [`Definition`], we look up the definition in
/// the semantic index and call the [`infer_definition_types()`] query on it, which creates another
/// [`TypeInferenceBuilder`] just for that definition, and we merge the returned [`TypeInference`]
/// into the one we are currently building for the entire scope. Using the query in this way
/// ensures that if we first infer types for some scattered definitions in a scope, and later for
/// the entire scope, we don't re-infer any types, we re-use the cached inference for those
/// definitions and their sub-expressions.
///
/// Functions with a name like `infer_*_definition` take both a node and a [`Definition`], and are
/// called by [`infer_region_definition`](TypeInferenceBuilder::infer_region_definition).
///
/// So for example we have both
/// [`infer_function_definition_statement`](TypeInferenceBuilder::infer_function_definition_statement),
/// which takes just the function AST node, and
/// [`infer_function_definition`](TypeInferenceBuilder::infer_function_definition), which takes
/// both the node and the [`Definition`] id. The former is called as part of walking the AST, and
/// it just looks up the [`Definition`] for that function in the semantic index and calls
/// [`infer_definition_types()`] on it, which will create a new [`TypeInferenceBuilder`] with
/// [`InferenceRegion::Definition`], and in that builder
/// [`infer_region_definition`](TypeInferenceBuilder::infer_region_definition) will call
/// [`infer_function_definition`](TypeInferenceBuilder::infer_function_definition) to actually
/// infer a type for the definition.
///
/// Similarly, when we encounter a standalone-inferable expression (right-hand side of an
/// assignment, type narrowing guard), we use the [`infer_expression_types()`] query to ensure we
/// don't infer its types more than once.
pub(super) struct TypeInferenceBuilder<'db> {
    db: &'db dyn Db,
    index: &'db SemanticIndex<'db>,
    region: InferenceRegion<'db>,

    // Cached lookups
    file: File,
    scope: ScopeId<'db>,

    /// The type inference results
    types: TypeInference<'db>,
}

impl<'db> TypeInferenceBuilder<'db> {
    /// How big a string do we build before bailing?
    ///
    /// This is a fairly arbitrary number. It should be *far* more than enough
    /// for most use cases, but we can reevaluate it later if useful.
    const MAX_STRING_LITERAL_SIZE: usize = 4096;

    /// Creates a new builder for inferring types in a region.
    pub(super) fn new(
        db: &'db dyn Db,
        region: InferenceRegion<'db>,
        index: &'db SemanticIndex<'db>,
    ) -> Self {
        let (file, scope) = match region {
            InferenceRegion::Expression(expression) => (expression.file(db), expression.scope(db)),
            InferenceRegion::Definition(definition) | InferenceRegion::Deferred(definition) => {
                (definition.file(db), definition.scope(db))
            }
            InferenceRegion::Scope(scope) => (scope.file(db), scope),
        };

        Self {
            db,
            index,
            region,

            file,
            scope,

            types: TypeInference::default(),
        }
    }

    fn extend(&mut self, inference: &TypeInference<'db>) {
        self.types.bindings.extend(inference.bindings.iter());
        self.types
            .declarations
            .extend(inference.declarations.iter());
        self.types.expressions.extend(inference.expressions.iter());
        self.types.diagnostics.extend(&inference.diagnostics);
        self.types.has_deferred |= inference.has_deferred;
    }

    /// Are we currently inferring types in file with deferred types?
    /// This is true for stub files and files with `__future__.annotations`
    fn are_all_types_deferred(&self) -> bool {
        self.index.has_future_annotations() || self.file.is_stub(self.db.upcast())
    }

    /// Are we currently inferring deferred types?
    fn is_deferred(&self) -> bool {
        matches!(self.region, InferenceRegion::Deferred(_))
    }

    /// Get the already-inferred type of an expression node.
    ///
    /// PANIC if no type has been inferred for this node.
    fn expression_ty(&self, expr: &ast::Expr) -> Type<'db> {
        self.types
            .expression_ty(expr.scoped_ast_id(self.db, self.scope))
    }

    /// Infers types in the given [`InferenceRegion`].
    fn infer_region(&mut self) {
        match self.region {
            InferenceRegion::Scope(scope) => self.infer_region_scope(scope),
            InferenceRegion::Definition(definition) => self.infer_region_definition(definition),
            InferenceRegion::Deferred(definition) => self.infer_region_deferred(definition),
            InferenceRegion::Expression(expression) => self.infer_region_expression(expression),
        }
    }

    fn infer_region_scope(&mut self, scope: ScopeId<'db>) {
        let node = scope.node(self.db);
        match node {
            NodeWithScopeKind::Module => {
                let parsed = parsed_module(self.db.upcast(), self.file);
                self.infer_module(parsed.syntax());
            }
            NodeWithScopeKind::Function(function) => self.infer_function_body(function.node()),
            NodeWithScopeKind::Lambda(lambda) => self.infer_lambda_body(lambda.node()),
            NodeWithScopeKind::Class(class) => self.infer_class_body(class.node()),
            NodeWithScopeKind::ClassTypeParameters(class) => {
                self.infer_class_type_params(class.node());
            }
            NodeWithScopeKind::FunctionTypeParameters(function) => {
                self.infer_function_type_params(function.node());
            }
            NodeWithScopeKind::ListComprehension(comprehension) => {
                self.infer_list_comprehension_expression_scope(comprehension.node());
            }
            NodeWithScopeKind::SetComprehension(comprehension) => {
                self.infer_set_comprehension_expression_scope(comprehension.node());
            }
            NodeWithScopeKind::DictComprehension(comprehension) => {
                self.infer_dict_comprehension_expression_scope(comprehension.node());
            }
            NodeWithScopeKind::GeneratorExpression(generator) => {
                self.infer_generator_expression_scope(generator.node());
            }
        }

        if self.types.has_deferred {
            let mut deferred_expression_types: FxHashMap<ScopedExpressionId, Type<'db>> =
                FxHashMap::default();
            // invariant: only annotations and base classes are deferred, and both of these only
            // occur within a declaration (annotated assignment, function or class definition)
            for definition in self.types.declarations.keys() {
                if infer_definition_types(self.db, *definition).has_deferred {
                    let deferred = infer_deferred_types(self.db, *definition);
                    deferred_expression_types.extend(deferred.expressions.iter());
                }
            }
            self.types
                .expressions
                .extend(deferred_expression_types.iter());
        }
    }

    fn infer_region_definition(&mut self, definition: Definition<'db>) {
        match definition.kind(self.db) {
            DefinitionKind::Function(function) => {
                self.infer_function_definition(function.node(), definition);
            }
            DefinitionKind::Class(class) => self.infer_class_definition(class.node(), definition),
            DefinitionKind::Import(import) => {
                self.infer_import_definition(import.node(), definition);
            }
            DefinitionKind::ImportFrom(import_from) => {
                self.infer_import_from_definition(
                    import_from.import(),
                    import_from.alias(),
                    definition,
                );
            }
            DefinitionKind::Assignment(assignment) => {
                self.infer_assignment_definition(
                    assignment.target(),
                    assignment.assignment(),
                    definition,
                );
            }
            DefinitionKind::AnnotatedAssignment(annotated_assignment) => {
                self.infer_annotated_assignment_definition(annotated_assignment.node(), definition);
            }
            DefinitionKind::AugmentedAssignment(augmented_assignment) => {
                self.infer_augment_assignment_definition(augmented_assignment.node(), definition);
            }
            DefinitionKind::For(for_statement_definition) => {
                self.infer_for_statement_definition(
                    for_statement_definition.target(),
                    for_statement_definition.iterable(),
                    for_statement_definition.is_async(),
                    definition,
                );
            }
            DefinitionKind::NamedExpression(named_expression) => {
                self.infer_named_expression_definition(named_expression.node(), definition);
            }
            DefinitionKind::Comprehension(comprehension) => {
                self.infer_comprehension_definition(
                    comprehension.iterable(),
                    comprehension.target(),
                    comprehension.is_first(),
                    comprehension.is_async(),
                    definition,
                );
            }
            DefinitionKind::Parameter(parameter) => {
                self.infer_parameter_definition(parameter, definition);
            }
            DefinitionKind::ParameterWithDefault(parameter_with_default) => {
                self.infer_parameter_with_default_definition(parameter_with_default, definition);
            }
            DefinitionKind::WithItem(with_item) => {
                self.infer_with_item_definition(with_item.target(), with_item.node(), definition);
            }
            DefinitionKind::MatchPattern(match_pattern) => {
                self.infer_match_pattern_definition(
                    match_pattern.pattern(),
                    match_pattern.index(),
                    definition,
                );
            }
            DefinitionKind::ExceptHandler(except_handler_definition) => {
                self.infer_except_handler_definition(except_handler_definition, definition);
            }
        }
    }

    fn infer_region_deferred(&mut self, definition: Definition<'db>) {
        match definition.kind(self.db) {
            DefinitionKind::Function(function) => self.infer_function_deferred(function.node()),
            DefinitionKind::Class(class) => self.infer_class_deferred(class.node()),
            DefinitionKind::AnnotatedAssignment(_annotated_assignment) => {
                // TODO self.infer_annotated_assignment_deferred(annotated_assignment.node());
            }
            _ => {}
        }
    }

    fn infer_region_expression(&mut self, expression: Expression<'db>) {
        self.infer_expression(expression.node_ref(self.db));
    }

    fn invalid_assignment_diagnostic(
        &mut self,
        node: AnyNodeRef,
        declared_ty: Type<'db>,
        assigned_ty: Type<'db>,
    ) {
        match declared_ty {
            Type::Class(class) => {
                self.add_diagnostic(node, "invalid-assignment", format_args!(
                        "Implicit shadowing of class `{}`; annotate to make it explicit if this is intentional",
                        class.name(self.db)));
            }
            Type::Function(function) => {
                self.add_diagnostic(node, "invalid-assignment", format_args!(
                        "Implicit shadowing of function `{}`; annotate to make it explicit if this is intentional",
                        function.name(self.db)));
            }
            _ => {
                self.add_diagnostic(
                    node,
                    "invalid-assignment",
                    format_args!(
                        "Object of type `{}` is not assignable to `{}`",
                        assigned_ty.display(self.db),
                        declared_ty.display(self.db),
                    ),
                );
            }
        }
    }

    /// Raise a diagnostic if the given type cannot be divided by zero.
    ///
    /// Expects the resolved type of the left side of the binary expression.
    fn check_division_by_zero(&mut self, expr: &ast::ExprBinOp, left: Type<'db>) {
        match left {
            Type::IntLiteral(_) => {}
            Type::Instance(cls)
                if cls.is_known(self.db, KnownClass::Float)
                    || cls.is_known(self.db, KnownClass::Int) => {}
            _ => return,
        };

        let (op, by_zero) = match expr.op {
            ast::Operator::Div => ("divide", "by zero"),
            ast::Operator::FloorDiv => ("floor divide", "by zero"),
            ast::Operator::Mod => ("reduce", "modulo zero"),
            _ => return,
        };

        self.add_diagnostic(
            expr.into(),
            "division-by-zero",
            format_args!(
                "Cannot {op} object of type `{}` {by_zero}",
                left.display(self.db)
            ),
        );
    }

    fn add_binding(&mut self, node: AnyNodeRef, binding: Definition<'db>, ty: Type<'db>) {
        debug_assert!(binding.is_binding(self.db));
        let use_def = self.index.use_def_map(binding.file_scope(self.db));
        let declarations = use_def.declarations_at_binding(binding);
        let undeclared_ty = if declarations.may_be_undeclared() {
            Some(Type::Unknown)
        } else {
            None
        };
        let mut bound_ty = ty;
        let declared_ty = declarations_ty(self.db, declarations, undeclared_ty).unwrap_or_else(
            |(ty, conflicting)| {
                // TODO point out the conflicting declarations in the diagnostic?
                let symbol_table = self.index.symbol_table(binding.file_scope(self.db));
                let symbol_name = symbol_table.symbol(binding.symbol(self.db)).name();
                self.add_diagnostic(
                    node,
                    "conflicting-declarations",
                    format_args!(
                        "Conflicting declared types for `{symbol_name}`: {}",
                        conflicting.display(self.db)
                    ),
                );
                ty
            },
        );
        if !bound_ty.is_assignable_to(self.db, declared_ty) {
            self.invalid_assignment_diagnostic(node, declared_ty, bound_ty);
            // allow declarations to override inference in case of invalid assignment
            bound_ty = declared_ty;
        };

        self.types.bindings.insert(binding, bound_ty);
    }

    fn add_declaration(&mut self, node: AnyNodeRef, declaration: Definition<'db>, ty: Type<'db>) {
        debug_assert!(declaration.is_declaration(self.db));
        let use_def = self.index.use_def_map(declaration.file_scope(self.db));
        let prior_bindings = use_def.bindings_at_declaration(declaration);
        // unbound_ty is Never because for this check we don't care about unbound
        let inferred_ty = bindings_ty(self.db, prior_bindings, Some(Type::Never));
        let ty = if inferred_ty.is_assignable_to(self.db, ty) {
            ty
        } else {
            self.add_diagnostic(
                node,
                "invalid-declaration",
                format_args!(
                    "Cannot declare type `{}` for inferred type `{}`",
                    ty.display(self.db),
                    inferred_ty.display(self.db)
                ),
            );
            Type::Unknown
        };
        self.types.declarations.insert(declaration, ty);
    }

    fn add_declaration_with_binding(
        &mut self,
        node: AnyNodeRef,
        definition: Definition<'db>,
        declared_ty: Type<'db>,
        inferred_ty: Type<'db>,
    ) {
        debug_assert!(definition.is_binding(self.db));
        debug_assert!(definition.is_declaration(self.db));
        let inferred_ty = if inferred_ty.is_assignable_to(self.db, declared_ty) {
            inferred_ty
        } else {
            self.invalid_assignment_diagnostic(node, declared_ty, inferred_ty);
            // if the assignment is invalid, fall back to assuming the annotation is correct
            declared_ty
        };
        self.types.declarations.insert(definition, declared_ty);
        self.types.bindings.insert(definition, inferred_ty);
    }

    fn infer_module(&mut self, module: &ast::ModModule) {
        self.infer_body(&module.body);
    }

    fn infer_class_type_params(&mut self, class: &ast::StmtClassDef) {
        let type_params = class
            .type_params
            .as_deref()
            .expect("class type params scope without type params");

        self.infer_type_parameters(type_params);

        if let Some(arguments) = class.arguments.as_deref() {
            self.infer_arguments(arguments);
        }
    }

    fn infer_class_body(&mut self, class: &ast::StmtClassDef) {
        self.infer_body(&class.body);
    }

    fn infer_function_type_params(&mut self, function: &ast::StmtFunctionDef) {
        let type_params = function
            .type_params
            .as_deref()
            .expect("function type params scope without type params");

        // TODO: defer annotation resolution in stubs, with __future__.annotations, or stringified
        self.infer_optional_expression(function.returns.as_deref());
        self.infer_type_parameters(type_params);
        self.infer_parameters(&function.parameters);
    }

    fn infer_function_body(&mut self, function: &ast::StmtFunctionDef) {
        self.infer_body(&function.body);
    }

    fn infer_body(&mut self, suite: &[ast::Stmt]) {
        for statement in suite {
            self.infer_statement(statement);
        }
    }

    fn infer_statement(&mut self, statement: &ast::Stmt) {
        match statement {
            ast::Stmt::FunctionDef(function) => self.infer_function_definition_statement(function),
            ast::Stmt::ClassDef(class) => self.infer_class_definition_statement(class),
            ast::Stmt::Expr(ast::StmtExpr { range: _, value }) => {
                self.infer_expression(value);
            }
            ast::Stmt::If(if_statement) => self.infer_if_statement(if_statement),
            ast::Stmt::Try(try_statement) => self.infer_try_statement(try_statement),
            ast::Stmt::With(with_statement) => self.infer_with_statement(with_statement),
            ast::Stmt::Match(match_statement) => self.infer_match_statement(match_statement),
            ast::Stmt::Assign(assign) => self.infer_assignment_statement(assign),
            ast::Stmt::AnnAssign(assign) => self.infer_annotated_assignment_statement(assign),
            ast::Stmt::AugAssign(aug_assign) => {
                self.infer_augmented_assignment_statement(aug_assign);
            }
            ast::Stmt::TypeAlias(type_statement) => self.infer_type_alias_statement(type_statement),
            ast::Stmt::For(for_statement) => self.infer_for_statement(for_statement),
            ast::Stmt::While(while_statement) => self.infer_while_statement(while_statement),
            ast::Stmt::Import(import) => self.infer_import_statement(import),
            ast::Stmt::ImportFrom(import) => self.infer_import_from_statement(import),
            ast::Stmt::Assert(assert_statement) => self.infer_assert_statement(assert_statement),
            ast::Stmt::Raise(raise) => self.infer_raise_statement(raise),
            ast::Stmt::Return(ret) => self.infer_return_statement(ret),
            ast::Stmt::Delete(delete) => self.infer_delete_statement(delete),
            ast::Stmt::Break(_)
            | ast::Stmt::Continue(_)
            | ast::Stmt::Pass(_)
            | ast::Stmt::IpyEscapeCommand(_)
            | ast::Stmt::Global(_)
            | ast::Stmt::Nonlocal(_) => {
                // No-op
            }
        }
    }

    fn infer_definition(&mut self, node: impl Into<DefinitionNodeKey>) {
        let definition = self.index.definition(node);
        let result = infer_definition_types(self.db, definition);
        self.extend(result);
    }

    fn infer_function_definition_statement(&mut self, function: &ast::StmtFunctionDef) {
        self.infer_definition(function);
    }

    fn infer_function_definition(
        &mut self,
        function: &ast::StmtFunctionDef,
        definition: Definition<'db>,
    ) {
        let ast::StmtFunctionDef {
            range: _,
            is_async: _,
            name,
            type_params,
            parameters,
            returns,
            body: _,
            decorator_list,
        } = function;

        let decorator_tys = decorator_list
            .iter()
            .map(|decorator| self.infer_decorator(decorator))
            .collect();

        for default in parameters
            .iter_non_variadic_params()
            .filter_map(|param| param.default.as_deref())
        {
            self.infer_expression(default);
        }

        // If there are type params, parameters and returns are evaluated in that scope, that is, in
        // `infer_function_type_params`, rather than here.
        if type_params.is_none() {
            self.infer_parameters(parameters);

            // TODO: this should also be applied to parameter annotations.
            if self.are_all_types_deferred() {
                self.types.has_deferred = true;
            } else {
                self.infer_optional_annotation_expression(returns.as_deref());
            }
        }

        let function_kind = match &**name {
            "reveal_type" if definition.is_typing_definition(self.db) => {
                Some(KnownFunction::RevealType)
            }
            _ => None,
        };
        let function_ty = Type::Function(FunctionType::new(
            self.db,
            name.id.clone(),
            function_kind,
            definition,
            decorator_tys,
        ));

        self.add_declaration_with_binding(function.into(), definition, function_ty, function_ty);
    }

    fn infer_parameters(&mut self, parameters: &ast::Parameters) {
        let ast::Parameters {
            range: _,
            posonlyargs: _,
            args: _,
            vararg,
            kwonlyargs: _,
            kwarg,
        } = parameters;

        for param_with_default in parameters.iter_non_variadic_params() {
            self.infer_parameter_with_default(param_with_default);
        }
        if let Some(vararg) = vararg {
            self.infer_parameter(vararg);
        }
        if let Some(kwarg) = kwarg {
            self.infer_parameter(kwarg);
        }
    }

    fn infer_parameter_with_default(&mut self, parameter_with_default: &ast::ParameterWithDefault) {
        let ast::ParameterWithDefault {
            range: _,
            parameter,
            default: _,
        } = parameter_with_default;

        self.infer_optional_expression(parameter.annotation.as_deref());

        self.infer_definition(parameter_with_default);
    }

    fn infer_parameter(&mut self, parameter: &ast::Parameter) {
        let ast::Parameter {
            range: _,
            name: _,
            annotation,
        } = parameter;

        self.infer_optional_expression(annotation.as_deref());

        self.infer_definition(parameter);
    }

    fn infer_parameter_with_default_definition(
        &mut self,
        parameter_with_default: &ast::ParameterWithDefault,
        definition: Definition<'db>,
    ) {
        // TODO(dhruvmanila): Infer types from annotation or default expression
        // TODO check that default is assignable to parameter type
        self.infer_parameter_definition(&parameter_with_default.parameter, definition);
    }

    fn infer_parameter_definition(
        &mut self,
        parameter: &ast::Parameter,
        definition: Definition<'db>,
    ) {
        // TODO(dhruvmanila): Annotation expression is resolved at the enclosing scope, infer the
        // parameter type from there
        let annotated_ty = Type::Todo;
        if parameter.annotation.is_some() {
            self.add_declaration_with_binding(
                parameter.into(),
                definition,
                annotated_ty,
                annotated_ty,
            );
        } else {
            self.add_binding(parameter.into(), definition, annotated_ty);
        }
    }

    fn infer_class_definition_statement(&mut self, class: &ast::StmtClassDef) {
        self.infer_definition(class);
    }

    fn infer_class_definition(&mut self, class: &ast::StmtClassDef, definition: Definition<'db>) {
        let ast::StmtClassDef {
            range: _,
            name,
            type_params: _,
            decorator_list,
            arguments: _,
            body: _,
        } = class;

        for decorator in decorator_list {
            self.infer_decorator(decorator);
        }

        let body_scope = self
            .index
            .node_scope(NodeWithScopeRef::Class(class))
            .to_scope_id(self.db, self.file);

        let maybe_known_class = file_to_module(self.db, body_scope.file(self.db))
            .as_ref()
            .and_then(|module| KnownClass::maybe_from_module(module, name.as_str()));
        let class_ty = Type::Class(ClassType::new(
            self.db,
            name.id.clone(),
            definition,
            body_scope,
            maybe_known_class,
        ));

        self.add_declaration_with_binding(class.into(), definition, class_ty, class_ty);

        for keyword in class.keywords() {
            self.infer_expression(&keyword.value);
        }

        // Inference of bases deferred in stubs
        // TODO also defer stringified generic type parameters
        if self.are_all_types_deferred() {
            self.types.has_deferred = true;
        } else {
            for base in class.bases() {
                self.infer_expression(base);
            }
        }
    }

    fn infer_function_deferred(&mut self, function: &ast::StmtFunctionDef) {
        self.infer_optional_annotation_expression(function.returns.as_deref());
    }

    fn infer_class_deferred(&mut self, class: &ast::StmtClassDef) {
        if self.are_all_types_deferred() {
            for base in class.bases() {
                self.infer_expression(base);
            }
        }
    }

    fn infer_if_statement(&mut self, if_statement: &ast::StmtIf) {
        let ast::StmtIf {
            range: _,
            test,
            body,
            elif_else_clauses,
        } = if_statement;

        self.infer_expression(test);
        self.infer_body(body);

        for clause in elif_else_clauses {
            let ast::ElifElseClause {
                range: _,
                test,
                body,
            } = clause;

            self.infer_optional_expression(test.as_ref());

            self.infer_body(body);
        }
    }

    fn infer_try_statement(&mut self, try_statement: &ast::StmtTry) {
        let ast::StmtTry {
            range: _,
            body,
            handlers,
            orelse,
            finalbody,
            is_star: _,
        } = try_statement;

        self.infer_body(body);

        for handler in handlers {
            let ast::ExceptHandler::ExceptHandler(handler) = handler;
            let ast::ExceptHandlerExceptHandler {
                type_: handled_exceptions,
                name: symbol_name,
                body,
                range: _,
            } = handler;

            // If `symbol_name` is `Some()` and `handled_exceptions` is `None`,
            // it's invalid syntax (something like `except as e:`).
            // However, it's obvious that the user *wanted* `e` to be bound here,
            // so we'll have created a definition in the semantic-index stage anyway.
            if symbol_name.is_some() {
                self.infer_definition(handler);
            } else {
                self.infer_optional_expression(handled_exceptions.as_deref());
            }

            self.infer_body(body);
        }

        self.infer_body(orelse);
        self.infer_body(finalbody);
    }

    fn infer_with_statement(&mut self, with_statement: &ast::StmtWith) {
        let ast::StmtWith {
            range: _,
            is_async: _,
            items,
            body,
        } = with_statement;

        for item in items {
            let target = item.optional_vars.as_deref();
            if let Some(ast::Expr::Name(name)) = target {
                self.infer_definition(name);
            } else {
                // TODO infer definitions in unpacking assignment
                self.infer_expression(&item.context_expr);
                self.infer_optional_expression(target);
            }
        }

        self.infer_body(body);
    }

    fn infer_with_item_definition(
        &mut self,
        target: &ast::ExprName,
        with_item: &ast::WithItem,
        definition: Definition<'db>,
    ) {
        let expression = self.index.expression(&with_item.context_expr);
        let result = infer_expression_types(self.db, expression);
        self.extend(result);

        // TODO(dhruvmanila): The correct type inference here is the return type of the __enter__
        // method of the context manager.
        let context_expr_ty = self.expression_ty(&with_item.context_expr);

        self.types
            .expressions
            .insert(target.scoped_ast_id(self.db, self.scope), context_expr_ty);
        self.add_binding(target.into(), definition, context_expr_ty);
    }

    fn infer_except_handler_definition(
        &mut self,
        except_handler_definition: &ExceptHandlerDefinitionKind,
        definition: Definition<'db>,
    ) {
        let node_ty = except_handler_definition
            .handled_exceptions()
            .map(|ty| self.infer_expression(ty))
            // If there is no handled exception, it's invalid syntax;
            // a diagnostic will have already been emitted
            .unwrap_or(Type::Unknown);

        let symbol_ty = if except_handler_definition.is_star() {
            // TODO should be generic --Alex
            //
            // TODO should infer `ExceptionGroup` if all caught exceptions
            // are subclasses of `Exception` --Alex
            builtins_symbol_ty(self.db, "BaseExceptionGroup").to_instance(self.db)
        } else {
            // TODO: anything that's a consistent subtype of
            // `type[BaseException] | tuple[type[BaseException], ...]` should be valid;
            // anything else should be invalid --Alex
            match node_ty {
                Type::Any | Type::Unknown => node_ty,
                Type::Class(class_ty) => Type::Instance(class_ty),
                _ => Type::Todo,
            }
        };

        self.add_binding(
            except_handler_definition.node().into(),
            definition,
            symbol_ty,
        );
    }

    fn infer_match_statement(&mut self, match_statement: &ast::StmtMatch) {
        let ast::StmtMatch {
            range: _,
            subject,
            cases,
        } = match_statement;

        let expression = self.index.expression(subject.as_ref());
        let result = infer_expression_types(self.db, expression);
        self.extend(result);

        for case in cases {
            let ast::MatchCase {
                range: _,
                body,
                pattern,
                guard,
            } = case;
            self.infer_match_pattern(pattern);
            self.infer_optional_expression(guard.as_deref());
            self.infer_body(body);
        }
    }

    fn infer_match_pattern_definition(
        &mut self,
        pattern: &ast::Pattern,
        _index: u32,
        definition: Definition<'db>,
    ) {
        // TODO(dhruvmanila): The correct way to infer types here is to perform structural matching
        // against the subject expression type (which we can query via `infer_expression_types`)
        // and extract the type at the `index` position if the pattern matches. This will be
        // similar to the logic in `self.infer_assignment_definition`.
        self.add_binding(pattern.into(), definition, Type::Todo);
    }

    fn infer_match_pattern(&mut self, pattern: &ast::Pattern) {
        // TODO(dhruvmanila): Add a Salsa query for inferring pattern types and matching against
        // the subject expression: https://github.com/astral-sh/ruff/pull/13147#discussion_r1739424510
        match pattern {
            ast::Pattern::MatchValue(match_value) => {
                self.infer_expression(&match_value.value);
            }
            ast::Pattern::MatchSequence(match_sequence) => {
                for pattern in &match_sequence.patterns {
                    self.infer_match_pattern(pattern);
                }
            }
            ast::Pattern::MatchMapping(match_mapping) => {
                let ast::PatternMatchMapping {
                    range: _,
                    keys,
                    patterns,
                    rest: _,
                } = match_mapping;
                for key in keys {
                    self.infer_expression(key);
                }
                for pattern in patterns {
                    self.infer_match_pattern(pattern);
                }
            }
            ast::Pattern::MatchClass(match_class) => {
                let ast::PatternMatchClass {
                    range: _,
                    cls,
                    arguments,
                } = match_class;
                for pattern in &arguments.patterns {
                    self.infer_match_pattern(pattern);
                }
                for keyword in &arguments.keywords {
                    self.infer_match_pattern(&keyword.pattern);
                }
                self.infer_expression(cls);
            }
            ast::Pattern::MatchAs(match_as) => {
                if let Some(pattern) = &match_as.pattern {
                    self.infer_match_pattern(pattern);
                }
            }
            ast::Pattern::MatchOr(match_or) => {
                for pattern in &match_or.patterns {
                    self.infer_match_pattern(pattern);
                }
            }
            ast::Pattern::MatchStar(_) | ast::Pattern::MatchSingleton(_) => {}
        };
    }

    fn infer_assignment_statement(&mut self, assignment: &ast::StmtAssign) {
        let ast::StmtAssign {
            range: _,
            targets,
            value,
        } = assignment;

        for target in targets {
            if let ast::Expr::Name(name) = target {
                self.infer_definition(name);
            } else {
                // TODO infer definitions in unpacking assignment. When we do, this duplication of
                // the "get `Expression`, call `infer_expression_types` on it, `self.extend`" dance
                // will be removed; it'll all happen in `infer_assignment_definition` instead.
                let expression = self.index.expression(value.as_ref());
                self.extend(infer_expression_types(self.db, expression));
                self.infer_expression(target);
            }
        }
    }

    fn infer_assignment_definition(
        &mut self,
        target: &ast::ExprName,
        assignment: &ast::StmtAssign,
        definition: Definition<'db>,
    ) {
        let expression = self.index.expression(assignment.value.as_ref());
        let result = infer_expression_types(self.db, expression);
        self.extend(result);
        let value_ty = self.expression_ty(&assignment.value);
        self.add_binding(assignment.into(), definition, value_ty);
        self.types
            .expressions
            .insert(target.scoped_ast_id(self.db, self.scope), value_ty);
    }

    fn infer_annotated_assignment_statement(&mut self, assignment: &ast::StmtAnnAssign) {
        // assignments to non-Names are not Definitions
        if matches!(*assignment.target, ast::Expr::Name(_)) {
            self.infer_definition(assignment);
        } else {
            let ast::StmtAnnAssign {
                range: _,
                annotation,
                value,
                target,
                simple: _,
            } = assignment;
            self.infer_annotation_expression(annotation);
            self.infer_optional_expression(value.as_deref());
            self.infer_expression(target);
        }
    }

    fn infer_annotated_assignment_definition(
        &mut self,
        assignment: &ast::StmtAnnAssign,
        definition: Definition<'db>,
    ) {
        let ast::StmtAnnAssign {
            range: _,
            target,
            annotation,
            value,
            simple: _,
        } = assignment;

        let annotation_ty = self.infer_annotation_expression(annotation);
        if let Some(value) = value {
            let value_ty = self.infer_expression(value);
            self.add_declaration_with_binding(
                assignment.into(),
                definition,
                annotation_ty,
                value_ty,
            );
        } else {
            self.add_declaration(assignment.into(), definition, annotation_ty);
        }

        self.infer_expression(target);
    }

    fn infer_augmented_assignment_statement(&mut self, assignment: &ast::StmtAugAssign) {
        if assignment.target.is_name_expr() {
            self.infer_definition(assignment);
        } else {
            // TODO currently we don't consider assignments to non-Names to be Definitions
            self.infer_augment_assignment(assignment);
        }
    }

    fn infer_augment_assignment_definition(
        &mut self,
        assignment: &ast::StmtAugAssign,
        definition: Definition<'db>,
    ) {
        let target_ty = self.infer_augment_assignment(assignment);
        self.add_binding(assignment.into(), definition, target_ty);
    }

    fn infer_augment_assignment(&mut self, assignment: &ast::StmtAugAssign) -> Type<'db> {
        let ast::StmtAugAssign {
            range: _,
            target,
            op: _,
            value,
        } = assignment;
        self.infer_expression(value);
        self.infer_expression(target);

        // TODO(dhruvmanila): Resolve the target type using the value type and the operator
        Type::Todo
    }

    fn infer_type_alias_statement(&mut self, type_alias_statement: &ast::StmtTypeAlias) {
        let ast::StmtTypeAlias {
            range: _,
            name,
            type_params,
            value,
        } = type_alias_statement;
        self.infer_expression(value);
        self.infer_expression(name);
        if let Some(type_params) = type_params {
            self.infer_type_parameters(type_params);
        }
    }

    fn infer_for_statement(&mut self, for_statement: &ast::StmtFor) {
        let ast::StmtFor {
            range: _,
            target,
            iter,
            body,
            orelse,
            is_async: _,
        } = for_statement;

        self.infer_expression(iter);
        // TODO more complex assignment targets
        if let ast::Expr::Name(name) = &**target {
            self.infer_definition(name);
        } else {
            self.infer_expression(target);
        }
        self.infer_body(body);
        self.infer_body(orelse);
    }

    /// Emit a diagnostic declaring that the object represented by `node` is not iterable
    pub(super) fn not_iterable_diagnostic(&mut self, node: AnyNodeRef, not_iterable_ty: Type<'db>) {
        self.add_diagnostic(
            node,
            "not-iterable",
            format_args!(
                "Object of type `{}` is not iterable",
                not_iterable_ty.display(self.db)
            ),
        );
    }

    /// Emit a diagnostic declaring that an index is out of bounds for a tuple.
    pub(super) fn tuple_index_out_of_bounds_diagnostic(
        &mut self,
        node: AnyNodeRef,
        tuple_ty: Type<'db>,
        length: usize,
        index: i64,
    ) {
        self.add_diagnostic(
            node,
            "index-out-of-bounds",
            format_args!(
                "Index {index} is out of bounds for tuple of type `{}` with length {length}",
                tuple_ty.display(self.db)
            ),
        );
    }

    /// Emit a diagnostic declaring that an index is out of bounds for a string.
    pub(super) fn string_index_out_of_bounds_diagnostic(
        &mut self,
        node: AnyNodeRef,
        string_ty: Type<'db>,
        length: usize,
        index: i64,
    ) {
        self.add_diagnostic(
            node,
            "index-out-of-bounds",
            format_args!(
                "Index {index} is out of bounds for string `{}` with length {length}",
                string_ty.display(self.db)
            ),
        );
    }

    /// Emit a diagnostic declaring that a type does not support subscripting.
    pub(super) fn non_subscriptable_diagnostic(
        &mut self,
        node: AnyNodeRef,
        non_subscriptable_ty: Type<'db>,
        method: &str,
    ) {
        self.add_diagnostic(
            node,
            "non-subscriptable",
            format_args!(
                "Cannot subscript object of type `{}` with no `{method}` method",
                non_subscriptable_ty.display(self.db)
            ),
        );
    }

    fn infer_for_statement_definition(
        &mut self,
        target: &ast::ExprName,
        iterable: &ast::Expr,
        is_async: bool,
        definition: Definition<'db>,
    ) {
        let expression = self.index.expression(iterable);
        let result = infer_expression_types(self.db, expression);
        self.extend(result);
        let iterable_ty = self.expression_ty(iterable);

        let loop_var_value_ty = if is_async {
            // TODO(Alex): async iterables/iterators!
            Type::Todo
        } else {
            iterable_ty
                .iterate(self.db)
                .unwrap_with_diagnostic(iterable.into(), self)
        };

        self.types
            .expressions
            .insert(target.scoped_ast_id(self.db, self.scope), loop_var_value_ty);
        self.add_binding(target.into(), definition, loop_var_value_ty);
    }

    fn infer_while_statement(&mut self, while_statement: &ast::StmtWhile) {
        let ast::StmtWhile {
            range: _,
            test,
            body,
            orelse,
        } = while_statement;

        self.infer_expression(test);
        self.infer_body(body);
        self.infer_body(orelse);
    }

    fn infer_import_statement(&mut self, import: &ast::StmtImport) {
        let ast::StmtImport { range: _, names } = import;

        for alias in names {
            self.infer_definition(alias);
        }
    }

    fn infer_import_definition(&mut self, alias: &'db ast::Alias, definition: Definition<'db>) {
        let ast::Alias {
            range: _,
            name,
            asname: _,
        } = alias;

        let module_ty = if let Some(module_name) = ModuleName::new(name) {
            if let Some(module) = self.module_ty_from_name(module_name) {
                module
            } else {
                self.unresolved_module_diagnostic(alias, 0, Some(name));
                Type::Unknown
            }
        } else {
            tracing::debug!("Failed to resolve import due to invalid syntax");
            Type::Unknown
        };

        self.add_declaration_with_binding(alias.into(), definition, module_ty, module_ty);
    }

    fn infer_import_from_statement(&mut self, import: &ast::StmtImportFrom) {
        let ast::StmtImportFrom {
            range: _,
            module: _,
            names,
            level: _,
        } = import;

        for alias in names {
            self.infer_definition(alias);
        }
    }

    fn infer_assert_statement(&mut self, assert: &ast::StmtAssert) {
        let ast::StmtAssert {
            range: _,
            test,
            msg,
        } = assert;

        self.infer_expression(test);
        self.infer_optional_expression(msg.as_deref());
    }

    fn infer_raise_statement(&mut self, raise: &ast::StmtRaise) {
        let ast::StmtRaise {
            range: _,
            exc,
            cause,
        } = raise;
        self.infer_optional_expression(exc.as_deref());
        self.infer_optional_expression(cause.as_deref());
    }

    fn unresolved_module_diagnostic(
        &mut self,
        import_node: impl Into<AnyNodeRef<'db>>,
        level: u32,
        module: Option<&str>,
    ) {
        self.add_diagnostic(
            import_node.into(),
            "unresolved-import",
            format_args!(
                "Cannot resolve import `{}{}`",
                ".".repeat(level as usize),
                module.unwrap_or_default()
            ),
        );
    }

    /// Given a `from .foo import bar` relative import, resolve the relative module
    /// we're importing `bar` from into an absolute [`ModuleName`]
    /// using the name of the module we're currently analyzing.
    ///
    /// - `level` is the number of dots at the beginning of the relative module name:
    ///   - `from .foo.bar import baz` => `level == 1`
    ///   - `from ...foo.bar import baz` => `level == 3`
    /// - `tail` is the relative module name stripped of all leading dots:
    ///   - `from .foo import bar` => `tail == "foo"`
    ///   - `from ..foo.bar import baz` => `tail == "foo.bar"`
    fn relative_module_name(
        &self,
        tail: Option<&str>,
        level: NonZeroU32,
    ) -> Result<ModuleName, ModuleNameResolutionError> {
        let module = file_to_module(self.db, self.file)
            .ok_or(ModuleNameResolutionError::UnknownCurrentModule)?;
        let mut level = level.get();
        if module.kind().is_package() {
            level -= 1;
        }
        let mut module_name = module.name().to_owned();
        for _ in 0..level {
            module_name = module_name
                .parent()
                .ok_or(ModuleNameResolutionError::TooManyDots)?;
        }
        if let Some(tail) = tail {
            let tail = ModuleName::new(tail).ok_or(ModuleNameResolutionError::InvalidSyntax)?;
            module_name.extend(&tail);
        }
        Ok(module_name)
    }

    fn infer_import_from_definition(
        &mut self,
        import_from: &'db ast::StmtImportFrom,
        alias: &ast::Alias,
        definition: Definition<'db>,
    ) {
        // TODO:
        // - Absolute `*` imports (`from collections import *`)
        // - Relative `*` imports (`from ...foo import *`)
        // - Submodule imports (`from collections import abc`,
        //   where `abc` is a submodule of the `collections` package)
        //
        // For the last item, see the currently skipped tests
        // `follow_relative_import_bare_to_module()` and
        // `follow_nonexistent_import_bare_to_module()`.
        let ast::StmtImportFrom { module, level, .. } = import_from;
        let module = module.as_deref();

        let module_name = if let Some(level) = NonZeroU32::new(*level) {
            tracing::trace!(
                "Resolving imported object `{}` from module `{}` relative to file `{}`",
                alias.name,
                format_import_from_module(level.get(), module),
                self.file.path(self.db),
            );
            self.relative_module_name(module, level)
        } else {
            tracing::trace!(
                "Resolving imported object `{}` from module `{}`",
                alias.name,
                format_import_from_module(*level, module),
            );
            module
                .and_then(ModuleName::new)
                .ok_or(ModuleNameResolutionError::InvalidSyntax)
        };

        let module_ty = match module_name {
            Ok(name) => {
                if let Some(ty) = self.module_ty_from_name(name) {
                    ty
                } else {
                    self.unresolved_module_diagnostic(import_from, *level, module);
                    Type::Unknown
                }
            }
            Err(ModuleNameResolutionError::InvalidSyntax) => {
                tracing::debug!("Failed to resolve import due to invalid syntax");
                // Invalid syntax diagnostics are emitted elsewhere.
                Type::Unknown
            }
            Err(ModuleNameResolutionError::TooManyDots) => {
                tracing::debug!(
                    "Relative module resolution `{}` failed: too many leading dots",
                    format_import_from_module(*level, module),
                );
                self.unresolved_module_diagnostic(import_from, *level, module);
                Type::Unknown
            }
            Err(ModuleNameResolutionError::UnknownCurrentModule) => {
                tracing::debug!(
                    "Relative module resolution `{}` failed; could not resolve file `{}` to a module",
                    format_import_from_module(*level, module),
                    self.file.path(self.db)
                );
                self.unresolved_module_diagnostic(import_from, *level, module);
                Type::Unknown
            }
        };

        let ast::Alias {
            range: _,
            name,
            asname: _,
        } = alias;

        let member_ty = module_ty.member(self.db, &ast::name::Name::new(&name.id));

        // TODO: What if it's a union where one of the elements is `Unbound`?
        if member_ty.is_unbound() {
            self.add_diagnostic(
                AnyNodeRef::Alias(alias),
                "unresolved-import",
                format_args!(
                    "Module `{}{}` has no member `{name}`",
                    ".".repeat(*level as usize),
                    module.unwrap_or_default()
                ),
            );
        }

        // If a symbol is unbound in the module the symbol was originally defined in,
        // when we're trying to import the symbol from that module into "our" module,
        // the runtime error will occur immediately (rather than when the symbol is *used*,
        // as would be the case for a symbol with type `Unbound`), so it's appropriate to
        // think of the type of the imported symbol as `Unknown` rather than `Unbound`
        let ty = member_ty.replace_unbound_with(self.db, Type::Unknown);

        self.add_declaration_with_binding(alias.into(), definition, ty, ty);
    }

    fn infer_return_statement(&mut self, ret: &ast::StmtReturn) {
        self.infer_optional_expression(ret.value.as_deref());
    }

    fn infer_delete_statement(&mut self, delete: &ast::StmtDelete) {
        let ast::StmtDelete { range: _, targets } = delete;
        for target in targets {
            self.infer_expression(target);
        }
    }

    fn module_ty_from_name(&self, module_name: ModuleName) -> Option<Type<'db>> {
        resolve_module(self.db, module_name).map(|module| Type::Module(module.file()))
    }

    fn infer_decorator(&mut self, decorator: &ast::Decorator) -> Type<'db> {
        let ast::Decorator {
            range: _,
            expression,
        } = decorator;

        self.infer_expression(expression)
    }

    fn infer_arguments(&mut self, arguments: &ast::Arguments) -> Vec<Type<'db>> {
        let mut types = Vec::with_capacity(
            arguments
                .args
                .len()
                .saturating_add(arguments.keywords.len()),
        );

        types.extend(arguments.args.iter().map(|arg| self.infer_expression(arg)));

        types.extend(arguments.keywords.iter().map(
            |ast::Keyword {
                 range: _,
                 arg: _,
                 value,
             }| self.infer_expression(value),
        ));

        types
    }

    fn infer_optional_expression(&mut self, expression: Option<&ast::Expr>) -> Option<Type<'db>> {
        expression.map(|expr| self.infer_expression(expr))
    }

    fn infer_optional_annotation_expression(
        &mut self,
        expr: Option<&ast::Expr>,
    ) -> Option<Type<'db>> {
        expr.map(|expr| self.infer_annotation_expression(expr))
    }

    fn infer_expression(&mut self, expression: &ast::Expr) -> Type<'db> {
        let ty = match expression {
            ast::Expr::NoneLiteral(ast::ExprNoneLiteral { range: _ }) => Type::None,
            ast::Expr::NumberLiteral(literal) => self.infer_number_literal_expression(literal),
            ast::Expr::BooleanLiteral(literal) => self.infer_boolean_literal_expression(literal),
            ast::Expr::StringLiteral(literal) => self.infer_string_literal_expression(literal),
            ast::Expr::BytesLiteral(bytes_literal) => {
                self.infer_bytes_literal_expression(bytes_literal)
            }
            ast::Expr::FString(fstring) => self.infer_fstring_expression(fstring),
            ast::Expr::EllipsisLiteral(literal) => self.infer_ellipsis_literal_expression(literal),
            ast::Expr::Tuple(tuple) => self.infer_tuple_expression(tuple),
            ast::Expr::List(list) => self.infer_list_expression(list),
            ast::Expr::Set(set) => self.infer_set_expression(set),
            ast::Expr::Dict(dict) => self.infer_dict_expression(dict),
            ast::Expr::Generator(generator) => self.infer_generator_expression(generator),
            ast::Expr::ListComp(listcomp) => self.infer_list_comprehension_expression(listcomp),
            ast::Expr::DictComp(dictcomp) => self.infer_dict_comprehension_expression(dictcomp),
            ast::Expr::SetComp(setcomp) => self.infer_set_comprehension_expression(setcomp),
            ast::Expr::Name(name) => self.infer_name_expression(name),
            ast::Expr::Attribute(attribute) => self.infer_attribute_expression(attribute),
            ast::Expr::UnaryOp(unary_op) => self.infer_unary_expression(unary_op),
            ast::Expr::BinOp(binary) => self.infer_binary_expression(binary),
            ast::Expr::BoolOp(bool_op) => self.infer_boolean_expression(bool_op),
            ast::Expr::Compare(compare) => self.infer_compare_expression(compare),
            ast::Expr::Subscript(subscript) => self.infer_subscript_expression(subscript),
            ast::Expr::Slice(slice) => self.infer_slice_expression(slice),
            ast::Expr::Named(named) => self.infer_named_expression(named),
            ast::Expr::If(if_expression) => self.infer_if_expression(if_expression),
            ast::Expr::Lambda(lambda_expression) => self.infer_lambda_expression(lambda_expression),
            ast::Expr::Call(call_expression) => self.infer_call_expression(call_expression),
            ast::Expr::Starred(starred) => self.infer_starred_expression(starred),
            ast::Expr::Yield(yield_expression) => self.infer_yield_expression(yield_expression),
            ast::Expr::YieldFrom(yield_from) => self.infer_yield_from_expression(yield_from),
            ast::Expr::Await(await_expression) => self.infer_await_expression(await_expression),
            ast::Expr::IpyEscapeCommand(_) => todo!("Implement Ipy escape command support"),
        };

        let expr_id = expression.scoped_ast_id(self.db, self.scope);
        let previous = self.types.expressions.insert(expr_id, ty);
        assert_eq!(previous, None);

        ty
    }

    fn infer_number_literal_expression(&mut self, literal: &ast::ExprNumberLiteral) -> Type<'db> {
        let ast::ExprNumberLiteral { range: _, value } = literal;

        match value {
            ast::Number::Int(n) => n
                .as_i64()
                .map(Type::IntLiteral)
                .unwrap_or_else(|| KnownClass::Int.to_instance(self.db)),
            ast::Number::Float(_) => KnownClass::Float.to_instance(self.db),
            ast::Number::Complex { .. } => {
                builtins_symbol_ty(self.db, "complex").to_instance(self.db)
            }
        }
    }

    #[allow(clippy::unused_self)]
    fn infer_boolean_literal_expression(&mut self, literal: &ast::ExprBooleanLiteral) -> Type<'db> {
        let ast::ExprBooleanLiteral { range: _, value } = literal;

        Type::BooleanLiteral(*value)
    }

    fn infer_string_literal_expression(&mut self, literal: &ast::ExprStringLiteral) -> Type<'db> {
        if literal.value.len() <= Self::MAX_STRING_LITERAL_SIZE {
            Type::StringLiteral(StringLiteralType::new(
                self.db,
                literal.value.to_str().into(),
            ))
        } else {
            Type::LiteralString
        }
    }

    fn infer_bytes_literal_expression(&mut self, literal: &ast::ExprBytesLiteral) -> Type<'db> {
        // TODO: ignoring r/R prefixes for now, should normalize bytes values
        Type::BytesLiteral(BytesLiteralType::new(
            self.db,
            literal.value.bytes().collect(),
        ))
    }

    fn infer_fstring_expression(&mut self, fstring: &ast::ExprFString) -> Type<'db> {
        let ast::ExprFString { range: _, value } = fstring;

        let mut collector = StringPartsCollector::new();
        for part in value {
            // Make sure we iter through every parts to infer all sub-expressions. The `collector`
            // struct ensures we don't allocate unnecessary strings.
            match part {
                ast::FStringPart::Literal(literal) => {
                    collector.push_str(&literal.value);
                }
                ast::FStringPart::FString(fstring) => {
                    for element in &fstring.elements {
                        match element {
                            ast::FStringElement::Expression(expression) => {
                                let ast::FStringExpressionElement {
                                    range: _,
                                    expression,
                                    debug_text: _,
                                    conversion,
                                    format_spec,
                                } = expression;
                                let ty = self.infer_expression(expression);

                                // TODO: handle format specifiers by calling a method
                                // (`Type::format`?) that handles the `__format__` method.
                                // Conversion flags should be handled before calling `__format__`.
                                // https://docs.python.org/3/library/string.html#format-string-syntax
                                if !conversion.is_none() || format_spec.is_some() {
                                    collector.add_expression();
                                } else {
                                    if let Type::StringLiteral(literal) = ty.str(self.db) {
                                        collector.push_str(literal.value(self.db));
                                    } else {
                                        collector.add_expression();
                                    }
                                }
                            }
                            ast::FStringElement::Literal(literal) => {
                                collector.push_str(&literal.value);
                            }
                        }
                    }
                }
            }
        }
        collector.ty(self.db)
    }

    fn infer_ellipsis_literal_expression(
        &mut self,
        _literal: &ast::ExprEllipsisLiteral,
    ) -> Type<'db> {
        builtins_symbol_ty(self.db, "Ellipsis")
    }

    fn infer_tuple_expression(&mut self, tuple: &ast::ExprTuple) -> Type<'db> {
        let ast::ExprTuple {
            range: _,
            elts,
            ctx: _,
            parenthesized: _,
        } = tuple;

        let element_types = elts
            .iter()
            .map(|elt| self.infer_expression(elt))
            .collect::<Vec<_>>();

        Type::Tuple(TupleType::new(self.db, element_types.into_boxed_slice()))
    }

    fn infer_list_expression(&mut self, list: &ast::ExprList) -> Type<'db> {
        let ast::ExprList {
            range: _,
            elts,
            ctx: _,
        } = list;

        for elt in elts {
            self.infer_expression(elt);
        }

        // TODO generic
        KnownClass::List.to_instance(self.db)
    }

    fn infer_set_expression(&mut self, set: &ast::ExprSet) -> Type<'db> {
        let ast::ExprSet { range: _, elts } = set;

        for elt in elts {
            self.infer_expression(elt);
        }

        // TODO generic
        KnownClass::Set.to_instance(self.db)
    }

    fn infer_dict_expression(&mut self, dict: &ast::ExprDict) -> Type<'db> {
        let ast::ExprDict { range: _, items } = dict;

        for item in items {
            self.infer_optional_expression(item.key.as_ref());
            self.infer_expression(&item.value);
        }

        // TODO generic
        KnownClass::Dict.to_instance(self.db)
    }

    /// Infer the type of the `iter` expression of the first comprehension.
    fn infer_first_comprehension_iter(&mut self, comprehensions: &[ast::Comprehension]) {
        let mut comprehensions_iter = comprehensions.iter();
        let Some(first_comprehension) = comprehensions_iter.next() else {
            unreachable!("Comprehension must contain at least one generator");
        };
        self.infer_expression(&first_comprehension.iter);
    }

    fn infer_generator_expression(&mut self, generator: &ast::ExprGenerator) -> Type<'db> {
        let ast::ExprGenerator {
            range: _,
            elt: _,
            generators,
            parenthesized: _,
        } = generator;

        self.infer_first_comprehension_iter(generators);

        // TODO generator type
        Type::Todo
    }

    fn infer_list_comprehension_expression(&mut self, listcomp: &ast::ExprListComp) -> Type<'db> {
        let ast::ExprListComp {
            range: _,
            elt: _,
            generators,
        } = listcomp;

        self.infer_first_comprehension_iter(generators);

        // TODO list type
        Type::Todo
    }

    fn infer_dict_comprehension_expression(&mut self, dictcomp: &ast::ExprDictComp) -> Type<'db> {
        let ast::ExprDictComp {
            range: _,
            key: _,
            value: _,
            generators,
        } = dictcomp;

        self.infer_first_comprehension_iter(generators);

        // TODO dict type
        Type::Todo
    }

    fn infer_set_comprehension_expression(&mut self, setcomp: &ast::ExprSetComp) -> Type<'db> {
        let ast::ExprSetComp {
            range: _,
            elt: _,
            generators,
        } = setcomp;

        self.infer_first_comprehension_iter(generators);

        // TODO set type
        Type::Todo
    }

    fn infer_generator_expression_scope(&mut self, generator: &ast::ExprGenerator) {
        let ast::ExprGenerator {
            range: _,
            elt,
            generators,
            parenthesized: _,
        } = generator;

        self.infer_expression(elt);
        self.infer_comprehensions(generators);
    }

    fn infer_list_comprehension_expression_scope(&mut self, listcomp: &ast::ExprListComp) {
        let ast::ExprListComp {
            range: _,
            elt,
            generators,
        } = listcomp;

        self.infer_expression(elt);
        self.infer_comprehensions(generators);
    }

    fn infer_dict_comprehension_expression_scope(&mut self, dictcomp: &ast::ExprDictComp) {
        let ast::ExprDictComp {
            range: _,
            key,
            value,
            generators,
        } = dictcomp;

        self.infer_expression(key);
        self.infer_expression(value);
        self.infer_comprehensions(generators);
    }

    fn infer_set_comprehension_expression_scope(&mut self, setcomp: &ast::ExprSetComp) {
        let ast::ExprSetComp {
            range: _,
            elt,
            generators,
        } = setcomp;

        self.infer_expression(elt);
        self.infer_comprehensions(generators);
    }

    fn infer_comprehensions(&mut self, comprehensions: &[ast::Comprehension]) {
        let mut comprehensions_iter = comprehensions.iter();
        let Some(first_comprehension) = comprehensions_iter.next() else {
            unreachable!("Comprehension must contain at least one generator");
        };
        self.infer_comprehension(first_comprehension, true);
        for comprehension in comprehensions_iter {
            self.infer_comprehension(comprehension, false);
        }
    }

    fn infer_comprehension(&mut self, comprehension: &ast::Comprehension, is_first: bool) {
        let ast::Comprehension {
            range: _,
            target,
            iter,
            ifs,
            is_async: _,
        } = comprehension;

        if !is_first {
            self.infer_expression(iter);
        }
        // TODO more complex assignment targets
        if let ast::Expr::Name(name) = target {
            self.infer_definition(name);
        } else {
            self.infer_expression(target);
        }
        for expr in ifs {
            self.infer_expression(expr);
        }
    }

    fn infer_comprehension_definition(
        &mut self,
        iterable: &ast::Expr,
        target: &ast::ExprName,
        is_first: bool,
        is_async: bool,
        definition: Definition<'db>,
    ) {
        let expression = self.index.expression(iterable);
        let result = infer_expression_types(self.db, expression);

        // Two things are different if it's the first comprehension:
        // (1) We must lookup the `ScopedExpressionId` of the iterable expression in the outer scope,
        //     because that's the scope we visit it in in the semantic index builder
        // (2) We must *not* call `self.extend()` on the result of the type inference,
        //     because `ScopedExpressionId`s are only meaningful within their own scope, so
        //     we'd add types for random wrong expressions in the current scope
        let iterable_ty = if is_first {
            let lookup_scope = self
                .index
                .parent_scope_id(self.scope.file_scope_id(self.db))
                .expect("A comprehension should never be the top-level scope")
                .to_scope_id(self.db, self.file);
            result.expression_ty(iterable.scoped_ast_id(self.db, lookup_scope))
        } else {
            self.extend(result);
            result.expression_ty(iterable.scoped_ast_id(self.db, self.scope))
        };

        let target_ty = if is_async {
            // TODO: async iterables/iterators! -- Alex
            Type::Todo
        } else {
            iterable_ty
                .iterate(self.db)
                .unwrap_with_diagnostic(iterable.into(), self)
        };

        self.types
            .expressions
            .insert(target.scoped_ast_id(self.db, self.scope), target_ty);
        self.add_binding(target.into(), definition, target_ty);
    }

    fn infer_named_expression(&mut self, named: &ast::ExprNamed) -> Type<'db> {
        let definition = self.index.definition(named);
        let result = infer_definition_types(self.db, definition);
        self.extend(result);
        result.binding_ty(definition)
    }

    fn infer_named_expression_definition(
        &mut self,
        named: &ast::ExprNamed,
        definition: Definition<'db>,
    ) -> Type<'db> {
        let ast::ExprNamed {
            range: _,
            target,
            value,
        } = named;

        let value_ty = self.infer_expression(value);
        self.infer_expression(target);

        self.add_binding(named.into(), definition, value_ty);

        value_ty
    }

    fn infer_if_expression(&mut self, if_expression: &ast::ExprIf) -> Type<'db> {
        let ast::ExprIf {
            range: _,
            test,
            body,
            orelse,
        } = if_expression;

        self.infer_expression(test);

        // TODO detect statically known truthy or falsy test
        let body_ty = self.infer_expression(body);
        let orelse_ty = self.infer_expression(orelse);

        UnionType::from_elements(self.db, [body_ty, orelse_ty])
    }

    fn infer_lambda_body(&mut self, lambda_expression: &ast::ExprLambda) {
        self.infer_expression(&lambda_expression.body);
    }

    fn infer_lambda_expression(&mut self, lambda_expression: &ast::ExprLambda) -> Type<'db> {
        let ast::ExprLambda {
            range: _,
            parameters,
            body: _,
        } = lambda_expression;

        if let Some(parameters) = parameters {
            for default in parameters
                .iter_non_variadic_params()
                .filter_map(|param| param.default.as_deref())
            {
                self.infer_expression(default);
            }

            self.infer_parameters(parameters);
        }

        // TODO function type
        Type::Todo
    }

    fn infer_call_expression(&mut self, call_expression: &ast::ExprCall) -> Type<'db> {
        let ast::ExprCall {
            range: _,
            func,
            arguments,
        } = call_expression;

        // TODO: proper typed call signature, representing keyword args etc
        let arg_types = self.infer_arguments(arguments);
        let function_type = self.infer_expression(func);
        function_type
            .call(self.db, arg_types.as_slice())
            .unwrap_with_diagnostic(self.db, func.as_ref().into(), self)
    }

    fn infer_starred_expression(&mut self, starred: &ast::ExprStarred) -> Type<'db> {
        let ast::ExprStarred {
            range: _,
            value,
            ctx: _,
        } = starred;

        let iterable_ty = self.infer_expression(value);
        iterable_ty
            .iterate(self.db)
            .unwrap_with_diagnostic(value.as_ref().into(), self);

        // TODO
        Type::Todo
    }

    fn infer_yield_expression(&mut self, yield_expression: &ast::ExprYield) -> Type<'db> {
        let ast::ExprYield { range: _, value } = yield_expression;

        self.infer_optional_expression(value.as_deref());

        // TODO awaitable type
        Type::Todo
    }

    fn infer_yield_from_expression(&mut self, yield_from: &ast::ExprYieldFrom) -> Type<'db> {
        let ast::ExprYieldFrom { range: _, value } = yield_from;

        let iterable_ty = self.infer_expression(value);
        iterable_ty
            .iterate(self.db)
            .unwrap_with_diagnostic(value.as_ref().into(), self);

        // TODO get type from `ReturnType` of generator
        Type::Todo
    }

    fn infer_await_expression(&mut self, await_expression: &ast::ExprAwait) -> Type<'db> {
        let ast::ExprAwait { range: _, value } = await_expression;

        self.infer_expression(value);

        // TODO awaitable type
        Type::Todo
    }

    /// Look up a name reference that isn't bound in the local scope.
    fn lookup_name(&mut self, name_node: &ast::ExprName) -> Type<'db> {
        let ast::ExprName { id: name, .. } = name_node;
        let file_scope_id = self.scope.file_scope_id(self.db);
        let is_bound = self
            .index
            .symbol_table(file_scope_id)
            .symbol_by_name(name)
            .expect("Symbol table should create a symbol for every Name node")
            .is_bound();

        // In function-like scopes, any local variable (symbol that is bound in this scope) can
        // only have a definition in this scope, or error; it never references another scope.
        // (At runtime, it would use the `LOAD_FAST` opcode.)
        if !is_bound || !self.scope.is_function_like(self.db) {
            // Walk up parent scopes looking for a possible enclosing scope that may have a
            // definition of this name visible to us (would be `LOAD_DEREF` at runtime.)
            for (enclosing_scope_file_id, _) in self.index.ancestor_scopes(file_scope_id) {
                // Class scopes are not visible to nested scopes, and we need to handle global
                // scope differently (because an unbound name there falls back to builtins), so
                // check only function-like scopes.
                let enclosing_scope_id = enclosing_scope_file_id.to_scope_id(self.db, self.file);
                if !enclosing_scope_id.is_function_like(self.db) {
                    continue;
                }
                let enclosing_symbol_table = self.index.symbol_table(enclosing_scope_file_id);
                let Some(enclosing_symbol) = enclosing_symbol_table.symbol_by_name(name) else {
                    continue;
                };
                if enclosing_symbol.is_bound() {
                    // We can return early here, because the nearest function-like scope that
                    // defines a name must be the only source for the nonlocal reference (at
                    // runtime, it is the scope that creates the cell for our closure.) If the name
                    // isn't bound in that scope, we should get an unbound name, not continue
                    // falling back to other scopes / globals / builtins.
                    return symbol_ty(self.db, enclosing_scope_id, name);
                }
            }
            // No nonlocal binding, check module globals. Avoid infinite recursion if `self.scope`
            // already is module globals.
            let ty = if file_scope_id.is_global() {
                Type::Unbound
            } else {
                global_symbol_ty(self.db, self.file, name)
            };
            // Fallback to builtins (without infinite recursion if we're already in builtins.)
            if ty.may_be_unbound(self.db) && Some(self.scope) != builtins_module_scope(self.db) {
                let mut builtin_ty = builtins_symbol_ty(self.db, name);
                if builtin_ty.is_unbound() && name == "reveal_type" {
                    self.add_diagnostic(
                        name_node.into(),
                        "undefined-reveal",
                        format_args!(
                            "`reveal_type` used without importing it; this is allowed for debugging convenience but will fail at runtime"),
                    );
                    builtin_ty = typing_extensions_symbol_ty(self.db, name);
                }
                ty.replace_unbound_with(self.db, builtin_ty)
            } else {
                ty
            }
        } else {
            Type::Unbound
        }
    }

    fn infer_name_expression(&mut self, name: &ast::ExprName) -> Type<'db> {
        let ast::ExprName { range: _, id, ctx } = name;
        let file_scope_id = self.scope.file_scope_id(self.db);

        match ctx {
            ExprContext::Load => {
                let use_def = self.index.use_def_map(file_scope_id);
                let symbol = self
                    .index
                    .symbol_table(file_scope_id)
                    .symbol_id_by_name(id)
                    .expect("Expected the symbol table to create a symbol for every Name node");
                // if we're inferring types of deferred expressions, always treat them as public symbols
                let (definitions, may_be_unbound) = if self.is_deferred() {
                    (
                        use_def.public_bindings(symbol),
                        use_def.public_may_be_unbound(symbol),
                    )
                } else {
                    let use_id = name.scoped_use_id(self.db, self.scope);
                    (
                        use_def.bindings_at_use(use_id),
                        use_def.use_may_be_unbound(use_id),
                    )
                };

                let unbound_ty = if may_be_unbound {
                    Some(self.lookup_name(name))
                } else {
                    None
                };

                bindings_ty(self.db, definitions, unbound_ty)
            }
            ExprContext::Store | ExprContext::Del => Type::None,
            ExprContext::Invalid => Type::Unknown,
        }
    }

    fn infer_attribute_expression(&mut self, attribute: &ast::ExprAttribute) -> Type<'db> {
        let ast::ExprAttribute {
            value,
            attr,
            range: _,
            ctx,
        } = attribute;

        let value_ty = self.infer_expression(value);
        let member_ty = value_ty.member(self.db, &ast::name::Name::new(&attr.id));

        match ctx {
            ExprContext::Load => member_ty,
            ExprContext::Store | ExprContext::Del => Type::None,
            ExprContext::Invalid => Type::Unknown,
        }
    }

    fn infer_unary_expression(&mut self, unary: &ast::ExprUnaryOp) -> Type<'db> {
        let ast::ExprUnaryOp {
            range: _,
            op,
            operand,
        } = unary;

        match (op, self.infer_expression(operand)) {
            (UnaryOp::UAdd, Type::IntLiteral(value)) => Type::IntLiteral(value),
            (UnaryOp::USub, Type::IntLiteral(value)) => Type::IntLiteral(-value),
            (UnaryOp::Invert, Type::IntLiteral(value)) => Type::IntLiteral(!value),

            (UnaryOp::UAdd, Type::BooleanLiteral(bool)) => Type::IntLiteral(i64::from(bool)),
            (UnaryOp::USub, Type::BooleanLiteral(bool)) => Type::IntLiteral(-i64::from(bool)),
            (UnaryOp::Invert, Type::BooleanLiteral(bool)) => Type::IntLiteral(!i64::from(bool)),

            (UnaryOp::Not, ty) => ty.bool(self.db).negate().into_type(self.db),

            _ => Type::Todo, // TODO other unary op types
        }
    }

    fn infer_binary_expression(&mut self, binary: &ast::ExprBinOp) -> Type<'db> {
        let ast::ExprBinOp {
            left,
            op,
            right,
            range: _,
        } = binary;

        let left_ty = self.infer_expression(left);
        let right_ty = self.infer_expression(right);

        // Check for division by zero; this doesn't change the inferred type for the expression, but
        // may emit a diagnostic
        if matches!(
            (op, right_ty),
            (
                ast::Operator::Div | ast::Operator::FloorDiv | ast::Operator::Mod,
                Type::IntLiteral(0),
            )
        ) {
            self.check_division_by_zero(binary, left_ty);
        }

        match (left_ty, right_ty, op) {
            // When interacting with Todo, Any and Unknown should propagate (as if we fix this
            // `Todo` in the future, the result would then become Any or Unknown, respectively.)
            (Type::Any, _, _) | (_, Type::Any, _) => Type::Any,
            (Type::Unknown, _, _) | (_, Type::Unknown, _) => Type::Unknown,

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::Add) => n
                .checked_add(m)
                .map(Type::IntLiteral)
                .unwrap_or_else(|| KnownClass::Int.to_instance(self.db)),

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::Sub) => n
                .checked_sub(m)
                .map(Type::IntLiteral)
                .unwrap_or_else(|| KnownClass::Int.to_instance(self.db)),

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::Mult) => n
                .checked_mul(m)
                .map(Type::IntLiteral)
                .unwrap_or_else(|| KnownClass::Int.to_instance(self.db)),

            (Type::IntLiteral(_), Type::IntLiteral(_), ast::Operator::Div) => {
                KnownClass::Float.to_instance(self.db)
            }

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::FloorDiv) => n
                .checked_div(m)
                .map(Type::IntLiteral)
                .unwrap_or_else(|| KnownClass::Int.to_instance(self.db)),

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::Mod) => n
                .checked_rem(m)
                .map(Type::IntLiteral)
                .unwrap_or_else(|| KnownClass::Int.to_instance(self.db)),

            (Type::BytesLiteral(lhs), Type::BytesLiteral(rhs), ast::Operator::Add) => {
                Type::BytesLiteral(BytesLiteralType::new(
                    self.db,
                    [lhs.value(self.db).as_ref(), rhs.value(self.db).as_ref()]
                        .concat()
                        .into_boxed_slice(),
                ))
            }

            (Type::StringLiteral(lhs), Type::StringLiteral(rhs), ast::Operator::Add) => {
                let lhs_value = lhs.value(self.db).to_string();
                let rhs_value = rhs.value(self.db).as_ref();
                if lhs_value.len() + rhs_value.len() <= Self::MAX_STRING_LITERAL_SIZE {
                    Type::StringLiteral(StringLiteralType::new(self.db, {
                        (lhs_value + rhs_value).into()
                    }))
                } else {
                    Type::LiteralString
                }
            }

            (
                Type::StringLiteral(_) | Type::LiteralString,
                Type::StringLiteral(_) | Type::LiteralString,
                ast::Operator::Add,
            ) => Type::LiteralString,

            (Type::StringLiteral(s), Type::IntLiteral(n), ast::Operator::Mult)
            | (Type::IntLiteral(n), Type::StringLiteral(s), ast::Operator::Mult) => {
                if n < 1 {
                    Type::StringLiteral(StringLiteralType::new(self.db, Box::default()))
                } else if let Ok(n) = usize::try_from(n) {
                    if n.checked_mul(s.value(self.db).len())
                        .is_some_and(|new_length| new_length <= Self::MAX_STRING_LITERAL_SIZE)
                    {
                        let new_literal = s.value(self.db).repeat(n);
                        Type::StringLiteral(StringLiteralType::new(self.db, new_literal.into()))
                    } else {
                        Type::LiteralString
                    }
                } else {
                    Type::LiteralString
                }
            }

            (Type::LiteralString, Type::IntLiteral(n), ast::Operator::Mult)
            | (Type::IntLiteral(n), Type::LiteralString, ast::Operator::Mult) => {
                if n < 1 {
                    Type::StringLiteral(StringLiteralType::new(self.db, Box::default()))
                } else {
                    Type::LiteralString
                }
            }

            _ => Type::Todo, // TODO
        }
    }

    fn infer_boolean_expression(&mut self, bool_op: &ast::ExprBoolOp) -> Type<'db> {
        let ast::ExprBoolOp {
            range: _,
            op,
            values,
        } = bool_op;
        Self::infer_chained_boolean_types(
            self.db,
            *op,
            values.iter().map(|value| self.infer_expression(value)),
            values.len(),
        )
    }

    /// Computes the output of a chain of (one) boolean operation, consuming as input an iterator
    /// of types. The iterator is consumed even if the boolean evaluation can be short-circuited,
    /// in order to ensure the invariant that all expressions are evaluated when inferring types.
    fn infer_chained_boolean_types(
        db: &'db dyn Db,
        op: ast::BoolOp,
        values: impl IntoIterator<Item = Type<'db>>,
        n_values: usize,
    ) -> Type<'db> {
        let mut done = false;
        UnionType::from_elements(
            db,
            values.into_iter().enumerate().map(|(i, ty)| {
                if done {
                    Type::Never
                } else {
                    let is_last = i == n_values - 1;
                    match (ty.bool(db), is_last, op) {
                        (Truthiness::Ambiguous, _, _) => ty,
                        (Truthiness::AlwaysTrue, false, ast::BoolOp::And) => Type::Never,
                        (Truthiness::AlwaysFalse, false, ast::BoolOp::Or) => Type::Never,
                        (Truthiness::AlwaysFalse, _, ast::BoolOp::And)
                        | (Truthiness::AlwaysTrue, _, ast::BoolOp::Or) => {
                            done = true;
                            ty
                        }
                        (_, true, _) => ty,
                    }
                }
            }),
        )
    }

    fn infer_compare_expression(&mut self, compare: &ast::ExprCompare) -> Type<'db> {
        let ast::ExprCompare {
            range: _,
            left,
            ops,
            comparators,
        } = compare;

        self.infer_expression(left);
        for right in comparators.as_ref() {
            self.infer_expression(right);
        }

        // https://docs.python.org/3/reference/expressions.html#comparisons
        // > Formally, if `a, b, c, …, y, z` are expressions and `op1, op2, …, opN` are comparison
        // > operators, then `a op1 b op2 c ... y opN z` is equivalent to a `op1 b and b op2 c and
        // ... > y opN z`, except that each expression is evaluated at most once.
        //
        // As some operators (==, !=, <, <=, >, >=) *can* return an arbitrary type, the logic below
        // is shared with the one in `infer_binary_type_comparison`.
        Self::infer_chained_boolean_types(
            self.db,
            ast::BoolOp::And,
            std::iter::once(left.as_ref())
                .chain(comparators.as_ref().iter())
                .tuple_windows::<(_, _)>()
                .zip(ops.iter())
                .map(|((left, right), op)| {
                    let left_ty = self.expression_ty(left);
                    let right_ty = self.expression_ty(right);

                    self.infer_binary_type_comparison(left_ty, *op, right_ty)
                        .unwrap_or_else(|| {
                            // Handle unsupported operators (diagnostic, `bool`/`Unknown` outcome)
                            self.add_diagnostic(
                                AnyNodeRef::ExprCompare(compare),
                                "operator-unsupported",
                                format_args!(
                                    "Operator `{}` is not supported for types `{}` and `{}`",
                                    op,
                                    left_ty.display(self.db),
                                    right_ty.display(self.db)
                                ),
                            );
                            match op {
                                // `in, not in, is, is not` always return bool instances
                                ast::CmpOp::In
                                | ast::CmpOp::NotIn
                                | ast::CmpOp::Is
                                | ast::CmpOp::IsNot => {
                                    builtins_symbol_ty(self.db, "bool").to_instance(self.db)
                                }
                                // Other operators can return arbitrary types
                                _ => Type::Unknown,
                            }
                        })
                }),
            ops.len(),
        )
    }

    /// Infers the type of a binary comparison (e.g. 'left == right'). See
    /// `infer_compare_expression` for the higher level logic dealing with multi-comparison
    /// expressions.
    ///
    /// If the operation is not supported, return None (we need upstream context to emit a
    /// diagnostic).
    fn infer_binary_type_comparison(
        &mut self,
        left: Type<'db>,
        op: ast::CmpOp,
        right: Type<'db>,
    ) -> Option<Type<'db>> {
        // Note: identity (is, is not) for equal builtin types is unreliable and not part of the
        // language spec.
        // - `[ast::CompOp::Is]`: return `false` if unequal, `bool` if equal
        // - `[ast::CompOp::IsNot]`: return `true` if unequal, `bool` if equal
        match (left, right) {
            (Type::IntLiteral(n), Type::IntLiteral(m)) => match op {
                ast::CmpOp::Eq => Some(Type::BooleanLiteral(n == m)),
                ast::CmpOp::NotEq => Some(Type::BooleanLiteral(n != m)),
                ast::CmpOp::Lt => Some(Type::BooleanLiteral(n < m)),
                ast::CmpOp::LtE => Some(Type::BooleanLiteral(n <= m)),
                ast::CmpOp::Gt => Some(Type::BooleanLiteral(n > m)),
                ast::CmpOp::GtE => Some(Type::BooleanLiteral(n >= m)),
                ast::CmpOp::Is => {
                    if n == m {
                        Some(builtins_symbol_ty(self.db, "bool").to_instance(self.db))
                    } else {
                        Some(Type::BooleanLiteral(false))
                    }
                }
                ast::CmpOp::IsNot => {
                    if n == m {
                        Some(builtins_symbol_ty(self.db, "bool").to_instance(self.db))
                    } else {
                        Some(Type::BooleanLiteral(true))
                    }
                }
                // Undefined for (int, int)
                ast::CmpOp::In | ast::CmpOp::NotIn => None,
            },
            (Type::IntLiteral(_), Type::Instance(_)) => {
                self.infer_binary_type_comparison(KnownClass::Int.to_instance(self.db), op, right)
            }
            (Type::Instance(_), Type::IntLiteral(_)) => {
                self.infer_binary_type_comparison(left, op, KnownClass::Int.to_instance(self.db))
            }
            // Booleans are coded as integers (False = 0, True = 1)
            (Type::IntLiteral(n), Type::BooleanLiteral(b)) => self.infer_binary_type_comparison(
                Type::IntLiteral(n),
                op,
                Type::IntLiteral(i64::from(b)),
            ),
            (Type::BooleanLiteral(b), Type::IntLiteral(m)) => self.infer_binary_type_comparison(
                Type::IntLiteral(i64::from(b)),
                op,
                Type::IntLiteral(m),
            ),
            (Type::BooleanLiteral(a), Type::BooleanLiteral(b)) => self
                .infer_binary_type_comparison(
                    Type::IntLiteral(i64::from(a)),
                    op,
                    Type::IntLiteral(i64::from(b)),
                ),
            // Lookup the rich comparison `__dunder__` methods on instances
            (Type::Instance(left_class_ty), Type::Instance(right_class_ty)) => match op {
                ast::CmpOp::Lt => {
                    perform_rich_comparison(self.db, left_class_ty, right_class_ty, "__lt__")
                }
                // TODO: implement mapping from `ast::CmpOp` to rich comparison methods
                _ => Some(Type::Todo),
            },
            // TODO: handle more types
            _ => Some(Type::Todo),
        }
    }

    fn infer_subscript_expression(&mut self, subscript: &ast::ExprSubscript) -> Type<'db> {
        let ast::ExprSubscript {
            range: _,
            value,
            slice,
            ctx: _,
        } = subscript;

        let value_ty = self.infer_expression(value);
        let slice_ty = self.infer_expression(slice);

        match (value_ty, slice_ty) {
            // Ex) Given `("a", "b", "c", "d")[1]`, return `"b"`
            (Type::Tuple(tuple_ty), Type::IntLiteral(int)) if int >= 0 => {
                let elements = tuple_ty.elements(self.db);
                usize::try_from(int)
                    .ok()
                    .and_then(|index| elements.get(index).copied())
                    .unwrap_or_else(|| {
                        self.tuple_index_out_of_bounds_diagnostic(
                            (&**value).into(),
                            value_ty,
                            elements.len(),
                            int,
                        );
                        Type::Unknown
                    })
            }
            // Ex) Given `("a", "b", "c", "d")[-1]`, return `"c"`
            (Type::Tuple(tuple_ty), Type::IntLiteral(int)) if int < 0 => {
                let elements = tuple_ty.elements(self.db);
                int.checked_neg()
                    .and_then(|int| usize::try_from(int).ok())
                    .and_then(|index| elements.len().checked_sub(index))
                    .and_then(|index| elements.get(index).copied())
                    .unwrap_or_else(|| {
                        self.tuple_index_out_of_bounds_diagnostic(
                            (&**value).into(),
                            value_ty,
                            elements.len(),
                            int,
                        );
                        Type::Unknown
                    })
            }
            // Ex) Given `("a", "b", "c", "d")[True]`, return `"b"`
            (Type::Tuple(tuple_ty), Type::BooleanLiteral(bool)) => {
                let elements = tuple_ty.elements(self.db);
                let int = i64::from(bool);
                elements.get(usize::from(bool)).copied().unwrap_or_else(|| {
                    self.tuple_index_out_of_bounds_diagnostic(
                        (&**value).into(),
                        value_ty,
                        elements.len(),
                        int,
                    );
                    Type::Unknown
                })
            }
            // Ex) Given `"value"[1]`, return `"a"`
            (Type::StringLiteral(literal_ty), Type::IntLiteral(int)) if int >= 0 => {
                let literal_value = literal_ty.value(self.db);
                usize::try_from(int)
                    .ok()
                    .and_then(|index| literal_value.chars().nth(index))
                    .map(|ch| {
                        Type::StringLiteral(StringLiteralType::new(
                            self.db,
                            ch.to_string().into_boxed_str(),
                        ))
                    })
                    .unwrap_or_else(|| {
                        self.string_index_out_of_bounds_diagnostic(
                            (&**value).into(),
                            value_ty,
                            literal_value.chars().count(),
                            int,
                        );
                        Type::Unknown
                    })
            }
            // Ex) Given `"value"[-1]`, return `"e"`
            (Type::StringLiteral(literal_ty), Type::IntLiteral(int)) if int < 0 => {
                let literal_value = literal_ty.value(self.db);
                int.checked_neg()
                    .and_then(|int| usize::try_from(int).ok())
                    .and_then(|index| index.checked_sub(1))
                    .and_then(|index| literal_value.chars().rev().nth(index))
                    .map(|ch| {
                        Type::StringLiteral(StringLiteralType::new(
                            self.db,
                            ch.to_string().into_boxed_str(),
                        ))
                    })
                    .unwrap_or_else(|| {
                        self.string_index_out_of_bounds_diagnostic(
                            (&**value).into(),
                            value_ty,
                            literal_value.chars().count(),
                            int,
                        );
                        Type::Unknown
                    })
            }
            // Ex) Given `"value"[True]`, return `"a"`
            (Type::StringLiteral(literal_ty), Type::BooleanLiteral(bool)) => {
                let literal_value = literal_ty.value(self.db);
                let int = i64::from(bool);
                literal_value
                    .chars()
                    .nth(usize::from(bool))
                    .map(|ch| {
                        Type::StringLiteral(StringLiteralType::new(
                            self.db,
                            ch.to_string().into_boxed_str(),
                        ))
                    })
                    .unwrap_or_else(|| {
                        self.string_index_out_of_bounds_diagnostic(
                            (&**value).into(),
                            value_ty,
                            literal_value.chars().count(),
                            int,
                        );
                        Type::Unknown
                    })
            }
            (value_ty, slice_ty) => {
                // Resolve the value to its class.
                let value_meta_ty = value_ty.to_meta_type(self.db);

                // If the class defines `__getitem__`, return its return type.
                //
                // See: https://docs.python.org/3/reference/datamodel.html#class-getitem-versus-getitem
                let dunder_getitem_method = value_meta_ty.member(self.db, "__getitem__");
                if !dunder_getitem_method.is_unbound() {
                    return dunder_getitem_method
                        .call(self.db, &[slice_ty])
                        .return_ty_result(self.db, value.as_ref().into(), self)
                        .unwrap_or_else(|err| {
                            self.add_diagnostic(
                                (&**value).into(),
                                "call-non-callable",
                                format_args!(
                                    "Method `__getitem__` of type `{}` is not callable on object of type `{}`",
                                    err.called_ty().display(self.db),
                                    value_ty.display(self.db),
                                ),
                            );
                            err.return_ty()
                        });
                }

                // Otherwise, if the value is itself a class and defines `__class_getitem__`,
                // return its return type.
                if value_ty.is_class(self.db) {
                    let dunder_class_getitem_method = value_ty.member(self.db, "__class_getitem__");
                    if !dunder_class_getitem_method.is_unbound() {
                        return dunder_class_getitem_method
                            .call(self.db, &[slice_ty])
                            .return_ty_result(self.db, value.as_ref().into(), self)
                            .unwrap_or_else(|err| {
                                self.add_diagnostic(
                                    (&**value).into(),
                                    "call-non-callable",
                                    format_args!(
                                        "Method `__class_getitem__` of type `{}` is not callable on object of type `{}`",
                                        err.called_ty().display(self.db),
                                        value_ty.display(self.db),
                                    ),
                                );
                                err.return_ty()
                            });
                    }

                    self.non_subscriptable_diagnostic(
                        (&**value).into(),
                        value_ty,
                        "__class_getitem__",
                    );
                } else {
                    self.non_subscriptable_diagnostic((&**value).into(), value_ty, "__getitem__");
                }

                Type::Unknown
            }
        }
    }

    fn infer_slice_expression(&mut self, slice: &ast::ExprSlice) -> Type<'db> {
        let ast::ExprSlice {
            range: _,
            lower,
            upper,
            step,
        } = slice;

        self.infer_optional_expression(lower.as_deref());
        self.infer_optional_expression(upper.as_deref());
        self.infer_optional_expression(step.as_deref());

        // TODO slice
        Type::Todo
    }

    fn infer_type_parameters(&mut self, type_parameters: &ast::TypeParams) {
        let ast::TypeParams {
            range: _,
            type_params,
        } = type_parameters;
        for type_param in type_params {
            match type_param {
                ast::TypeParam::TypeVar(typevar) => {
                    let ast::TypeParamTypeVar {
                        range: _,
                        name: _,
                        bound,
                        default,
                    } = typevar;
                    self.infer_optional_expression(bound.as_deref());
                    self.infer_optional_expression(default.as_deref());
                }
                ast::TypeParam::ParamSpec(param_spec) => {
                    let ast::TypeParamParamSpec {
                        range: _,
                        name: _,
                        default,
                    } = param_spec;
                    self.infer_optional_expression(default.as_deref());
                }
                ast::TypeParam::TypeVarTuple(typevar_tuple) => {
                    let ast::TypeParamTypeVarTuple {
                        range: _,
                        name: _,
                        default,
                    } = typevar_tuple;
                    self.infer_optional_expression(default.as_deref());
                }
            }
        }
    }

    /// Adds a new diagnostic.
    ///
    /// The diagnostic does not get added if the rule isn't enabled for this file.
    pub(super) fn add_diagnostic(
        &mut self,
        node: AnyNodeRef,
        rule: &str,
        message: std::fmt::Arguments,
    ) {
        if !self.db.is_file_open(self.file) {
            return;
        }

        // TODO: Don't emit the diagnostic if:
        // * The enclosing node contains any syntax errors
        // * The rule is disabled for this file. We probably want to introduce a new query that
        //   returns a rule selector for a given file that respects the package's settings,
        //   any global pragma comments in the file, and any per-file-ignores.

        self.types.diagnostics.push(TypeCheckDiagnostic {
            file: self.file,
            rule: rule.to_string(),
            message: message.to_string(),
            range: node.range(),
        });
    }

    pub(super) fn finish(mut self) -> TypeInference<'db> {
        self.infer_region();
        self.types.shrink_to_fit();
        self.types
    }
}

/// Annotation expressions.
impl<'db> TypeInferenceBuilder<'db> {
    fn infer_annotation_expression(&mut self, expression: &ast::Expr) -> Type<'db> {
        // https://typing.readthedocs.io/en/latest/spec/annotations.html#grammar-token-expression-grammar-annotation_expression
        match expression {
            // TODO: parse the expression and check whether it is a string annotation, since they
            // can be annotation expressions distinct from type expressions.
            // https://typing.readthedocs.io/en/latest/spec/annotations.html#string-annotations
            ast::Expr::StringLiteral(_literal) => Type::Todo,

            // Annotation expressions also get special handling for `*args` and `**kwargs`.
            ast::Expr::Starred(starred) => self.infer_starred_expression(starred),

            // All other annotation expressions are (possibly) valid type expressions, so handle
            // them there instead.
            type_expr => self.infer_type_expression(type_expr),
        }
    }
}

/// Type expressions
impl<'db> TypeInferenceBuilder<'db> {
    fn infer_type_expression(&mut self, expression: &ast::Expr) -> Type<'db> {
        // https://typing.readthedocs.io/en/latest/spec/annotations.html#grammar-token-expression-grammar-type_expression
        // TODO: this does not include any of the special forms, and is only a
        //   stub of the forms other than a standalone name in scope.

        let ty = match expression {
            ast::Expr::Name(name) => {
                debug_assert!(
                    name.ctx.is_load(),
                    "name in a type expression is always 'load' but got: '{:?}'",
                    name.ctx
                );

                self.infer_name_expression(name).to_instance(self.db)
            }

            ast::Expr::NoneLiteral(_literal) => Type::None,

            // TODO: parse the expression and check whether it is a string annotation.
            // https://typing.readthedocs.io/en/latest/spec/annotations.html#string-annotations
            ast::Expr::StringLiteral(_literal) => Type::Todo,

            // TODO: an Ellipsis literal *on its own* does not have any meaning in annotation
            // expressions, but is meaningful in the context of a number of special forms.
            ast::Expr::EllipsisLiteral(_literal) => Type::Todo,

            // Other literals do not have meaningful values in the annotation expression context.
            // However, we will we want to handle these differently when working with special forms,
            // since (e.g.) `123` is not valid in an annotation expression but `Literal[123]` is.
            ast::Expr::BytesLiteral(_literal) => Type::Todo,
            ast::Expr::NumberLiteral(_literal) => Type::Todo,
            ast::Expr::BooleanLiteral(_literal) => Type::Todo,

            // TODO: this may be a place we need to revisit with special forms.
            ast::Expr::Subscript(subscript) => {
                self.infer_subscript_expression(subscript);
                Type::Todo
            }

            // Forms which are invalid in the context of annotation expressions: we infer their
            // nested expressions as normal expressions, but the type of the top-level expression is
            // always `Type::Unknown` in these cases.
            ast::Expr::BoolOp(bool_op) => {
                self.infer_boolean_expression(bool_op);
                Type::Unknown
            }
            ast::Expr::Named(named) => {
                self.infer_named_expression(named);
                Type::Unknown
            }
            ast::Expr::BinOp(binary) => {
                self.infer_binary_expression(binary);
                Type::Unknown
            }
            ast::Expr::UnaryOp(unary) => {
                self.infer_unary_expression(unary);
                Type::Unknown
            }
            ast::Expr::Lambda(lambda_expression) => {
                self.infer_lambda_expression(lambda_expression);
                Type::Unknown
            }
            ast::Expr::If(if_expression) => {
                self.infer_if_expression(if_expression);
                Type::Unknown
            }
            ast::Expr::Dict(dict) => {
                self.infer_dict_expression(dict);
                Type::Unknown
            }
            ast::Expr::Set(set) => {
                self.infer_set_expression(set);
                Type::Unknown
            }
            ast::Expr::ListComp(listcomp) => {
                self.infer_list_comprehension_expression(listcomp);
                Type::Unknown
            }
            ast::Expr::SetComp(setcomp) => {
                self.infer_set_comprehension_expression(setcomp);
                Type::Unknown
            }
            ast::Expr::DictComp(dictcomp) => {
                self.infer_dict_comprehension_expression(dictcomp);
                Type::Unknown
            }
            ast::Expr::Generator(generator) => {
                self.infer_generator_expression(generator);
                Type::Unknown
            }
            ast::Expr::Await(await_expression) => {
                self.infer_await_expression(await_expression);
                Type::Unknown
            }
            ast::Expr::Yield(yield_expression) => {
                self.infer_yield_expression(yield_expression);
                Type::Unknown
            }
            ast::Expr::YieldFrom(yield_from) => {
                self.infer_yield_from_expression(yield_from);
                Type::Unknown
            }
            ast::Expr::Compare(compare) => {
                self.infer_compare_expression(compare);
                Type::Unknown
            }
            ast::Expr::Call(call_expr) => {
                self.infer_call_expression(call_expr);
                Type::Unknown
            }
            ast::Expr::FString(fstring) => {
                self.infer_fstring_expression(fstring);
                Type::Unknown
            }
            //
            ast::Expr::Attribute(attribute) => {
                self.infer_attribute_expression(attribute);
                Type::Unknown
            }
            ast::Expr::Starred(starred) => {
                self.infer_starred_expression(starred);
                Type::Unknown
            }
            ast::Expr::List(list) => {
                self.infer_list_expression(list);
                Type::Unknown
            }
            ast::Expr::Tuple(tuple) => {
                self.infer_tuple_expression(tuple);
                Type::Unknown
            }
            ast::Expr::Slice(slice) => {
                self.infer_slice_expression(slice);
                Type::Unknown
            }

            ast::Expr::IpyEscapeCommand(_) => todo!("Implement Ipy escape command support"),
        };

        let expr_id = expression.scoped_ast_id(self.db, self.scope);
        let previous = self.types.expressions.insert(expr_id, ty);
        assert!(previous.is_none());

        ty
    }
}

fn format_import_from_module(level: u32, module: Option<&str>) -> String {
    format!(
        "{}{}",
        ".".repeat(level as usize),
        module.unwrap_or_default()
    )
}

/// Various ways in which resolving a [`ModuleName`]
/// from an [`ast::StmtImport`] or [`ast::StmtImportFrom`] node might fail
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ModuleNameResolutionError {
    /// The import statement has invalid syntax
    InvalidSyntax,

    /// We couldn't resolve the file we're currently analyzing back to a module
    /// (Only necessary for relative import statements)
    UnknownCurrentModule,

    /// The relative import statement seems to take us outside of the module search path
    /// (e.g. our current module is `foo.bar`, and the relative import statement in `foo.bar`
    /// is `from ....baz import spam`)
    TooManyDots,
}

/// Struct collecting string parts when inferring a formatted string. Infers a string literal if the
/// concatenated string is small enough, otherwise infers a literal string.
///
/// If the formatted string contains an expression (with a representation unknown at compile time),
/// infers an instance of `builtins.str`.
struct StringPartsCollector {
    concatenated: Option<String>,
    expression: bool,
}

impl StringPartsCollector {
    fn new() -> Self {
        Self {
            concatenated: Some(String::new()),
            expression: false,
        }
    }

    fn push_str(&mut self, literal: &str) {
        if let Some(mut concatenated) = self.concatenated.take() {
            if concatenated.len().saturating_add(literal.len())
                <= TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE
            {
                concatenated.push_str(literal);
                self.concatenated = Some(concatenated);
            } else {
                self.concatenated = None;
            }
        }
    }

    fn add_expression(&mut self) {
        self.concatenated = None;
        self.expression = true;
    }

    fn ty(self, db: &dyn Db) -> Type {
        if self.expression {
            KnownClass::Str.to_instance(db)
        } else if let Some(concatenated) = self.concatenated {
            Type::StringLiteral(StringLiteralType::new(db, concatenated.into_boxed_str()))
        } else {
            Type::LiteralString
        }
    }
}

/// Rich comparison in Python are the operators `==`, `!=`, `<`, `<=`, `>`, and `>=`. Their
/// behaviour can be edited for classes by implementing corresponding dunder methods.
/// This function performs rich comparison between two  instances and returns the resulting type.
/// see `<https://docs.python.org/3/reference/datamodel.html#object.__lt__>`
fn perform_rich_comparison<'db>(
    db: &'db dyn Db,
    left: ClassType<'db>,
    right: ClassType<'db>,
    dunder_name: &str,
) -> Option<Type<'db>> {
    // The following resource has details about the rich comparison algorithm:
    // https://snarky.ca/unravelling-rich-comparison-operators/
    //
    // TODO: the reflected dunder actually has priority if the r.h.s. is a strict subclass of the
    // l.h.s.
    // TODO: `object.__ne__` will call `__eq__` if `__ne__` is not defined

    let dunder = left.class_member(db, dunder_name);
    if !dunder.is_unbound() {
        // TODO: this currently gives the return type even if the arg types are invalid
        // (e.g. int.__lt__ with string instance should be None, currently bool)
        return dunder
            .call(db, &[Type::Instance(left), Type::Instance(right)])
            .return_ty(db);
    }

    // TODO: reflected dunder -- (==, ==), (!=, !=), (<, >), (>, <), (<=, >=), (>=, <=)
    None
}

#[cfg(test)]
mod tests {

    use anyhow::Context;

    use crate::db::tests::TestDb;
    use crate::program::{Program, SearchPathSettings};
    use crate::python_version::PythonVersion;
    use crate::semantic_index::definition::Definition;
    use crate::semantic_index::symbol::FileScopeId;
    use crate::semantic_index::{global_scope, semantic_index, symbol_table, use_def_map};
    use crate::stdlib::builtins_module_scope;
    use crate::types::{
        check_types, global_symbol_ty, infer_definition_types, symbol_ty, TypeCheckDiagnostics,
    };
    use crate::{HasTy, ProgramSettings, SemanticModel};
    use ruff_db::files::{system_path_to_file, File};
    use ruff_db::parsed::parsed_module;
    use ruff_db::system::{DbWithTestSystem, SystemPathBuf};
    use ruff_db::testing::assert_function_query_was_not_run;
    use ruff_python_ast::name::Name;

    use super::TypeInferenceBuilder;

    fn setup_db() -> TestDb {
        let db = TestDb::new();

        let src_root = SystemPathBuf::from("/src");
        db.memory_file_system()
            .create_directory_all(&src_root)
            .unwrap();

        Program::from_settings(
            &db,
            &ProgramSettings {
                target_version: PythonVersion::default(),
                search_paths: SearchPathSettings::new(src_root),
            },
        )
        .expect("Valid search path settings");

        db
    }

    fn setup_db_with_custom_typeshed<'a>(
        typeshed: &str,
        files: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> anyhow::Result<TestDb> {
        let mut db = TestDb::new();
        let src_root = SystemPathBuf::from("/src");

        db.write_files(files)
            .context("Failed to write test files")?;

        Program::from_settings(
            &db,
            &ProgramSettings {
                target_version: PythonVersion::default(),
                search_paths: SearchPathSettings {
                    custom_typeshed: Some(SystemPathBuf::from(typeshed)),
                    ..SearchPathSettings::new(src_root)
                },
            },
        )
        .context("Failed to create Program")?;

        Ok(db)
    }

    fn assert_public_ty(db: &TestDb, file_name: &str, symbol_name: &str, expected: &str) {
        let file = system_path_to_file(db, file_name).expect("file to exist");

        let ty = global_symbol_ty(db, file, symbol_name);
        assert_eq!(
            ty.display(db).to_string(),
            expected,
            "Mismatch for symbol '{symbol_name}' in '{file_name}'"
        );
    }

    fn assert_scope_ty(
        db: &TestDb,
        file_name: &str,
        scopes: &[&str],
        symbol_name: &str,
        expected: &str,
    ) {
        let file = system_path_to_file(db, file_name).expect("file to exist");
        let index = semantic_index(db, file);
        let mut file_scope_id = FileScopeId::global();
        let mut scope = file_scope_id.to_scope_id(db, file);
        for expected_scope_name in scopes {
            file_scope_id = index
                .child_scopes(file_scope_id)
                .next()
                .unwrap_or_else(|| panic!("scope of {expected_scope_name}"))
                .0;
            scope = file_scope_id.to_scope_id(db, file);
            assert_eq!(scope.name(db), *expected_scope_name);
        }

        let ty = symbol_ty(db, scope, symbol_name);
        assert_eq!(ty.display(db).to_string(), expected);
    }

    fn assert_diagnostic_messages(diagnostics: &TypeCheckDiagnostics, expected: &[&str]) {
        let messages: Vec<&str> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message())
            .collect();
        assert_eq!(&messages, expected);
    }

    fn assert_file_diagnostics(db: &TestDb, filename: &str, expected: &[&str]) {
        let file = system_path_to_file(db, filename).unwrap();
        let diagnostics = check_types(db, file);

        assert_diagnostic_messages(&diagnostics, expected);
    }

    #[test]
    fn reveal_type() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            from typing import reveal_type

            x = 1
            reveal_type(x)
            ",
        )?;

        assert_file_diagnostics(&db, "/src/a.py", &["Revealed type is `Literal[1]`"]);

        Ok(())
    }

    #[test]
    fn reveal_type_aliased() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            from typing import reveal_type as rt

            x = 1
            rt(x)
            ",
        )?;

        assert_file_diagnostics(&db, "/src/a.py", &["Revealed type is `Literal[1]`"]);

        Ok(())
    }

    #[test]
    fn reveal_type_typing_extensions() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            import typing_extensions

            x = 1
            typing_extensions.reveal_type(x)
            ",
        )?;

        assert_file_diagnostics(&db, "/src/a.py", &["Revealed type is `Literal[1]`"]);

        Ok(())
    }

    #[test]
    fn reveal_type_builtin() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            x = 1
            reveal_type(x)
            ",
        )?;

        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &[
                "`reveal_type` used without importing it; this is allowed for debugging convenience but will fail at runtime",
                "Revealed type is `Literal[1]`",
            ],
        );

        Ok(())
    }

    #[test]
    fn follow_import_to_class() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("src/a.py", "from b import C as D; E = D"),
            ("src/b.py", "class C: pass"),
        ])?;

        assert_public_ty(&db, "src/a.py", "E", "Literal[C]");

        Ok(())
    }

    #[test]
    fn follow_relative_import_simple() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("src/package/__init__.py", ""),
            ("src/package/foo.py", "X = 42"),
            ("src/package/bar.py", "from .foo import X"),
        ])?;

        assert_public_ty(&db, "src/package/bar.py", "X", "Literal[42]");

        Ok(())
    }

    #[test]
    fn follow_nonexistent_relative_import_simple() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("src/package/__init__.py", ""),
            ("src/package/bar.py", "from .foo import X"),
        ])?;

        assert_public_ty(&db, "src/package/bar.py", "X", "Unknown");

        Ok(())
    }

    #[test]
    fn follow_relative_import_dotted() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("src/package/__init__.py", ""),
            ("src/package/foo/bar/baz.py", "X = 42"),
            ("src/package/bar.py", "from .foo.bar.baz import X"),
        ])?;

        assert_public_ty(&db, "src/package/bar.py", "X", "Literal[42]");

        Ok(())
    }

    #[test]
    fn follow_relative_import_bare_to_package() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("src/package/__init__.py", "X = 42"),
            ("src/package/bar.py", "from . import X"),
        ])?;

        assert_public_ty(&db, "src/package/bar.py", "X", "Literal[42]");

        Ok(())
    }

    #[test]
    fn follow_nonexistent_relative_import_bare_to_package() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_files([("src/package/bar.py", "from . import X")])?;
        assert_public_ty(&db, "src/package/bar.py", "X", "Unknown");
        Ok(())
    }

    #[ignore = "TODO: Submodule imports possibly not supported right now?"]
    #[test]
    fn follow_relative_import_bare_to_module() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("src/package/__init__.py", ""),
            ("src/package/foo.py", "X = 42"),
            ("src/package/bar.py", "from . import foo; y = foo.X"),
        ])?;

        assert_public_ty(&db, "src/package/bar.py", "y", "Literal[42]");

        Ok(())
    }

    #[ignore = "TODO: Submodule imports possibly not supported right now?"]
    #[test]
    fn follow_nonexistent_import_bare_to_module() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("src/package/__init__.py", ""),
            ("src/package/bar.py", "from . import foo"),
        ])?;

        assert_public_ty(&db, "src/package/bar.py", "foo", "Unknown");

        Ok(())
    }

    #[test]
    fn follow_relative_import_from_dunder_init() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("src/package/__init__.py", "from .foo import X"),
            ("src/package/foo.py", "X = 42"),
        ])?;

        assert_public_ty(&db, "src/package/__init__.py", "X", "Literal[42]");

        Ok(())
    }

    #[test]
    fn follow_nonexistent_relative_import_from_dunder_init() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_files([("src/package/__init__.py", "from .foo import X")])?;
        assert_public_ty(&db, "src/package/__init__.py", "X", "Unknown");
        Ok(())
    }

    #[test]
    fn follow_very_relative_import() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("src/package/__init__.py", ""),
            ("src/package/foo.py", "X = 42"),
            (
                "src/package/subpackage/subsubpackage/bar.py",
                "from ...foo import X",
            ),
        ])?;

        assert_public_ty(
            &db,
            "src/package/subpackage/subsubpackage/bar.py",
            "X",
            "Literal[42]",
        );

        Ok(())
    }

    #[test]
    fn imported_unbound_symbol_is_unknown() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("src/package/__init__.py", ""),
            ("src/package/foo.py", "x"),
            ("src/package/bar.py", "from package.foo import x"),
        ])?;

        // the type as seen from external modules (`Unknown`)
        // is different from the type inside the module itself (`Unbound`):
        assert_public_ty(&db, "src/package/foo.py", "x", "Unbound");
        assert_public_ty(&db, "src/package/bar.py", "x", "Unknown");

        Ok(())
    }

    #[test]
    fn from_import_with_no_module_name() -> anyhow::Result<()> {
        // This test checks that invalid syntax in a `StmtImportFrom` node
        // leads to the type being inferred as `Unknown`
        let mut db = setup_db();
        db.write_file("src/foo.py", "from import bar")?;
        assert_public_ty(&db, "src/foo.py", "bar", "Unknown");
        Ok(())
    }

    #[test]
    fn resolve_base_class_by_name() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/mod.py",
            "
            class Base:
                pass

            class Sub(Base):
                pass
            ",
        )?;

        let mod_file = system_path_to_file(&db, "src/mod.py").expect("file to exist");
        let ty = global_symbol_ty(&db, mod_file, "Sub");

        let class = ty.expect_class();

        let base_names: Vec<_> = class
            .bases(&db)
            .map(|base_ty| format!("{}", base_ty.display(&db)))
            .collect();

        assert_eq!(base_names, vec!["Literal[Base]"]);

        Ok(())
    }

    #[test]
    fn resolve_method() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/mod.py",
            "
            class C:
                def f(self): pass
            ",
        )?;

        let mod_file = system_path_to_file(&db, "src/mod.py").unwrap();
        let ty = global_symbol_ty(&db, mod_file, "C");
        let class_id = ty.expect_class();
        let member_ty = class_id.class_member(&db, &Name::new_static("f"));
        let func = member_ty.expect_function();

        assert_eq!(func.name(&db), "f");
        Ok(())
    }

    #[test]
    fn resolve_module_member() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("src/a.py", "import b; D = b.C"),
            ("src/b.py", "class C: pass"),
        ])?;

        assert_public_ty(&db, "src/a.py", "D", "Literal[C]");

        Ok(())
    }

    #[test]
    fn number_literal() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            a = 1
            b = 9223372036854775808
            c = 1.45
            d = 2j
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "a", "Literal[1]");
        assert_public_ty(&db, "src/a.py", "b", "int");
        assert_public_ty(&db, "src/a.py", "c", "float");
        assert_public_ty(&db, "src/a.py", "d", "complex");

        Ok(())
    }

    #[test]
    fn negated_int_literal() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            x = -1
            y = -1234567890987654321
            z = --987
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[-1]");
        assert_public_ty(&db, "src/a.py", "y", "Literal[-1234567890987654321]");
        assert_public_ty(&db, "src/a.py", "z", "Literal[987]");

        Ok(())
    }

    #[test]
    fn boolean_literal() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file("src/a.py", "x = True\ny = False")?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[True]");
        assert_public_ty(&db, "src/a.py", "y", "Literal[False]");

        Ok(())
    }

    #[test]
    fn not_none_literal() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file(
            "src/a.py",
            r#"
            a = not None
            b = not not None
            "#,
        )?;
        assert_public_ty(&db, "src/a.py", "a", "Literal[True]");
        assert_public_ty(&db, "src/a.py", "b", "Literal[False]");

        Ok(())
    }

    #[test]
    fn not_function() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file(
            "src/a.py",
            r#"
            from typing import reveal_type
            def f():
                return 1

            a = not f
            b = not reveal_type
            "#,
        )?;

        assert_public_ty(&db, "src/a.py", "a", "Literal[False]");
        // TODO Unknown should not be part of the type of typing.reveal_type
        // assert_public_ty(&db, "src/a.py", "b", "Literal[False]");
        Ok(())
    }

    #[test]
    fn not_module() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            (
                "src/a.py",
                "import b; import warnings;
                x = not b;
                z = not warnings",
            ),
            ("src/b.py", "y = 1"),
        ])?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[False]");
        assert_public_ty(&db, "src/a.py", "z", "Literal[False]");

        Ok(())
    }

    #[test]
    fn not_union() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file(
            "src/a.py",
            r#"
            if flag:
                p = 1
                q = 3.3
                r = "hello"
                s = "world"
                t = 0
            else:
                p = "hello"
                q = 4
                r = ""
                s = 0
                t = ""

            a = not p
            b = not q
            c = not r
            d = not s
            e = not t
            "#,
        )?;

        assert_public_ty(&db, "src/a.py", "a", "Literal[False]");
        assert_public_ty(&db, "src/a.py", "b", "bool");
        assert_public_ty(&db, "src/a.py", "c", "bool");
        assert_public_ty(&db, "src/a.py", "d", "bool");
        assert_public_ty(&db, "src/a.py", "e", "Literal[True]");

        Ok(())
    }

    #[test]
    fn not_integer_literal() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file(
            "src/a.py",
            r#"
            a = not 1
            b = not 1234567890987654321
            e = not 0
            x = not -1
            y = not -1234567890987654321
            z = not --987
            "#,
        )?;
        assert_public_ty(&db, "src/a.py", "a", "Literal[False]");
        assert_public_ty(&db, "src/a.py", "b", "Literal[False]");
        assert_public_ty(&db, "src/a.py", "e", "Literal[True]");
        assert_public_ty(&db, "src/a.py", "x", "Literal[False]");
        assert_public_ty(&db, "src/a.py", "y", "Literal[False]");
        assert_public_ty(&db, "src/a.py", "z", "Literal[False]");

        Ok(())
    }

    #[test]
    fn not_boolean_literal() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file(
            "src/a.py",
            r#"
            w = True
            x = False
            y = not w
            z = not x

            "#,
        )?;
        assert_public_ty(&db, "src/a.py", "w", "Literal[True]");
        assert_public_ty(&db, "src/a.py", "x", "Literal[False]");
        assert_public_ty(&db, "src/a.py", "y", "Literal[False]");
        assert_public_ty(&db, "src/a.py", "z", "Literal[True]");

        Ok(())
    }

    #[test]
    fn not_string_literal() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file(
            "src/a.py",
            r#"
            a = not "hello"
            b = not ""
            c = not "0"
            d = not "hello" + "world"
            "#,
        )?;
        assert_public_ty(&db, "src/a.py", "a", "Literal[False]");
        assert_public_ty(&db, "src/a.py", "b", "Literal[True]");
        assert_public_ty(&db, "src/a.py", "c", "Literal[False]");
        assert_public_ty(&db, "src/a.py", "d", "Literal[False]");

        Ok(())
    }

    #[test]
    fn not_literal_string() -> anyhow::Result<()> {
        let mut db = setup_db();
        let content = format!(
            r#"
        v = not "{y}"
        w = not 10*"{y}"
        x = not "{y}"*10
        z = not 0*"{y}"
        u = not (-100)*"{y}"
        "#,
            y = "a".repeat(TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE + 1),
        );
        db.write_dedented("src/a.py", &content)?;

        assert_public_ty(&db, "src/a.py", "v", "bool");
        assert_public_ty(&db, "src/a.py", "w", "bool");
        assert_public_ty(&db, "src/a.py", "x", "bool");
        assert_public_ty(&db, "src/a.py", "z", "Literal[True]");
        assert_public_ty(&db, "src/a.py", "u", "Literal[True]");

        Ok(())
    }

    #[test]
    fn not_bytes_literal() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file(
            "src/a.py",
            r#"
            a = not b"hello"
            b = not b""
            c = not b"0"
            d = not b"hello" + b"world"
            "#,
        )?;
        assert_public_ty(&db, "src/a.py", "a", "Literal[False]");
        assert_public_ty(&db, "src/a.py", "b", "Literal[True]");
        assert_public_ty(&db, "src/a.py", "c", "Literal[False]");
        assert_public_ty(&db, "src/a.py", "d", "Literal[False]");

        Ok(())
    }

    #[test]
    fn not_tuple() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file(
            "src/a.py",
            r#"
            a = not (1,)
            b = not (1, 2)
            c = not (1, 2, 3)
            d = not ()
            e = not ("hello",)
            f = not (1, "hello")
            "#,
        )?;

        assert_public_ty(&db, "src/a.py", "a", "Literal[False]");
        assert_public_ty(&db, "src/a.py", "b", "Literal[False]");
        assert_public_ty(&db, "src/a.py", "c", "Literal[False]");
        assert_public_ty(&db, "src/a.py", "d", "Literal[True]");
        assert_public_ty(&db, "src/a.py", "e", "Literal[False]");
        assert_public_ty(&db, "src/a.py", "f", "Literal[False]");

        Ok(())
    }

    #[test]
    fn string_type() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            r#"
            w = "Hello"
            x = 'world'
            y = "Guten " + 'tag'
            z = 'bon ' + "jour"
            "#,
        )?;

        assert_public_ty(&db, "src/a.py", "w", r#"Literal["Hello"]"#);
        assert_public_ty(&db, "src/a.py", "x", r#"Literal["world"]"#);
        assert_public_ty(&db, "src/a.py", "y", r#"Literal["Guten tag"]"#);
        assert_public_ty(&db, "src/a.py", "z", r#"Literal["bon jour"]"#);

        Ok(())
    }

    #[test]
    fn string_type_with_nested_quotes() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            r#"
            x = 'I say "hello" to you'
            y = "You say \"hey\" back"
            z = 'No "closure here'
            "#,
        )?;

        assert_public_ty(&db, "src/a.py", "x", r#"Literal["I say \"hello\" to you"]"#);
        assert_public_ty(&db, "src/a.py", "y", r#"Literal["You say \"hey\" back"]"#);
        assert_public_ty(&db, "src/a.py", "z", r#"Literal["No \"closure here"]"#);

        Ok(())
    }

    #[test]
    fn multiplied_string() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            &format!(
                r#"
            w = 2 * "hello"
            x = "goodbye" * 3
            y = "a" * {y}
            z = {z} * "b"
            a = 0 * "hello"
            b = -3 * "hello"
            "#,
                y = TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE,
                z = TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE + 1
            ),
        )?;

        assert_public_ty(&db, "src/a.py", "w", r#"Literal["hellohello"]"#);
        assert_public_ty(&db, "src/a.py", "x", r#"Literal["goodbyegoodbyegoodbye"]"#);
        assert_public_ty(
            &db,
            "src/a.py",
            "y",
            &format!(
                r#"Literal["{}"]"#,
                "a".repeat(TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE)
            ),
        );
        assert_public_ty(&db, "src/a.py", "z", "LiteralString");
        assert_public_ty(&db, "src/a.py", "a", r#"Literal[""]"#);
        assert_public_ty(&db, "src/a.py", "b", r#"Literal[""]"#);

        Ok(())
    }

    #[test]
    fn multiplied_literal_string() -> anyhow::Result<()> {
        let mut db = setup_db();
        let content = format!(
            r#"
        v = "{y}"
        w = 10*"{y}"
        x = "{y}"*10
        z = 0*"{y}"
        u = (-100)*"{y}"
        "#,
            y = "a".repeat(TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE + 1),
        );
        db.write_dedented("src/a.py", &content)?;

        assert_public_ty(&db, "src/a.py", "v", "LiteralString");
        assert_public_ty(&db, "src/a.py", "w", "LiteralString");
        assert_public_ty(&db, "src/a.py", "x", "LiteralString");
        assert_public_ty(&db, "src/a.py", "z", r#"Literal[""]"#);
        assert_public_ty(&db, "src/a.py", "u", r#"Literal[""]"#);
        Ok(())
    }

    #[test]
    fn truncated_string_literals_become_literal_string() -> anyhow::Result<()> {
        let mut db = setup_db();
        let content = format!(
            r#"
        w = "{y}"
        x = "a" + "{z}"
        "#,
            y = "a".repeat(TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE + 1),
            z = "a".repeat(TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE),
        );
        db.write_dedented("src/a.py", &content)?;

        assert_public_ty(&db, "src/a.py", "w", "LiteralString");
        assert_public_ty(&db, "src/a.py", "x", "LiteralString");

        Ok(())
    }

    #[test]
    fn adding_string_literals_and_literal_string() -> anyhow::Result<()> {
        let mut db = setup_db();
        let content = format!(
            r#"
        v = "{y}"
        w = "{y}" + "a"
        x = "a" + "{y}"
        z = "{y}" + "{y}"
        "#,
            y = "a".repeat(TypeInferenceBuilder::MAX_STRING_LITERAL_SIZE + 1),
        );
        db.write_dedented("src/a.py", &content)?;

        assert_public_ty(&db, "src/a.py", "v", "LiteralString");
        assert_public_ty(&db, "src/a.py", "w", "LiteralString");
        assert_public_ty(&db, "src/a.py", "x", "LiteralString");
        assert_public_ty(&db, "src/a.py", "z", "LiteralString");

        Ok(())
    }

    #[test]
    fn comparison_integer_literals() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "src/a.py",
            r#"
            a = 1 == 1 == True
            b = 1 == 1 == 2 == 4
            c = False < True <= 2 < 3 != 6
            d = 1 < 1
            e = 1 > 1
            f = 1 is 1
            g = 1 is not 1
            h = 1 is 2
            i = 1 is not 7
            j = 1 <= "" and 0 < 1
            "#,
        )?;

        assert_public_ty(&db, "src/a.py", "a", "Literal[True]");
        assert_public_ty(&db, "src/a.py", "b", "Literal[False]");
        assert_public_ty(&db, "src/a.py", "c", "Literal[True]");
        assert_public_ty(&db, "src/a.py", "d", "Literal[False]");
        assert_public_ty(&db, "src/a.py", "e", "Literal[False]");
        assert_public_ty(&db, "src/a.py", "f", "bool");
        assert_public_ty(&db, "src/a.py", "g", "bool");
        assert_public_ty(&db, "src/a.py", "h", "Literal[False]");
        assert_public_ty(&db, "src/a.py", "i", "Literal[True]");
        assert_public_ty(&db, "src/a.py", "j", "@Todo | Literal[True]");

        Ok(())
    }

    #[test]
    fn comparison_integer_instance() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            r#"
            def int_instance() -> int: ...
            a = 1 == int_instance()
            b = 9 < int_instance()
            c = int_instance() < int_instance()
            "#,
        )?;

        // TODO: implement lookup of `__eq__` on typeshed `int` stub
        assert_public_ty(&db, "src/a.py", "a", "@Todo");
        assert_public_ty(&db, "src/a.py", "b", "bool");
        assert_public_ty(&db, "src/a.py", "c", "bool");

        Ok(())
    }

    #[test]
    fn comparison_unsupported_operators() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "src/a.py",
            r#"
            a = 1 in 7
            b = 0 not in 10
            c = object() < 5
            d = 5 < object()
            "#,
        )?;

        assert_file_diagnostics(
            &db,
            "src/a.py",
            &[
                "Operator `in` is not supported for types `Literal[1]` and `Literal[7]`",
                "Operator `not in` is not supported for types `Literal[0]` and `Literal[10]`",
                "Operator `<` is not supported for types `object` and `Literal[5]`",
            ],
        );
        assert_public_ty(&db, "src/a.py", "a", "bool");
        assert_public_ty(&db, "src/a.py", "b", "bool");
        assert_public_ty(&db, "src/a.py", "c", "Unknown");
        // TODO: this should be `Unknown` but we don't check if __lt__ signature is valid for right
        // operand type
        assert_public_ty(&db, "src/a.py", "d", "bool");

        Ok(())
    }

    #[test]
    fn comparison_non_bool_returns() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "src/a.py",
            r#"
            from __future__ import annotations
            class A:
                def __lt__(self, other) -> A: ...
            class B:
                def __lt__(self, other) -> B: ...
            class C:
                def __lt__(self, other) -> C: ...

            a = A() < B() < C()
            b = 0 < 1 < A() < 3
            c = 10 < 0 < A() < B() < C()
            "#,
        )?;

        // Walking through the example
        // 1. A() < B() < C()
        // 2. A() < B() and B() < C()  - split in N comparison
        // 3. A() and B()              - evaluate outcome types
        // 4. bool and bool            - evaluate truthiness
        // 5. A | B                    - union of "first true" types
        assert_public_ty(&db, "src/a.py", "a", "A | B");
        // Walking through the example
        // 1. 0 < 1 < A() < 3
        // 2. 0 < 1 and 1 < A() and A() < 3   - split in N comparison
        // 3. True and bool and A             - evaluate outcome types
        // 4. True and bool and bool          - evaluate truthiness
        // 5. bool | A                        - union of "true" types
        assert_public_ty(&db, "src/a.py", "b", "bool | A");
        // Short-cicuit to False
        assert_public_ty(&db, "src/a.py", "c", "Literal[False]");

        Ok(())
    }

    #[test]
    fn bytes_type() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            w = b'red' b'knot'
            x = b'hello'
            y = b'world' + b'!'
            z = b'\\xff\\x00'
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "w", "Literal[b\"redknot\"]");
        assert_public_ty(&db, "src/a.py", "x", "Literal[b\"hello\"]");
        assert_public_ty(&db, "src/a.py", "y", "Literal[b\"world!\"]");
        assert_public_ty(&db, "src/a.py", "z", "Literal[b\"\\xff\\x00\"]");

        Ok(())
    }

    #[test]
    fn ellipsis_type() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            x = ...
            ",
        )?;

        // TODO: sys.version_info, and need to understand @final and @type_check_only
        assert_public_ty(&db, "src/a.py", "x", "Unknown | EllipsisType");

        Ok(())
    }

    #[test]
    fn function_return_type() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file("src/a.py", "def example() -> int: return 42")?;

        let mod_file = system_path_to_file(&db, "src/a.py").unwrap();
        let function = global_symbol_ty(&db, mod_file, "example").expect_function();
        let returns = function.return_type(&db);
        assert_eq!(returns.display(&db).to_string(), "int");

        Ok(())
    }

    #[test]
    fn fstring_expression() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            x = 0
            y = str()
            z = False

            a = f'hello'
            b = f'h {x}'
            c = 'one ' f'single ' f'literal'
            d = 'first ' f'second({b})' f' third'
            e = f'-{y}-'
            f = f'-{y}-' f'--' '--'
            g = f'{z} == {False} is {True}'
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "a", "Literal[\"hello\"]");
        assert_public_ty(&db, "src/a.py", "b", "Literal[\"h 0\"]");
        assert_public_ty(&db, "src/a.py", "c", "Literal[\"one single literal\"]");
        assert_public_ty(&db, "src/a.py", "d", "Literal[\"first second(h 0) third\"]");
        assert_public_ty(&db, "src/a.py", "e", "str");
        assert_public_ty(&db, "src/a.py", "f", "str");
        assert_public_ty(&db, "src/a.py", "g", "Literal[\"False == False is True\"]");

        Ok(())
    }

    #[test]
    fn fstring_expression_with_conversion_flags() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            string = 'hello'
            a = f'{string!r}'
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "a", "str"); // Should be `Literal["'hello'"]`

        Ok(())
    }

    #[test]
    fn fstring_expression_with_format_specifier() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            a = f'{1:02}'
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "a", "str"); // Should be `Literal["01"]`

        Ok(())
    }

    #[test]
    fn basic_call_expression() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            def get_int() -> int:
                return 42

            x = get_int()
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "x", "int");

        Ok(())
    }

    #[test]
    fn basic_async_call_expression() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            async def get_int_async() -> int:
                return 42

            x = get_int_async()
            ",
        )?;

        // TODO: Generic `types.CoroutineType`!
        assert_public_ty(&db, "src/a.py", "x", "@Todo");

        Ok(())
    }

    #[test]
    fn basic_decorated_call_expression() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            from typing import Callable

            def foo() -> int:
                return 42

            def decorator(func) -> Callable[[], int]:
                return foo

            @decorator
            def bar() -> str:
                return 'bar'

            x = bar()
            ",
        )?;

        // TODO: should be `int`!
        assert_public_ty(&db, "src/a.py", "x", "@Todo");

        Ok(())
    }

    #[test]
    fn class_constructor_call_expression() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            class Foo: ...

            x = Foo()
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "x", "Foo");

        Ok(())
    }

    #[test]
    fn call_union() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            if flag:
                def f() -> int:
                    return 1
            else:
                def f() -> str:
                    return 'foo'
            x = f()
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "x", "int | str");

        Ok(())
    }

    #[test]
    fn call_union_with_unknown() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            from nonexistent import f
            if flag:
                def f() -> int:
                    return 1
            x = f()
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "x", "Unknown | int");

        Ok(())
    }

    #[test]
    fn call_union_with_not_callable() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            if flag:
                f = 1
            else:
                def f() -> int:
                    return 1
            x = f()
            ",
        )?;

        assert_file_diagnostics(
            &db,
            "src/a.py",
            &["Object of type `Literal[1] | Literal[f]` is not callable (due to union element `Literal[1]`)"],
        );
        assert_public_ty(&db, "src/a.py", "x", "Unknown | int");

        Ok(())
    }

    #[test]
    fn call_union_with_multiple_not_callable() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            if flag:
                f = 1
            elif flag2:
                f = 'foo'
            else:
                def f() -> int:
                    return 1
            x = f()
            ",
        )?;

        assert_file_diagnostics(
            &db,
            "src/a.py",
            &[
                r#"Object of type `Literal[1] | Literal["foo"] | Literal[f]` is not callable (due to union elements Literal[1], Literal["foo"])"#,
            ],
        );
        assert_public_ty(&db, "src/a.py", "x", "Unknown | int");

        Ok(())
    }

    #[test]
    fn call_union_with_all_not_callable() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            if flag:
                f = 1
            else:
                f = 'foo'
            x = f()
            ",
        )?;

        assert_file_diagnostics(
            &db,
            "src/a.py",
            &[r#"Object of type `Literal[1] | Literal["foo"]` is not callable"#],
        );
        assert_public_ty(&db, "src/a.py", "x", "Unknown");

        Ok(())
    }

    #[test]
    fn invalid_callable() {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            nonsense = 123
            x = nonsense()
            ",
        )
        .unwrap();

        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &["Object of type `Literal[123]` is not callable"],
        );
    }

    #[test]
    fn resolve_union() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            if flag:
                x = 1
            else:
                x = 2
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[1, 2]");

        Ok(())
    }

    #[test]
    fn simplify_true_and_false_to_bool() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            from typing_extensions import reveal_type

            def returns_bool() -> bool: ...

            if returns_bool():
                x = True
            else:
                x = False

            reveal_type(x)
            ",
        )?;

        assert_file_diagnostics(&db, "src/a.py", &["Revealed type is `bool`"]);

        Ok(())
    }

    #[test]
    fn literal_int_arithmetic() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            a = 2 + 1
            b = a - 4
            c = a * b
            d = c // 3
            e = c / 3
            f = 5 % 3
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "a", "Literal[3]");
        assert_public_ty(&db, "src/a.py", "b", "Literal[-1]");
        assert_public_ty(&db, "src/a.py", "c", "Literal[-3]");
        assert_public_ty(&db, "src/a.py", "d", "Literal[-1]");
        assert_public_ty(&db, "src/a.py", "e", "float");
        assert_public_ty(&db, "src/a.py", "f", "Literal[2]");

        Ok(())
    }

    #[test]
    fn division_by_zero() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            a = 1 / 0
            b = 2 // 0
            c = 3 % 0
            d = int() / 0
            e = 1.0 / 0
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "float");
        assert_public_ty(&db, "/src/a.py", "b", "int");
        assert_public_ty(&db, "/src/a.py", "c", "int");
        // TODO: These should be `int` and `float` respectively once we support inference
        assert_public_ty(&db, "/src/a.py", "d", "@Todo");
        assert_public_ty(&db, "/src/a.py", "e", "@Todo");

        assert_file_diagnostics(
            &db,
            "src/a.py",
            &[
                "Cannot divide object of type `Literal[1]` by zero",
                "Cannot floor divide object of type `Literal[2]` by zero",
                "Cannot reduce object of type `Literal[3]` modulo zero",
                "Cannot divide object of type `int` by zero",
                "Cannot divide object of type `float` by zero",
            ],
        );

        Ok(())
    }

    #[test]
    fn walrus() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file("src/a.py", "x = (y := 1) + 1")?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[2]");
        assert_public_ty(&db, "src/a.py", "y", "Literal[1]");

        Ok(())
    }

    #[test]
    fn walrus_self_plus_one() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            x = 0
            (x := x + 1)
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[1]");

        Ok(())
    }

    #[test]
    fn ifexpr() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file("src/a.py", "x = 1 if flag else 2")?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[1, 2]");

        Ok(())
    }

    #[test]
    fn ifexpr_walrus() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            y = 0
            z = 0
            x = (y := 1) if flag else (z := 2)
            a = y
            b = z
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[1, 2]");
        assert_public_ty(&db, "src/a.py", "a", "Literal[0, 1]");
        assert_public_ty(&db, "src/a.py", "b", "Literal[0, 2]");

        Ok(())
    }

    #[test]
    fn ifexpr_nested() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file("src/a.py", "x = 1 if flag else 2 if flag2 else 3")?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[1, 2, 3]");

        Ok(())
    }

    #[test]
    fn multi_target_assign() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file("src/a.py", "x = y = 1")?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[1]");
        assert_public_ty(&db, "src/a.py", "y", "Literal[1]");

        Ok(())
    }

    #[test]
    fn none() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file("src/a.py", "x = 1 if flag else None")?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[1] | None");
        Ok(())
    }

    #[test]
    fn simple_if() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            y = 1
            y = 2
            if flag:
                y = 3
            x = y
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[2, 3]");
        Ok(())
    }

    #[test]
    fn maybe_unbound() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            if flag:
                y = 3
            x = y
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "x", "Unbound | Literal[3]");
        Ok(())
    }

    #[test]
    fn if_elif_else() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            y = 1
            y = 2
            if flag:
                y = 3
            elif flag2:
                y = 4
            else:
                r = y
                y = 5
                s = y
            x = y
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[3, 4, 5]");
        assert_public_ty(&db, "src/a.py", "r", "Unbound | Literal[2]");
        assert_public_ty(&db, "src/a.py", "s", "Unbound | Literal[5]");
        Ok(())
    }

    #[test]
    fn if_elif_else_single_symbol() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            if flag:
                y = 1
            elif flag2:
                y = 2
            else:
                y = 3
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "y", "Literal[1, 2, 3]");
        Ok(())
    }

    #[test]
    fn if_elif_else_no_definition_in_else() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            y = 0
            if flag:
                y = 1
            elif flag2:
                y = 2
            else:
                pass
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "y", "Literal[0, 1, 2]");
        Ok(())
    }

    #[test]
    fn if_elif_else_no_definition_in_else_one_intervening_definition() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            y = 0
            if flag:
                y = 1
                z = 3
            elif flag2:
                y = 2
            else:
                pass
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "y", "Literal[0, 1, 2]");
        Ok(())
    }

    #[test]
    fn nested_if() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            y = 0
            if flag:
                if flag2:
                    y = 1
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "y", "Literal[0, 1]");
        Ok(())
    }

    #[test]
    fn if_elif() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            y = 1
            y = 2
            if flag:
                y = 3
            elif flag2:
                y = 4
            x = y
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[2, 3, 4]");
        Ok(())
    }

    #[test]
    fn match_with_wildcard() {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            match 0:
                case 1:
                    y = 2
                case _:
                    y = 3
",
        )
        .unwrap();

        assert_public_ty(&db, "src/a.py", "y", "Literal[2, 3]");
    }

    #[test]
    fn match_without_wildcard() {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            match 0:
                case 1:
                    y = 2
                case 2:
                    y = 3
",
        )
        .unwrap();

        assert_public_ty(&db, "src/a.py", "y", "Unbound | Literal[2, 3]");
    }

    #[test]
    fn match_stmt() {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            y = 1
            y = 2
            match 0:
                case 1:
                    y = 3
                case 2:
                    y = 4
",
        )
        .unwrap();

        assert_public_ty(&db, "src/a.py", "y", "Literal[2, 3, 4]");
    }

    #[test]
    fn import_cycle() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            class A: pass
            import b
            class C(b.B): pass
            ",
        )?;
        db.write_dedented(
            "src/b.py",
            "
            from a import A
            class B(A): pass
            ",
        )?;

        let a = system_path_to_file(&db, "src/a.py").expect("file to exist");
        let c_ty = global_symbol_ty(&db, a, "C");
        let c_class = c_ty.expect_class();
        let mut c_bases = c_class.bases(&db);
        let b_ty = c_bases.next().unwrap();
        let b_class = b_ty.expect_class();
        assert_eq!(b_class.name(&db), "B");
        let mut b_bases = b_class.bases(&db);
        let a_ty = b_bases.next().unwrap();
        let a_class = a_ty.expect_class();
        assert_eq!(a_class.name(&db), "A");

        Ok(())
    }

    /// An unbound function local that has definitions in the scope does not fall back to globals.
    #[test]
    fn unbound_function_local() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            x = 1
            def f():
                y = x
                x = 2
            ",
        )?;

        let file = system_path_to_file(&db, "src/a.py").expect("file to exist");
        let index = semantic_index(&db, file);
        let function_scope = index
            .child_scopes(FileScopeId::global())
            .next()
            .unwrap()
            .0
            .to_scope_id(&db, file);
        let y_ty = symbol_ty(&db, function_scope, "y");
        let x_ty = symbol_ty(&db, function_scope, "x");

        assert_eq!(y_ty.display(&db).to_string(), "Unbound");
        assert_eq!(x_ty.display(&db).to_string(), "Literal[2]");

        Ok(())
    }

    /// A name reference to a never-defined symbol in a function is implicitly a global lookup.
    #[test]
    fn implicit_global_in_function() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            x = 1
            def f():
                y = x
            ",
        )?;

        let file = system_path_to_file(&db, "src/a.py").expect("file to exist");
        let index = semantic_index(&db, file);
        let function_scope = index
            .child_scopes(FileScopeId::global())
            .next()
            .unwrap()
            .0
            .to_scope_id(&db, file);
        let y_ty = symbol_ty(&db, function_scope, "y");
        let x_ty = symbol_ty(&db, function_scope, "x");

        assert_eq!(x_ty.display(&db).to_string(), "Unbound");
        assert_eq!(y_ty.display(&db).to_string(), "Literal[1]");

        Ok(())
    }

    #[test]
    fn conditionally_global_or_builtin() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            if flag:
                copyright = 1
            def f():
                y = copyright
            ",
        )?;

        let file = system_path_to_file(&db, "src/a.py").expect("file to exist");
        let index = semantic_index(&db, file);
        let function_scope = index
            .child_scopes(FileScopeId::global())
            .next()
            .unwrap()
            .0
            .to_scope_id(&db, file);
        let y_ty = symbol_ty(&db, function_scope, "y");

        assert_eq!(
            y_ty.display(&db).to_string(),
            "Literal[copyright] | Literal[1]"
        );

        Ok(())
    }

    /// Class name lookups do fall back to globals, but the public type never does.
    #[test]
    fn unbound_class_local() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            x = 1
            class C:
                y = x
                if flag:
                    x = 2
            ",
        )?;

        let file = system_path_to_file(&db, "src/a.py").expect("file to exist");
        let index = semantic_index(&db, file);
        let class_scope = index
            .child_scopes(FileScopeId::global())
            .next()
            .unwrap()
            .0
            .to_scope_id(&db, file);
        let y_ty = symbol_ty(&db, class_scope, "y");
        let x_ty = symbol_ty(&db, class_scope, "x");

        assert_eq!(x_ty.display(&db).to_string(), "Unbound | Literal[2]");
        assert_eq!(y_ty.display(&db).to_string(), "Literal[1]");

        Ok(())
    }

    #[test]
    fn local_inference() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file("/src/a.py", "x = 10")?;
        let a = system_path_to_file(&db, "/src/a.py").unwrap();

        let parsed = parsed_module(&db, a);

        let statement = parsed.suite().first().unwrap().as_assign_stmt().unwrap();
        let model = SemanticModel::new(&db, a);

        let literal_ty = statement.value.ty(&model);

        assert_eq!(format!("{}", literal_ty.display(&db)), "Literal[10]");

        Ok(())
    }

    #[test]
    fn builtin_symbol_vendored_stdlib() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file("/src/a.py", "c = copyright")?;

        assert_public_ty(&db, "/src/a.py", "c", "Literal[copyright]");

        Ok(())
    }

    #[test]
    fn builtin_symbol_custom_stdlib() -> anyhow::Result<()> {
        let db = setup_db_with_custom_typeshed(
            "/typeshed",
            [
                ("/src/a.py", "c = copyright"),
                (
                    "/typeshed/stdlib/builtins.pyi",
                    "def copyright() -> None: ...",
                ),
                ("/typeshed/stdlib/VERSIONS", "builtins: 3.8-"),
            ],
        )?;

        assert_public_ty(&db, "/src/a.py", "c", "Literal[copyright]");

        Ok(())
    }

    #[test]
    fn unknown_global_later_defined() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file("/src/a.py", "x = foo; foo = 1")?;

        assert_public_ty(&db, "/src/a.py", "x", "Unbound");

        Ok(())
    }

    #[test]
    fn unknown_builtin_later_defined() -> anyhow::Result<()> {
        let db = setup_db_with_custom_typeshed(
            "/typeshed",
            [
                ("/src/a.py", "x = foo"),
                ("/typeshed/stdlib/builtins.pyi", "foo = bar; bar = 1"),
                ("/typeshed/stdlib/VERSIONS", "builtins: 3.8-"),
            ],
        )?;

        assert_public_ty(&db, "/src/a.py", "x", "Unbound");

        Ok(())
    }

    #[test]
    fn import_builtins() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file("/src/a.py", "import builtins; x = builtins.copyright")?;

        assert_public_ty(&db, "/src/a.py", "x", "Literal[copyright]");
        // imported builtins module is the same file as the implicit builtins
        let file = system_path_to_file(&db, "/src/a.py").expect("file to exist");
        let builtins_ty = global_symbol_ty(&db, file, "builtins");
        let builtins_file = builtins_ty.expect_module();
        let implicit_builtins_file = builtins_module_scope(&db)
            .expect("builtins module should exist")
            .file(&db);
        assert_eq!(builtins_file, implicit_builtins_file);

        Ok(())
    }

    /// A class's bases can be self-referential; this looks silly but a slightly more complex
    /// version of it actually occurs in typeshed: `class str(Sequence[str]): ...`
    #[test]
    fn cyclical_class_pyi_definition() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_file("/src/a.pyi", "class C(C): ...")?;
        assert_public_ty(&db, "/src/a.pyi", "C", "Literal[C]");
        Ok(())
    }

    #[test]
    fn str_builtin() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_file("/src/a.py", "x = str")?;
        assert_public_ty(&db, "/src/a.py", "x", "Literal[str]");
        Ok(())
    }

    #[test]
    fn deferred_annotation_builtin() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_file("/src/a.pyi", "class C(object): pass")?;
        let file = system_path_to_file(&db, "/src/a.pyi").unwrap();
        let ty = global_symbol_ty(&db, file, "C");

        let base = ty
            .expect_class()
            .bases(&db)
            .next()
            .expect("there should be at least one base");

        assert_eq!(base.display(&db).to_string(), "Literal[object]");

        Ok(())
    }

    #[test]
    fn deferred_annotation_in_stubs_always_resolve() -> anyhow::Result<()> {
        let mut db = setup_db();

        // Stub files should always resolve deferred annotations
        db.write_dedented(
            "/src/stub.pyi",
            "
            def get_foo() -> Foo: ...
            class Foo: ...
            foo = get_foo()
            ",
        )?;
        assert_public_ty(&db, "/src/stub.pyi", "foo", "Foo");

        Ok(())
    }

    #[test]
    fn deferred_annotations_regular_source_fails() -> anyhow::Result<()> {
        let mut db = setup_db();

        // In (regular) source files, deferred annotations are *not* resolved
        // Also tests imports from `__future__` that are not annotations
        db.write_dedented(
            "/src/source.py",
            "
            from __future__ import with_statement as annotations
            def get_foo() -> Foo: ...
            class Foo: ...
            foo = get_foo()
            ",
        )?;
        assert_public_ty(&db, "/src/source.py", "foo", "Unknown");

        Ok(())
    }

    #[test]
    fn deferred_annotation_in_sources_with_future_resolves() -> anyhow::Result<()> {
        let mut db = setup_db();

        // In source files with `__future__.annotations`, deferred annotations are resolved
        db.write_dedented(
            "/src/source_with_future.py",
            "
            from __future__ import annotations
            def get_foo() -> Foo: ...
            class Foo: ...
            foo = get_foo()
            ",
        )?;
        assert_public_ty(&db, "/src/source_with_future.py", "foo", "Foo");

        Ok(())
    }

    #[test]
    fn narrow_not_none() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            x = None if flag else 1
            y = 0
            if x is not None:
                y = x
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "x", "None | Literal[1]");
        assert_public_ty(&db, "/src/a.py", "y", "Literal[0, 1]");

        Ok(())
    }

    #[test]
    fn narrow_singleton_pattern() {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            x = None if flag else 1
            y = 0
            match x:
                case None:
                    y = x
            ",
        )
        .unwrap();

        // TODO: The correct inferred type should be `Literal[0] | None` but currently the
        // simplification logic doesn't account for this. The final type with parenthesis:
        // `Literal[0] | None | (Literal[1] & None)`
        assert_public_ty(
            &db,
            "/src/a.py",
            "y",
            "Literal[0] | None | Literal[1] & None",
        );
    }

    #[test]
    fn while_loop() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            x = 1
            while flag:
                x = 2
            ",
        )?;

        // body of while loop may or may not run
        assert_public_ty(&db, "/src/a.py", "x", "Literal[1, 2]");

        Ok(())
    }

    #[test]
    fn while_else_no_break() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            x = 1
            while flag:
                x = 2
            else:
                y = x
                x = 3
            ",
        )?;

        // body of the loop can't break, so we can get else, or body+else
        // x must be 3, because else will always run
        assert_public_ty(&db, "/src/a.py", "x", "Literal[3]");
        // y can be 1 or 2 because else always runs, and body may or may not run first
        assert_public_ty(&db, "/src/a.py", "y", "Literal[1, 2]");

        Ok(())
    }

    #[test]
    fn while_else_may_break() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            x = 1
            y = 0
            while flag:
                x = 2
                if flag2:
                    y = 4
                    break
            else:
                y = x
                x = 3
            ",
        )?;

        // body may break: we can get just-body (only if we break), just-else, or body+else
        assert_public_ty(&db, "/src/a.py", "x", "Literal[2, 3]");
        // if just-body were possible without the break, then 0 would be possible for y
        // 1 and 2 both being possible for y shows that we can hit else with or without body
        assert_public_ty(&db, "/src/a.py", "y", "Literal[1, 2, 4]");

        Ok(())
    }

    #[test]
    fn attribute_of_union() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            if flag:
                class C:
                    x = 1
            else:
                class C:
                    x = 2
            y = C.x
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "y", "Literal[1, 2]");

        Ok(())
    }

    #[test]
    fn big_int() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            x = 10_000_000_000_000_000_000
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "x", "int");

        Ok(())
    }

    #[test]
    fn empty_tuple_literal() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            x = ()
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "x", "tuple[()]");

        Ok(())
    }

    #[test]
    fn tuple_heterogeneous_literal() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            x = (1, 'a')
            y = (1, (2, 3))
            z = (x, 2)
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "x", r#"tuple[Literal[1], Literal["a"]]"#);
        assert_public_ty(
            &db,
            "/src/a.py",
            "y",
            "tuple[Literal[1], tuple[Literal[2], Literal[3]]]",
        );
        assert_public_ty(
            &db,
            "/src/a.py",
            "z",
            r#"tuple[tuple[Literal[1], Literal["a"]], Literal[2]]"#,
        );

        Ok(())
    }

    #[test]
    fn list_literal() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            x = []
            ",
        )?;

        // TODO should be a generic type
        assert_public_ty(&db, "/src/a.py", "x", "list");

        Ok(())
    }

    #[test]
    fn set_literal() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            x = {1, 2}
            ",
        )?;

        // TODO should be a generic type
        assert_public_ty(&db, "/src/a.py", "x", "set");

        Ok(())
    }

    #[test]
    fn dict_literal() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            x = {}
            ",
        )?;

        // TODO should be a generic type
        assert_public_ty(&db, "/src/a.py", "x", "dict");

        Ok(())
    }

    #[test]
    fn nonlocal_name_reference() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            def f():
                x = 1
                def g():
                    y = x
            ",
        )?;

        assert_scope_ty(&db, "/src/a.py", &["f", "g"], "y", "Literal[1]");

        Ok(())
    }

    #[test]
    fn nonlocal_name_reference_multi_level() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            def f():
                x = 1
                def g():
                    def h():
                        y = x
            ",
        )?;

        assert_scope_ty(&db, "/src/a.py", &["f", "g", "h"], "y", "Literal[1]");

        Ok(())
    }

    #[test]
    fn nonlocal_name_reference_skips_class_scope() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            def f():
                x = 1
                class C:
                    x = 2
                    def g():
                        y = x
            ",
        )?;

        assert_scope_ty(&db, "/src/a.py", &["f", "C", "g"], "y", "Literal[1]");

        Ok(())
    }

    #[test]
    fn nonlocal_name_reference_skips_annotation_only_assignment() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            def f():
                x = 1
                def g():
                    // it's pretty weird to have an annotated assignment in a function where the
                    // name is otherwise not defined; maybe should be an error?
                    x: int
                    def h():
                        y = x
            ",
        )?;

        assert_scope_ty(&db, "/src/a.py", &["f", "g", "h"], "y", "Literal[1]");

        Ok(())
    }

    #[test]
    fn annotation_only_assignment_transparent_to_local_inference() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            x = 1
            x: int
            y = x
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "y", "Literal[1]");

        Ok(())
    }

    /// A declared-but-not-bound name can be imported from a stub file.
    #[test]
    fn import_from_stub_declaration_only() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            from b import x
            y = x
            ",
        )?;
        db.write_dedented(
            "/src/b.pyi",
            "
            x: int
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "y", "int");

        Ok(())
    }

    /// Declarations take priority over definitions when importing from a non-stub file.
    #[test]
    fn import_from_non_stub_declared_and_bound() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            from b import x
            y = x
            ",
        )?;
        db.write_dedented(
            "/src/b.py",
            "
            x: int = 1
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "y", "int");

        Ok(())
    }

    #[test]
    fn unresolved_import_statement() {
        let mut db = setup_db();

        db.write_file("src/foo.py", "import bar\n").unwrap();

        assert_file_diagnostics(&db, "src/foo.py", &["Cannot resolve import `bar`"]);
    }

    #[test]
    fn unresolved_import_from_statement() {
        let mut db = setup_db();

        db.write_file("src/foo.py", "from bar import baz\n")
            .unwrap();
        assert_file_diagnostics(&db, "/src/foo.py", &["Cannot resolve import `bar`"]);
    }

    #[test]
    fn unresolved_import_from_resolved_module() {
        let mut db = setup_db();

        db.write_files([("/src/a.py", ""), ("/src/b.py", "from a import thing")])
            .unwrap();

        assert_file_diagnostics(&db, "/src/b.py", &["Module `a` has no member `thing`"]);
    }

    #[test]
    fn resolved_import_of_symbol_from_unresolved_import() {
        let mut db = setup_db();

        db.write_files([
            ("/src/a.py", "import foo as foo"),
            ("/src/b.py", "from a import foo"),
        ])
        .unwrap();

        assert_file_diagnostics(&db, "/src/a.py", &["Cannot resolve import `foo`"]);

        // Importing the unresolved import into a second first-party file should not trigger
        // an additional "unresolved import" violation
        assert_file_diagnostics(&db, "/src/b.py", &[]);
    }

    #[test]
    fn basic_for_loop() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            class IntIterator:
                def __next__(self) -> int:
                    return 42

            class IntIterable:
                def __iter__(self) -> IntIterator:
                    return IntIterator()

            for x in IntIterable():
                pass
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "x", "Unbound | int");

        Ok(())
    }

    #[test]
    fn for_loop_with_previous_definition() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            class IntIterator:
                def __next__(self) -> int:
                    return 42

            class IntIterable:
                def __iter__(self) -> IntIterator:
                    return IntIterator()

            x = 'foo'

            for x in IntIterable():
                pass
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "x", r#"Literal["foo"] | int"#);

        Ok(())
    }

    #[test]
    fn for_loop_no_break() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            class IntIterator:
                def __next__(self) -> int:
                    return 42

            class IntIterable:
                def __iter__(self) -> IntIterator:
                    return IntIterator()

            for x in IntIterable():
                pass
            else:
                x = 'foo'
            ",
        )?;

        // The `for` loop can never break, so the `else` clause will always be executed,
        // meaning that the visible definition by the end of the scope is solely determined
        // by the `else` clause
        assert_public_ty(&db, "src/a.py", "x", r#"Literal["foo"]"#);

        Ok(())
    }

    #[test]
    fn for_loop_may_break() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            class IntIterator:
                def __next__(self) -> int:
                    return 42

            class IntIterable:
                def __iter__(self) -> IntIterator:
                    return IntIterator()

            for x in IntIterable():
                if x > 5:
                    break
            else:
                x = 'foo'
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "x", r#"int | Literal["foo"]"#);

        Ok(())
    }

    #[test]
    fn for_loop_with_old_style_iteration_protocol() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            class OldStyleIterable:
                def __getitem__(self, key: int) -> int:
                    return 42

            for x in OldStyleIterable():
                pass
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "x", "Unbound | int");

        Ok(())
    }

    /// This tests that we understand that `async` for loops
    /// do not work according to the synchronous iteration protocol
    #[test]
    fn invalid_async_for_loop() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            async def foo():
                class Iterator:
                    def __next__(self) -> int:
                        return 42

                class Iterable:
                    def __iter__(self) -> Iterator:
                        return Iterator()

                async for x in Iterator():
                    pass
            ",
        )?;

        // We currently return `Todo` for all `async for` loops,
        // including loops that have invalid syntax
        assert_scope_ty(&db, "src/a.py", &["foo"], "x", "Unbound | @Todo");

        Ok(())
    }

    #[test]
    fn basic_async_for_loop() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            async def foo():
                class IntAsyncIterator:
                    async def __anext__(self) -> int:
                        return 42

                class IntAsyncIterable:
                    def __aiter__(self) -> IntAsyncIterator:
                        return IntAsyncIterator()

                async for x in IntAsyncIterable():
                    pass
            ",
        )?;

        // TODO(Alex) async iterables/iterators!
        assert_scope_ty(&db, "src/a.py", &["foo"], "x", "Unbound | @Todo");

        Ok(())
    }

    #[test]
    fn for_loop_with_heterogenous_tuple() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            for x in (1, 'a', b'foo'):
                pass
            ",
        )?;

        assert_public_ty(
            &db,
            "src/a.py",
            "x",
            r#"Unbound | Literal[1] | Literal["a"] | Literal[b"foo"]"#,
        );

        Ok(())
    }

    #[test]
    fn for_loop_non_callable_iter() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            class NotIterable:
                if flag:
                    __iter__ = 1
                else:
                    __iter__ = None

            for x in NotIterable():
                pass
            ",
        )?;

        assert_file_diagnostics(
            &db,
            "src/a.py",
            &["Object of type `NotIterable` is not iterable"],
        );
        assert_public_ty(&db, "src/a.py", "x", "Unbound | Unknown");

        Ok(())
    }

    #[test]
    fn except_handler_single_exception() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            import re
            from typing_extensions import reveal_type

            try:
                x
            except NameError as e:
                reveal_type(e)
            except re.error as f:
                reveal_type(f)
            ",
        )?;

        assert_file_diagnostics(
            &db,
            "src/a.py",
            &["Revealed type is `NameError`", "Revealed type is `error`"],
        );

        Ok(())
    }

    #[test]
    fn unknown_type_in_except_handler_does_not_cause_spurious_diagnostic() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            from nonexistent_module import foo
            from typing_extensions import reveal_type

            try:
                x
            except foo as e:
                reveal_type(foo)
                reveal_type(e)
            ",
        )?;

        assert_file_diagnostics(
            &db,
            "src/a.py",
            &[
                "Cannot resolve import `nonexistent_module`",
                "Revealed type is `Unknown`",
                "Revealed type is `Unknown`",
            ],
        );

        Ok(())
    }

    #[test]
    fn except_handler_multiple_exceptions() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            from typing_extensions import reveal_type

            EXCEPTIONS = (AttributeError, TypeError)

            try:
                x
            except (RuntimeError, OSError) as e:
                reveal_type(e)
            except EXCEPTIONS as f:
                reveal_type(f)
            ",
        )?;

        // For these TODOs we need support for `tuple` types:
        let expected_diagnostics = &[
            // TODO: Should be `RuntimeError | OSError` --Alex
            "Revealed type is `@Todo`",
            // TODO: Should be `AttributeError | TypeError` --Alex
            "Revealed type is `@Todo`",
        ];

        assert_file_diagnostics(&db, "src/a.py", expected_diagnostics);

        Ok(())
    }

    #[test]
    fn exception_handler_with_invalid_syntax() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            from typing_extensions import reveal_type

            try:
                x
            except as e:
                reveal_type(e)
            ",
        )?;

        assert_file_diagnostics(&db, "src/a.py", &["Revealed type is `Unknown`"]);

        Ok(())
    }

    #[test]
    fn except_star_handler_baseexception() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            from typing_extensions import reveal_type

            try:
                x
            except* BaseException as e:
                reveal_type(e)
            ",
        )?;

        // TODO: once we support `sys.version_info` branches,
        // we can set `--target-version=py311` in this test
        // and the inferred type will just be `BaseExceptionGroup` --Alex
        assert_file_diagnostics(
            &db,
            "src/a.py",
            &["Revealed type is `Unknown | BaseExceptionGroup`"],
        );

        Ok(())
    }

    #[test]
    fn except_star_handler() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            from typing_extensions import reveal_type

            try:
                x
            except* OSError as e:
                reveal_type(e)
            ",
        )?;

        // TODO: once we support `sys.version_info` branches,
        // we can set `--target-version=py311` in this test
        // and the inferred type will just be `BaseExceptionGroup` --Alex
        //
        // TODO more precise would be `ExceptionGroup[OSError]` --Alex
        assert_file_diagnostics(
            &db,
            "src/a.py",
            &["Revealed type is `Unknown | BaseExceptionGroup`"],
        );

        Ok(())
    }

    #[test]
    fn except_star_handler_multiple_types() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            from typing_extensions import reveal_type

            try:
                x
            except* (TypeError, AttributeError) as e:
                reveal_type(e)
            ",
        )?;

        // TODO: once we support `sys.version_info` branches,
        // we can set `--target-version=py311` in this test
        // and the inferred type will just be `BaseExceptionGroup` --Alex
        //
        // TODO more precise would be `ExceptionGroup[TypeError | AttributeError]` --Alex
        assert_file_diagnostics(
            &db,
            "src/a.py",
            &["Revealed type is `Unknown | BaseExceptionGroup`"],
        );

        Ok(())
    }

    #[test]
    fn basic_comprehension() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            def foo():
                [x for y in IterableOfIterables() for x in y]

            class IntIterator:
                def __next__(self) -> int:
                    return 42

            class IntIterable:
                def __iter__(self) -> IntIterator:
                    return IntIterator()

            class IteratorOfIterables:
                def __next__(self) -> IntIterable:
                    return IntIterable()

            class IterableOfIterables:
                def __iter__(self) -> IteratorOfIterables:
                    return IteratorOfIterables()
            ",
        )?;

        assert_scope_ty(&db, "src/a.py", &["foo", "<listcomp>"], "x", "int");
        assert_scope_ty(&db, "src/a.py", &["foo", "<listcomp>"], "y", "IntIterable");

        Ok(())
    }

    #[test]
    fn comprehension_inside_comprehension() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            def foo():
                [[x for x in iter1] for y in iter2]

            class IntIterator:
                def __next__(self) -> int:
                    return 42

            class IntIterable:
                def __iter__(self) -> IntIterator:
                    return IntIterator()

            iter1 = IntIterable()
            iter2 = IntIterable()
            ",
        )?;

        assert_scope_ty(
            &db,
            "src/a.py",
            &["foo", "<listcomp>", "<listcomp>"],
            "x",
            "int",
        );
        assert_scope_ty(&db, "src/a.py", &["foo", "<listcomp>"], "y", "int");

        Ok(())
    }

    #[test]
    fn inner_comprehension_referencing_outer_comprehension() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            def foo():
                [[x for x in y] for y in z]

            class IntIterator:
                def __next__(self) -> int:
                    return 42

            class IntIterable:
                def __iter__(self) -> IntIterator:
                    return IntIterator()

            class IteratorOfIterables:
                def __next__(self) -> IntIterable:
                    return IntIterable()

            class IterableOfIterables:
                def __iter__(self) -> IteratorOfIterables:
                    return IteratorOfIterables()

            z = IterableOfIterables()
            ",
        )?;

        assert_scope_ty(
            &db,
            "src/a.py",
            &["foo", "<listcomp>", "<listcomp>"],
            "x",
            "int",
        );
        assert_scope_ty(&db, "src/a.py", &["foo", "<listcomp>"], "y", "IntIterable");

        Ok(())
    }

    #[test]
    fn comprehension_with_unbound_iter() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented("src/a.py", "[z for z in x]")?;

        assert_scope_ty(&db, "src/a.py", &["<listcomp>"], "x", "Unbound");

        // Iterating over an `Unbound` yields `Unknown`:
        assert_scope_ty(&db, "src/a.py", &["<listcomp>"], "z", "Unknown");

        // TODO: not the greatest error message in the world! --Alex
        assert_file_diagnostics(
            &db,
            "src/a.py",
            &["Object of type `Unbound` is not iterable"],
        );

        Ok(())
    }

    #[test]
    fn comprehension_with_not_iterable_iter_in_second_comprehension() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            def foo():
                [z for x in IntIterable() for z in x]

            class IntIterator:
                def __next__(self) -> int:
                    return 42

            class IntIterable:
                def __iter__(self) -> IntIterator:
                    return IntIterator()
            ",
        )?;

        assert_scope_ty(&db, "src/a.py", &["foo", "<listcomp>"], "x", "int");
        assert_scope_ty(&db, "src/a.py", &["foo", "<listcomp>"], "z", "Unknown");
        assert_file_diagnostics(&db, "src/a.py", &["Object of type `int` is not iterable"]);

        Ok(())
    }

    #[test]
    fn dict_comprehension_variable_key() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            def foo():
                {x: 0 for x in IntIterable()}

            class IntIterator:
                def __next__(self) -> int:
                    return 42

            class IntIterable:
                def __iter__(self) -> IntIterator:
                    return IntIterator()
            ",
        )?;

        assert_scope_ty(&db, "src/a.py", &["foo", "<dictcomp>"], "x", "int");
        assert_file_diagnostics(&db, "src/a.py", &[]);

        Ok(())
    }

    #[test]
    fn dict_comprehension_variable_value() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            def foo():
                {0: x for x in IntIterable()}

            class IntIterator:
                def __next__(self) -> int:
                    return 42

            class IntIterable:
                def __iter__(self) -> IntIterator:
                    return IntIterator()
            ",
        )?;

        assert_scope_ty(&db, "src/a.py", &["foo", "<dictcomp>"], "x", "int");
        assert_file_diagnostics(&db, "src/a.py", &[]);

        Ok(())
    }

    #[test]
    fn comprehension_with_missing_in_keyword() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            def foo():
                [z for z IntIterable()]

            class IntIterator:
                def __next__(self) -> int:
                    return 42

            class IntIterable:
                def __iter__(self) -> IntIterator:
                    return IntIterator()
            ",
        )?;

        // We'll emit a diagnostic separately for invalid syntax,
        // but it's reasonably clear here what they *meant* to write,
        // so we'll still infer the correct type:
        assert_scope_ty(&db, "src/a.py", &["foo", "<listcomp>"], "z", "int");
        Ok(())
    }

    #[test]
    fn comprehension_with_missing_iter() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            def foo():
                [z for in IntIterable()]

            class IntIterator:
                def __next__(self) -> int:
                    return 42

            class IntIterable:
                def __iter__(self) -> IntIterator:
                    return IntIterator()
            ",
        )?;

        assert_scope_ty(&db, "src/a.py", &["foo", "<listcomp>"], "z", "Unbound");

        // (There is a diagnostic for invalid syntax that's emitted, but it's not listed by `assert_file_diagnostics`)
        assert_file_diagnostics(&db, "src/a.py", &[]);

        Ok(())
    }

    #[test]
    fn comprehension_with_missing_for() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented("src/a.py", "[z for z in]")?;
        assert_scope_ty(&db, "src/a.py", &["<listcomp>"], "z", "Unknown");
        Ok(())
    }

    #[test]
    fn comprehension_with_missing_in_keyword_and_missing_iter() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented("src/a.py", "[z for z]")?;
        assert_scope_ty(&db, "src/a.py", &["<listcomp>"], "z", "Unknown");
        Ok(())
    }

    /// This tests that we understand that `async` comprehensions
    /// do not work according to the synchronous iteration protocol
    #[test]
    fn invalid_async_comprehension() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            async def foo():
                [x async for x in Iterable()]
            class Iterator:
                def __next__(self) -> int:
                    return 42
            class Iterable:
                def __iter__(self) -> Iterator:
                    return Iterator()
            ",
        )?;

        // We currently return `Todo` for all async comprehensions,
        // including comprehensions that have invalid syntax
        assert_scope_ty(&db, "src/a.py", &["foo", "<listcomp>"], "x", "@Todo");

        Ok(())
    }

    #[test]
    fn basic_async_comprehension() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            async def foo():
                [x async for x in AsyncIterable()]
            class AsyncIterator:
                async def __anext__(self) -> int:
                    return 42
            class AsyncIterable:
                def __aiter__(self) -> AsyncIterator:
                    return AsyncIterator()
            ",
        )?;

        // TODO async iterables/iterators! --Alex
        assert_scope_ty(&db, "src/a.py", &["foo", "<listcomp>"], "x", "@Todo");

        Ok(())
    }

    #[test]
    fn invalid_iterable() {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            nonsense = 123
            for x in nonsense:
                pass
            ",
        )
        .unwrap();

        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &["Object of type `Literal[123]` is not iterable"],
        );
    }

    #[test]
    fn new_iteration_protocol_takes_precedence_over_old_style() {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            class NotIterable:
                def __getitem__(self, key: int) -> int:
                    return 42

                __iter__ = None

            for x in NotIterable():
                pass
            ",
        )
        .unwrap();

        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &["Object of type `NotIterable` is not iterable"],
        );
    }

    #[test]
    fn starred_expressions_must_be_iterable() {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            class NotIterable: pass

            class Iterator:
                def __next__(self) -> int:
                    return 42

            class Iterable:
                def __iter__(self) -> Iterator:

            x = [*NotIterable()]
            y = [*Iterable()]
            ",
        )
        .unwrap();

        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &["Object of type `NotIterable` is not iterable"],
        );
    }

    #[test]
    fn yield_from_expression_must_be_iterable() {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            class NotIterable: pass

            class Iterator:
                def __next__(self) -> int:
                    return 42

            class Iterable:
                def __iter__(self) -> Iterator:

            def generator_function():
                yield from Iterable()
                yield from NotIterable()
            ",
        )
        .unwrap();

        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &["Object of type `NotIterable` is not iterable"],
        );
    }

    #[test]
    fn assignment_violates_own_annotation() {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            x: int = 'foo'
            ",
        )
        .unwrap();

        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &[r#"Object of type `Literal["foo"]` is not assignable to `int`"#],
        );
    }

    #[test]
    fn assignment_violates_previous_annotation() {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            x: int
            x = 'foo'
            ",
        )
        .unwrap();

        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &[r#"Object of type `Literal["foo"]` is not assignable to `int`"#],
        );
    }

    #[test]
    fn shadowing_is_ok() {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            x: str = 'foo'
            x: int = 1
            ",
        )
        .unwrap();

        assert_file_diagnostics(&db, "/src/a.py", &[]);
    }

    #[test]
    fn shadowing_parameter_is_ok() {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            def f(x: str):
                x: int = int(x)
            ",
        )
        .unwrap();

        assert_file_diagnostics(&db, "/src/a.py", &[]);
    }

    #[test]
    fn declaration_violates_previous_assignment() {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            x = 1
            x: str
            ",
        )
        .unwrap();

        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &[r"Cannot declare type `str` for inferred type `Literal[1]`"],
        );
    }

    #[test]
    fn incompatible_declarations() {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            if flag:
                x: str
            else:
                x: int
            x = 1
            ",
        )
        .unwrap();

        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &[r"Conflicting declared types for `x`: str, int"],
        );
    }

    #[test]
    fn partial_declarations() {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            if flag:
                x: int
            x = 1
            ",
        )
        .unwrap();

        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &[r"Conflicting declared types for `x`: Unknown, int"],
        );
    }

    #[test]
    fn incompatible_declarations_bad_assignment() {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            if flag:
                x: str
            else:
                x: int
            x = b'foo'
            ",
        )
        .unwrap();

        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &[
                r"Conflicting declared types for `x`: str, int",
                r#"Object of type `Literal[b"foo"]` is not assignable to `str | int`"#,
            ],
        );
    }

    #[test]
    fn partial_declarations_questionable_assignment() {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            if flag:
                x: int
            x = 'foo'
            ",
        )
        .unwrap();

        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &[r"Conflicting declared types for `x`: Unknown, int"],
        );
    }

    #[test]
    fn shadow_after_incompatible_declarations_is_ok() {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            if flag:
                x: str
            else:
                x: int
            x: bytes = b'foo'
            ",
        )
        .unwrap();

        assert_file_diagnostics(&db, "/src/a.py", &[]);
    }

    #[test]
    fn no_implicit_shadow_function() {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            def f(): pass
            f = 1
            ",
        )
        .unwrap();

        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &["Implicit shadowing of function `f`; annotate to make it explicit if this is intentional"],
        );
    }

    #[test]
    fn no_implicit_shadow_class() {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            class C: pass
            C = 1
            ",
        )
        .unwrap();

        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &["Implicit shadowing of class `C`; annotate to make it explicit if this is intentional"],
        );
    }

    #[test]
    fn explicit_shadow_function() {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            def f(): pass
            f: int = 1
            ",
        )
        .unwrap();

        assert_file_diagnostics(&db, "/src/a.py", &[]);
    }

    #[test]
    fn explicit_shadow_class() {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            class C(): pass
            C: int = 1
            ",
        )
        .unwrap();

        assert_file_diagnostics(&db, "/src/a.py", &[]);
    }

    #[test]
    fn no_implicit_shadow_import() {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            from b import x

            x = 'foo'
            ",
        )
        .unwrap();

        db.write_file("/src/b.py", "x: int").unwrap();

        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &[r#"Object of type `Literal["foo"]` is not assignable to `int`"#],
        );
    }

    #[test]
    fn import_from_conditional_reimport() {
        let mut db = setup_db();

        db.write_file("/src/a.py", "from b import f").unwrap();
        db.write_dedented(
            "/src/b.py",
            "
            if flag:
                from c import f
            else:
                def f(): ...
            ",
        )
        .unwrap();
        db.write_file("/src/c.py", "def f(): ...").unwrap();

        // TODO we should really disambiguate in such cases: Literal[b.f, c.f]
        assert_public_ty(&db, "/src/a.py", "f", "Literal[f, f]");
    }

    #[test]
    fn import_from_conditional_reimport_vs_non_declaration() {
        let mut db = setup_db();

        db.write_file("/src/a.py", "from b import x").unwrap();
        db.write_dedented(
            "/src/b.py",
            "
            if flag:
                from c import x
            else:
                x = 1
            ",
        )
        .unwrap();
        db.write_file("/src/c.pyi", "x: int").unwrap();

        assert_public_ty(&db, "/src/a.py", "x", "int");
    }

    // Incremental inference tests

    fn first_public_binding<'db>(db: &'db TestDb, file: File, name: &str) -> Definition<'db> {
        let scope = global_scope(db, file);
        use_def_map(db, scope)
            .public_bindings(symbol_table(db, scope).symbol_id_by_name(name).unwrap())
            .next()
            .unwrap()
            .binding
    }

    #[test]
    fn dependency_public_symbol_type_change() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("/src/a.py", "from foo import x"),
            ("/src/foo.py", "x = 10\ndef foo(): ..."),
        ])?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();
        let x_ty = global_symbol_ty(&db, a, "x");

        assert_eq!(x_ty.display(&db).to_string(), "Literal[10]");

        // Change `x` to a different value
        db.write_file("/src/foo.py", "x = 20\ndef foo(): ...")?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();

        let x_ty_2 = global_symbol_ty(&db, a, "x");

        assert_eq!(x_ty_2.display(&db).to_string(), "Literal[20]");

        Ok(())
    }

    #[test]
    fn dependency_internal_symbol_change() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("/src/a.py", "from foo import x"),
            ("/src/foo.py", "x = 10\ndef foo(): y = 1"),
        ])?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();
        let x_ty = global_symbol_ty(&db, a, "x");

        assert_eq!(x_ty.display(&db).to_string(), "Literal[10]");

        db.write_file("/src/foo.py", "x = 10\ndef foo(): pass")?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();

        db.clear_salsa_events();

        let x_ty_2 = global_symbol_ty(&db, a, "x");

        assert_eq!(x_ty_2.display(&db).to_string(), "Literal[10]");

        let events = db.take_salsa_events();

        assert_function_query_was_not_run(
            &db,
            infer_definition_types,
            first_public_binding(&db, a, "x"),
            &events,
        );

        Ok(())
    }

    #[test]
    fn dependency_unrelated_symbol() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("/src/a.py", "from foo import x"),
            ("/src/foo.py", "x = 10\ny = 20"),
        ])?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();
        let x_ty = global_symbol_ty(&db, a, "x");

        assert_eq!(x_ty.display(&db).to_string(), "Literal[10]");

        db.write_file("/src/foo.py", "x = 10\ny = 30")?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();

        db.clear_salsa_events();

        let x_ty_2 = global_symbol_ty(&db, a, "x");

        assert_eq!(x_ty_2.display(&db).to_string(), "Literal[10]");

        let events = db.take_salsa_events();

        assert_function_query_was_not_run(
            &db,
            infer_definition_types,
            first_public_binding(&db, a, "x"),
            &events,
        );
        Ok(())
    }

    #[test]
    fn subscript_tuple() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            t = (1, 'a', 'b')

            a = t[0]
            b = t[1]
            c = t[-1]
            d = t[-2]
            e = t[4]
            f = t[-4]
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "Literal[1]");
        assert_public_ty(&db, "/src/a.py", "b", "Literal[\"a\"]");
        assert_public_ty(&db, "/src/a.py", "c", "Literal[\"b\"]");
        assert_public_ty(&db, "/src/a.py", "d", "Literal[\"a\"]");
        assert_public_ty(&db, "/src/a.py", "e", "Unknown");
        assert_public_ty(&db, "/src/a.py", "f", "Unknown");

        assert_file_diagnostics(
            &db,
            "src/a.py",
            &["Index 4 is out of bounds for tuple of type `tuple[Literal[1], Literal[\"a\"], Literal[\"b\"]]` with length 3", "Index -4 is out of bounds for tuple of type `tuple[Literal[1], Literal[\"a\"], Literal[\"b\"]]` with length 3"],
        );

        Ok(())
    }

    #[test]
    fn subscript_literal_string() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            s = 'abcde'

            a = s[0]
            b = s[1]
            c = s[-1]
            d = s[-2]
            e = s[8]
            f = s[-8]
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "Literal[\"a\"]");
        assert_public_ty(&db, "/src/a.py", "b", "Literal[\"b\"]");
        assert_public_ty(&db, "/src/a.py", "c", "Literal[\"e\"]");
        assert_public_ty(&db, "/src/a.py", "d", "Literal[\"d\"]");
        assert_public_ty(&db, "/src/a.py", "e", "Unknown");
        assert_public_ty(&db, "/src/a.py", "f", "Unknown");

        assert_file_diagnostics(
            &db,
            "src/a.py",
            &[
                "Index 8 is out of bounds for string `Literal[\"abcde\"]` with length 5",
                "Index -8 is out of bounds for string `Literal[\"abcde\"]` with length 5",
            ],
        );

        Ok(())
    }

    #[test]
    fn subscript_getitem_unbound() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
                class NotSubscriptable:
                    pass

                a = NotSubscriptable()[0]
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "Unknown");
        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &["Cannot subscript object of type `NotSubscriptable` with no `__getitem__` method"],
        );

        Ok(())
    }

    #[test]
    fn subscript_class_getitem_unbound() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
                class NotSubscriptable:
                    pass

                a = NotSubscriptable[0]
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "Unknown");
        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &["Cannot subscript object of type `Literal[NotSubscriptable]` with no `__class_getitem__` method"],
        );

        Ok(())
    }

    #[test]
    fn subscript_not_callable_getitem() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
                class NotSubscriptable:
                    __getitem__ = None

                a = NotSubscriptable()[0]
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "Unknown");
        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &["Method `__getitem__` of type `None` is not callable on object of type `NotSubscriptable`"],
        );

        Ok(())
    }

    #[test]
    fn subscript_str_literal() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
                def add(x: int, y: int) -> int:
                    return x + y

                a = 'abcde'[add(0, 1)]
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "str");

        Ok(())
    }

    #[test]
    fn subscript_getitem() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
                class Identity:
                    def __getitem__(self, index: int) -> int:
                        return index

                a = Identity()[0]
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "int");

        Ok(())
    }

    #[test]
    fn subscript_class_getitem() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
                class Identity:
                    def __class_getitem__(cls, item: int) -> str:
                        return item

                a = Identity[0]
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "str");

        Ok(())
    }

    #[test]
    fn subscript_getitem_union() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
                flag = True

                class Identity:
                    if flag:
                        def __getitem__(self, index: int) -> int:
                            return index
                    else:
                        def __getitem__(self, index: int) -> str:
                            return str(index)

                a = Identity()[0]
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "int | str");

        Ok(())
    }

    #[test]
    fn subscript_class_getitem_union() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
                flag = True

                class Identity:
                    if flag:
                        def __class_getitem__(cls, item: int) -> str:
                            return item
                    else:
                        def __class_getitem__(cls, item: int) -> int:
                            return item

                a = Identity[0]
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "str | int");

        Ok(())
    }

    #[test]
    fn subscript_class_getitem_class_union() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
                flag = True

                class Identity1:
                    def __class_getitem__(cls, item: int) -> str:
                        return item

                class Identity2:
                    def __class_getitem__(cls, item: int) -> int:
                        return item

                if flag:
                    a = Identity1
                else:
                    a = Identity2

                b = a[0]
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "Literal[Identity1, Identity2]");
        assert_public_ty(&db, "/src/a.py", "b", "str | int");

        Ok(())
    }

    #[test]
    fn subscript_class_getitem_unbound_method_union() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
                flag = True

                if flag:
                    class Identity:
                        def __class_getitem__(self, x: int) -> str:
                            pass
                else:
                    class Identity:
                        pass

                a = Identity[42]
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "str | Unknown");

        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &["Method `__class_getitem__` of type `Literal[__class_getitem__] | Unbound` is not callable on object of type `Literal[Identity, Identity]`"],
        );

        Ok(())
    }

    #[test]
    fn subscript_class_getitem_non_class_union() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
                flag = True

                if flag:
                    class Identity:
                        def __class_getitem__(self, x: int) -> str:
                            pass
                else:
                    Identity = 1

                a = Identity[42]
            ",
        )?;

        // TODO this should _probably_ emit `str | Unknown` instead of `Unknown`.
        assert_public_ty(&db, "/src/a.py", "a", "Unknown");

        assert_file_diagnostics(
            &db,
            "/src/a.py",
            &["Cannot subscript object of type `Literal[Identity] | Literal[1]` with no `__getitem__` method"],
        );

        Ok(())
    }

    #[test]
    fn dunder_call() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
                class Multiplier:
                    def __init__(self, factor: float):
                        self.factor = factor

                    def __call__(self, number: float) -> float:
                        return number * self.factor

                a = Multiplier(2.0)(3.0)

                class Unit:
                    ...

                b = Unit()(3.0)
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "float");
        assert_public_ty(&db, "/src/a.py", "b", "Unknown");

        assert_file_diagnostics(&db, "src/a.py", &["Object of type `Unit` is not callable"]);

        Ok(())
    }

    #[test]
    fn boolean_or_expression() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            def foo() -> str:
                pass

            a = True or False
            b = 'x' or 'y' or 'z'
            c = '' or 'y' or 'z'
            d = False or 'z'
            e = False or True
            f = False or False
            g = foo() or False
            h = foo() or True
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "Literal[True]");
        assert_public_ty(&db, "/src/a.py", "b", r#"Literal["x"]"#);
        assert_public_ty(&db, "/src/a.py", "c", r#"Literal["y"]"#);
        assert_public_ty(&db, "/src/a.py", "d", r#"Literal["z"]"#);
        assert_public_ty(&db, "/src/a.py", "e", "Literal[True]");
        assert_public_ty(&db, "/src/a.py", "f", "Literal[False]");
        assert_public_ty(&db, "/src/a.py", "g", "str | Literal[False]");
        assert_public_ty(&db, "/src/a.py", "h", "str | Literal[True]");

        Ok(())
    }

    #[test]
    fn boolean_and_expression() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            def foo() -> str:
                pass

            a = True and False
            b = False and True
            c = foo() and False
            d = foo() and True
            e = 'x' and 'y' and 'z'
            f = 'x' and 'y' and ''
            g = '' and 'y'
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "Literal[False]");
        assert_public_ty(&db, "/src/a.py", "b", "Literal[False]");
        assert_public_ty(&db, "/src/a.py", "c", "str | Literal[False]");
        assert_public_ty(&db, "/src/a.py", "d", "str | Literal[True]");
        assert_public_ty(&db, "/src/a.py", "e", r#"Literal["z"]"#);
        assert_public_ty(&db, "/src/a.py", "f", r#"Literal[""]"#);
        assert_public_ty(&db, "/src/a.py", "g", r#"Literal[""]"#);
        Ok(())
    }

    #[test]
    fn boolean_complex_expression() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            r#"
            def foo() -> str:
                pass

            a = "x" and "y" or "z"
            b = "x" or "y" and "z"
            c = "" and "y" or "z"
            d = "" or "y" and "z"
            e = "x" and "y" or ""
            f = "x" or "y" and ""

            "#,
        )?;

        assert_public_ty(&db, "/src/a.py", "a", r#"Literal["y"]"#);
        assert_public_ty(&db, "/src/a.py", "b", r#"Literal["x"]"#);
        assert_public_ty(&db, "/src/a.py", "c", r#"Literal["z"]"#);
        assert_public_ty(&db, "/src/a.py", "d", r#"Literal["z"]"#);
        assert_public_ty(&db, "/src/a.py", "e", r#"Literal["y"]"#);
        assert_public_ty(&db, "/src/a.py", "f", r#"Literal["x"]"#);
        Ok(())
    }

    #[test]
    fn bool_function_falsy_values() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
            a = bool(0)
            b = bool(())
            c = bool(None)
            d = bool("")
            e = bool(False)
            f = bool()
            "#,
        )?;
        assert_public_ty(&db, "/src/a.py", "a", "Literal[False]");
        assert_public_ty(&db, "/src/a.py", "b", "Literal[False]");
        assert_public_ty(&db, "/src/a.py", "c", "Literal[False]");
        assert_public_ty(&db, "/src/a.py", "d", "Literal[False]");
        assert_public_ty(&db, "/src/a.py", "e", "Literal[False]");
        assert_public_ty(&db, "/src/a.py", "f", "Literal[False]");
        Ok(())
    }

    #[test]
    fn builtin_bool_function_detected() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            "
            redefined_builtin_bool = bool

            def my_bool(x)-> bool: pass
            ",
        )?;
        db.write_dedented(
            "/src/b.py",
            "
            from a import redefined_builtin_bool, my_bool
            a = redefined_builtin_bool(0)
            b = my_bool(0)
            ",
        )?;
        assert_public_ty(&db, "/src/b.py", "a", "Literal[False]");
        assert_public_ty(&db, "/src/b.py", "b", "bool");
        Ok(())
    }

    #[test]
    fn bool_function_truthy_values() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
            a = bool(1)
            b = bool((0,))
            c = bool("NON EMPTY")
            d = bool(True)

            def foo(): pass
            e = bool(foo)
            "#,
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "Literal[True]");
        assert_public_ty(&db, "/src/a.py", "b", "Literal[True]");
        assert_public_ty(&db, "/src/a.py", "c", "Literal[True]");
        assert_public_ty(&db, "/src/a.py", "d", "Literal[True]");
        assert_public_ty(&db, "/src/a.py", "e", "Literal[True]");
        Ok(())
    }

    #[test]
    fn bool_function_ambiguous_values() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            "
            a = bool([])
            b = bool({})
            c = bool(set())
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "bool");
        assert_public_ty(&db, "/src/a.py", "b", "bool");
        assert_public_ty(&db, "/src/a.py", "c", "bool");
        Ok(())
    }

    #[test]
    fn unary_add() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            a = +0
            b = +1
            c = +True
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "Literal[0]");
        assert_public_ty(&db, "/src/a.py", "b", "Literal[1]");
        assert_public_ty(&db, "/src/a.py", "c", "Literal[1]");
        Ok(())
    }

    #[test]
    fn unary_sub() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            a = -0
            b = -1
            c = -True
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "Literal[0]");
        assert_public_ty(&db, "/src/a.py", "b", "Literal[-1]");
        assert_public_ty(&db, "/src/a.py", "c", "Literal[-1]");
        Ok(())
    }

    #[test]
    fn unary_invert() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            a = ~0
            b = ~1
            c = ~True
            ",
        )?;

        assert_public_ty(&db, "/src/a.py", "a", "Literal[-1]");
        assert_public_ty(&db, "/src/a.py", "b", "Literal[-2]");
        assert_public_ty(&db, "/src/a.py", "c", "Literal[-2]");
        Ok(())
    }
}
