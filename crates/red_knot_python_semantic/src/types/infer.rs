//! We have three Salsa queries for inferring types at three different granularities: scope-level,
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
//! The expression-level inference query is needed in only a few cases. Since an assignment
//! statement can have multiple targets (via `x = y = z` or unpacking `(x, y) = z`, it can be
//! associated with multiple definitions. In order to avoid inferring the type of the right-hand
//! side once per definition, we infer it as a standalone query, so its result will be cached by
//! Salsa. We also need the expression-level query for inferring types in type guard expressions
//! (e.g. the test clause of an `if` statement.)
//!
//! Inferring types at any of the three region granularities returns a [`TypeInference`], which
//! holds types for every [`Definition`] and expression within the inferred region.
use std::num::NonZeroU32;

use rustc_hash::FxHashMap;
use salsa;
use salsa::plumbing::AsId;

use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, UnaryOp};
use ruff_python_ast::{AnyNodeRef, ExprContext};
use ruff_text_size::Ranged;

use crate::builtins::builtins_scope;
use crate::module_name::ModuleName;
use crate::module_resolver::{file_to_module, resolve_module};
use crate::semantic_index::ast_ids::{HasScopedAstId, HasScopedUseId, ScopedExpressionId};
use crate::semantic_index::definition::{Definition, DefinitionKind, DefinitionNodeKey};
use crate::semantic_index::expression::Expression;
use crate::semantic_index::semantic_index;
use crate::semantic_index::symbol::{FileScopeId, NodeWithScopeKind, NodeWithScopeRef, ScopeId};
use crate::semantic_index::SemanticIndex;
use crate::types::diagnostic::{TypeCheckDiagnostic, TypeCheckDiagnostics};
use crate::types::{
    builtins_symbol_ty_by_name, definitions_ty, global_symbol_ty_by_name, BytesLiteralType,
    ClassType, FunctionType, Name, StringLiteralType, Type, UnionBuilder,
};
use crate::Db;

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
    _db: &'db dyn Db,
    _cycle: &salsa::Cycle,
    input: Definition<'db>,
) -> TypeInference<'db> {
    let mut inference = TypeInference::default();
    inference.definitions.insert(input, Type::Unknown);
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
    Expression(Expression<'db>),
    Definition(Definition<'db>),
    Scope(ScopeId<'db>),
}

/// The inferred types for a single region.
#[derive(Debug, Eq, PartialEq, Default)]
pub(crate) struct TypeInference<'db> {
    /// The types of every expression in this region.
    expressions: FxHashMap<ScopedExpressionId, Type<'db>>,

    /// The types of every definition in this region.
    definitions: FxHashMap<Definition<'db>, Type<'db>>,

    /// The diagnostics for this region.
    diagnostics: TypeCheckDiagnostics,
}

impl<'db> TypeInference<'db> {
    pub(crate) fn expression_ty(&self, expression: ScopedExpressionId) -> Type<'db> {
        self.expressions[&expression]
    }

    pub(crate) fn definition_ty(&self, definition: Definition<'db>) -> Type<'db> {
        self.definitions[&definition]
    }

    pub(crate) fn diagnostics(&self) -> &[std::sync::Arc<TypeCheckDiagnostic>] {
        &self.diagnostics
    }

    fn shrink_to_fit(&mut self) {
        self.expressions.shrink_to_fit();
        self.definitions.shrink_to_fit();
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
struct TypeInferenceBuilder<'db> {
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
            InferenceRegion::Definition(definition) => (definition.file(db), definition.scope(db)),
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
        self.types.definitions.extend(inference.definitions.iter());
        self.types.expressions.extend(inference.expressions.iter());
        self.types.diagnostics.extend(&inference.diagnostics);
    }

    /// Infers types in the given [`InferenceRegion`].
    fn infer_region(&mut self) {
        match self.region {
            InferenceRegion::Scope(scope) => self.infer_region_scope(scope),
            InferenceRegion::Definition(definition) => self.infer_region_definition(definition),
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
    }

    fn infer_region_definition(&mut self, definition: Definition<'db>) {
        match definition.node(self.db) {
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
                    definition,
                );
            }
            DefinitionKind::NamedExpression(named_expression) => {
                self.infer_named_expression_definition(named_expression.node(), definition);
            }
            DefinitionKind::Comprehension(comprehension) => {
                self.infer_comprehension_definition(
                    comprehension.node(),
                    comprehension.is_first(),
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
        }
    }

    fn infer_region_expression(&mut self, expression: Expression<'db>) {
        self.infer_expression(expression.node_ref(self.db));
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
        let Some(type_params) = function.type_params.as_deref() else {
            panic!("function type params scope without type params");
        };
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

        // If there are type params, parameters and returns are evaluated in that scope.
        if type_params.is_none() {
            self.infer_parameters(parameters);
            self.infer_optional_expression(returns.as_deref());
        }

        let function_ty =
            Type::Function(FunctionType::new(self.db, name.id.clone(), decorator_tys));

        self.types.definitions.insert(definition, function_ty);
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
        _parameter_with_default: &ast::ParameterWithDefault,
        definition: Definition<'db>,
    ) {
        // TODO(dhruvmanila): Infer types from annotation or default expression
        self.types.definitions.insert(definition, Type::Unknown);
    }

    fn infer_parameter_definition(
        &mut self,
        _parameter: &ast::Parameter,
        definition: Definition<'db>,
    ) {
        // TODO(dhruvmanila): Annotation expression is resolved at the enclosing scope, infer the
        // parameter type from there
        self.types.definitions.insert(definition, Type::Unknown);
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
            arguments,
            body: _,
        } = class;

        for decorator in decorator_list {
            self.infer_decorator(decorator);
        }

        // TODO if there are type params, the bases should be inferred inside that scope (only)

        let bases = arguments
            .as_deref()
            .map(|arguments| self.infer_arguments(arguments))
            .unwrap_or(Vec::new());

        let body_scope = self
            .index
            .node_scope(NodeWithScopeRef::Class(class))
            .to_scope_id(self.db, self.file);

        let class_ty = Type::Class(ClassType::new(self.db, name.id.clone(), bases, body_scope));

        self.types.definitions.insert(definition, class_ty);
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
            self.infer_optional_expression(handler.type_.as_deref());
            self.infer_body(&handler.body);
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
            match item.optional_vars.as_deref() {
                Some(ast::Expr::Name(name)) => {
                    self.infer_definition(name);
                }
                _ => {
                    // TODO infer definitions in unpacking assignment
                    self.infer_expression(&item.context_expr);
                }
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
        let context_expr_ty = self
            .types
            .expression_ty(with_item.context_expr.scoped_ast_id(self.db, self.scope));

        self.types
            .expressions
            .insert(target.scoped_ast_id(self.db, self.scope), context_expr_ty);
        self.types.definitions.insert(definition, context_expr_ty);
    }

    fn infer_match_statement(&mut self, match_statement: &ast::StmtMatch) {
        let ast::StmtMatch {
            range: _,
            subject,
            cases,
        } = match_statement;

        self.infer_expression(subject);
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

    fn infer_match_pattern(&mut self, pattern: &ast::Pattern) {
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

        // TODO remove once we infer definitions in unpacking assignment, since that infers the RHS
        // too, and uses the `infer_expression_types` query to do it
        self.infer_expression(value);

        for target in targets {
            match target {
                ast::Expr::Name(name) => {
                    self.infer_definition(name);
                }
                _ => {
                    // TODO infer definitions in unpacking assignment
                    self.infer_expression(target);
                }
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
        let value_ty = self
            .types
            .expression_ty(assignment.value.scoped_ast_id(self.db, self.scope));
        self.types
            .expressions
            .insert(target.scoped_ast_id(self.db, self.scope), value_ty);
        self.types.definitions.insert(definition, value_ty);
    }

    fn infer_annotated_assignment_statement(&mut self, assignment: &ast::StmtAnnAssign) {
        if let ast::Expr::Name(_) = assignment.target.as_ref() {
            self.infer_definition(assignment);
        } else {
            // currently we don't consider assignments to non-Names to be Definitions
            self.infer_annotated_assignment(assignment);
        }
    }

    fn infer_annotated_assignment_definition(
        &mut self,
        assignment: &ast::StmtAnnAssign,
        definition: Definition<'db>,
    ) {
        let ty = self.infer_annotated_assignment(assignment);
        self.types.definitions.insert(definition, ty);
    }

    fn infer_annotated_assignment(&mut self, assignment: &ast::StmtAnnAssign) -> Type<'db> {
        let ast::StmtAnnAssign {
            range: _,
            target,
            annotation,
            value,
            simple: _,
        } = assignment;

        self.infer_optional_expression(value.as_deref());

        let annotation_ty = self.infer_expression(annotation);

        self.infer_expression(target);

        annotation_ty
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
        self.types.definitions.insert(definition, target_ty);
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
        Type::Unknown
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

    fn infer_for_statement_definition(
        &mut self,
        target: &ast::ExprName,
        iterable: &ast::Expr,
        definition: Definition<'db>,
    ) {
        let expression = self.index.expression(iterable);
        let result = infer_expression_types(self.db, expression);
        self.extend(result);
        let iterable_ty = self
            .types
            .expression_ty(iterable.scoped_ast_id(self.db, self.scope));

        // TODO(Alex): only a valid iterable if the *type* of `iterable_ty` has an `__iter__`
        // member (dunders are never looked up on an instance)
        let _dunder_iter_ty = iterable_ty.member(self.db, &ast::name::Name::from("__iter__"));

        // TODO(Alex):
        // - infer the return type of the `__iter__` method, which gives us the iterator
        // - lookup the `__next__` method on the iterator
        // - infer the return type of the iterator's `__next__` method,
        //   which gives us the type of the variable being bound here
        //   (...or the type of the object being unpacked into multiple definitions, if it's something like
        //   `for k, v in d.items(): ...`)
        let loop_var_value_ty = Type::Unknown;

        self.types
            .expressions
            .insert(target.scoped_ast_id(self.db, self.scope), loop_var_value_ty);
        self.types.definitions.insert(definition, loop_var_value_ty);
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

        self.types.definitions.insert(definition, module_ty);
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
                "Cannot resolve import '{}{}'.",
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
        tracing::trace!("Resolving imported object {alias:?} from statement {import_from:?}");
        let module = module.as_deref();

        let module_name = if let Some(level) = NonZeroU32::new(*level) {
            tracing::trace!(
                "Resolving imported object '{}' from module '{}' relative to file '{}'",
                alias.name,
                format_import_from_module(level.get(), module),
                self.file.path(self.db),
            );
            self.relative_module_name(module, level)
        } else {
            tracing::trace!(
                "Resolving imported object '{}' from module '{}'",
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
                    "Relative module resolution '{}' failed: too many leading dots",
                    format_import_from_module(*level, module),
                );
                self.unresolved_module_diagnostic(import_from, *level, module);
                Type::Unknown
            }
            Err(ModuleNameResolutionError::UnknownCurrentModule) => {
                tracing::debug!(
                    "Relative module resolution '{}' failed; could not resolve file '{}' to a module",
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

        let member_ty = module_ty.member(self.db, &Name::new(&name.id));

        // TODO: What if it's a union where one of the elements is `Unbound`?
        if member_ty.is_unbound() {
            self.add_diagnostic(
                AnyNodeRef::Alias(alias),
                "unresolved-import",
                format_args!(
                    "Module '{}{}' has no member '{name}'",
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
        self.types.definitions.insert(
            definition,
            member_ty.replace_unbound_with(self.db, Type::Unknown),
        );
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
        self.types.expressions.insert(expr_id, ty);

        ty
    }

    fn infer_number_literal_expression(&mut self, literal: &ast::ExprNumberLiteral) -> Type<'db> {
        let ast::ExprNumberLiteral { range: _, value } = literal;

        match value {
            ast::Number::Int(n) => n
                .as_i64()
                .map(Type::IntLiteral)
                .unwrap_or_else(|| builtins_symbol_ty_by_name(self.db, "int").instance()),
            ast::Number::Float(_) => builtins_symbol_ty_by_name(self.db, "float").instance(),
            ast::Number::Complex { .. } => {
                builtins_symbol_ty_by_name(self.db, "complex").instance()
            }
        }
    }

    #[allow(clippy::unused_self)]
    fn infer_boolean_literal_expression(&mut self, literal: &ast::ExprBooleanLiteral) -> Type<'db> {
        let ast::ExprBooleanLiteral { range: _, value } = literal;

        Type::BooleanLiteral(*value)
    }

    fn infer_string_literal_expression(&mut self, literal: &ast::ExprStringLiteral) -> Type<'db> {
        let value = if literal.value.len() <= Self::MAX_STRING_LITERAL_SIZE {
            literal.value.to_str().into()
        } else {
            Box::default()
        };

        Type::StringLiteral(StringLiteralType::new(self.db, value))
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

        for part in value {
            match part {
                ast::FStringPart::Literal(_) => {
                    // TODO string literal type
                }
                ast::FStringPart::FString(fstring) => {
                    let ast::FString {
                        range: _,
                        elements,
                        flags: _,
                    } = fstring;
                    for element in elements {
                        self.infer_fstring_element(element);
                    }
                }
            }
        }

        // TODO str type
        Type::Unknown
    }

    fn infer_fstring_element(&mut self, element: &ast::FStringElement) {
        match element {
            ast::FStringElement::Literal(_) => {
                // TODO string literal type
            }
            ast::FStringElement::Expression(expr_element) => {
                let ast::FStringExpressionElement {
                    range: _,
                    expression,
                    debug_text: _,
                    conversion: _,
                    format_spec,
                } = expr_element;
                self.infer_expression(expression);

                if let Some(format_spec) = format_spec {
                    for spec_element in &format_spec.elements {
                        self.infer_fstring_element(spec_element);
                    }
                }
            }
        }
    }

    #[allow(clippy::unused_self)]
    fn infer_ellipsis_literal_expression(
        &mut self,
        _literal: &ast::ExprEllipsisLiteral,
    ) -> Type<'db> {
        // TODO: this does not currently resolve to `types.EllipsisType` as it
        // should, and it needs support for sealed types (with a single member
        // for singletons like this) as well (#12694).
        builtins_symbol_ty_by_name(self.db, "Ellipsis")
    }

    fn infer_tuple_expression(&mut self, tuple: &ast::ExprTuple) -> Type<'db> {
        let ast::ExprTuple {
            range: _,
            elts,
            ctx: _,
            parenthesized: _,
        } = tuple;

        for elt in elts {
            self.infer_expression(elt);
        }

        // TODO generic
        builtins_symbol_ty_by_name(self.db, "tuple").instance()
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
        builtins_symbol_ty_by_name(self.db, "list").instance()
    }

    fn infer_set_expression(&mut self, set: &ast::ExprSet) -> Type<'db> {
        let ast::ExprSet { range: _, elts } = set;

        for elt in elts {
            self.infer_expression(elt);
        }

        // TODO generic
        builtins_symbol_ty_by_name(self.db, "set").instance()
    }

    fn infer_dict_expression(&mut self, dict: &ast::ExprDict) -> Type<'db> {
        let ast::ExprDict { range: _, items } = dict;

        for item in items {
            self.infer_optional_expression(item.key.as_ref());
            self.infer_expression(&item.value);
        }

        // TODO generic
        builtins_symbol_ty_by_name(self.db, "dict").instance()
    }

    /// Infer the type of the `iter` expression of the first comprehension.
    fn infer_first_comprehension_iter(&mut self, comprehensions: &[ast::Comprehension]) {
        let mut generators_iter = comprehensions.iter();
        let Some(first_generator) = generators_iter.next() else {
            unreachable!("Comprehension must contain at least one generator");
        };
        self.infer_expression(&first_generator.iter);
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
        Type::Unknown
    }

    fn infer_list_comprehension_expression(&mut self, listcomp: &ast::ExprListComp) -> Type<'db> {
        let ast::ExprListComp {
            range: _,
            elt: _,
            generators,
        } = listcomp;

        self.infer_first_comprehension_iter(generators);

        // TODO list type
        Type::Unknown
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
        Type::Unknown
    }

    fn infer_set_comprehension_expression(&mut self, setcomp: &ast::ExprSetComp) -> Type<'db> {
        let ast::ExprSetComp {
            range: _,
            elt: _,
            generators,
        } = setcomp;

        self.infer_first_comprehension_iter(generators);

        // TODO set type
        Type::Unknown
    }

    fn infer_generator_expression_scope(&mut self, generator: &ast::ExprGenerator) {
        let ast::ExprGenerator {
            range: _,
            elt,
            generators,
            parenthesized: _,
        } = generator;

        self.infer_expression(elt);
        for comprehension in generators {
            self.infer_comprehension(comprehension);
        }
    }

    fn infer_list_comprehension_expression_scope(&mut self, listcomp: &ast::ExprListComp) {
        let ast::ExprListComp {
            range: _,
            elt,
            generators,
        } = listcomp;

        self.infer_expression(elt);
        for comprehension in generators {
            self.infer_comprehension(comprehension);
        }
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
        for comprehension in generators {
            self.infer_comprehension(comprehension);
        }
    }

    fn infer_set_comprehension_expression_scope(&mut self, setcomp: &ast::ExprSetComp) {
        let ast::ExprSetComp {
            range: _,
            elt,
            generators,
        } = setcomp;

        self.infer_expression(elt);
        for comprehension in generators {
            self.infer_comprehension(comprehension);
        }
    }

    fn infer_comprehension(&mut self, comprehension: &ast::Comprehension) {
        self.infer_definition(comprehension);
        for expr in &comprehension.ifs {
            self.infer_expression(expr);
        }
    }

    fn infer_comprehension_definition(
        &mut self,
        comprehension: &ast::Comprehension,
        is_first: bool,
        definition: Definition<'db>,
    ) {
        let ast::Comprehension {
            range: _,
            target,
            iter,
            ifs: _,
            is_async: _,
        } = comprehension;

        if !is_first {
            self.infer_expression(iter);
        }
        // TODO(dhruvmanila): The target type should be inferred based on the iter type instead.
        let target_ty = self.infer_expression(target);
        self.types.definitions.insert(definition, target_ty);
    }

    fn infer_named_expression(&mut self, named: &ast::ExprNamed) -> Type<'db> {
        let definition = self.index.definition(named);
        let result = infer_definition_types(self.db, definition);
        self.extend(result);
        result.definition_ty(definition)
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

        self.types.definitions.insert(definition, value_ty);

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

        UnionBuilder::new(self.db)
            .add(body_ty)
            .add(orelse_ty)
            .build()
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
        Type::Unknown
    }

    fn infer_call_expression(&mut self, call_expression: &ast::ExprCall) -> Type<'db> {
        let ast::ExprCall {
            range: _,
            func,
            arguments,
        } = call_expression;

        self.infer_arguments(arguments);
        self.infer_expression(func);

        // TODO resolve to return type of `func`, if its a callable type
        Type::Unknown
    }

    fn infer_starred_expression(&mut self, starred: &ast::ExprStarred) -> Type<'db> {
        let ast::ExprStarred {
            range: _,
            value,
            ctx: _,
        } = starred;

        self.infer_expression(value);

        // TODO
        Type::Unknown
    }

    fn infer_yield_expression(&mut self, yield_expression: &ast::ExprYield) -> Type<'db> {
        let ast::ExprYield { range: _, value } = yield_expression;

        self.infer_optional_expression(value.as_deref());

        // TODO awaitable type
        Type::Unknown
    }

    fn infer_yield_from_expression(&mut self, yield_from: &ast::ExprYieldFrom) -> Type<'db> {
        let ast::ExprYieldFrom { range: _, value } = yield_from;

        self.infer_expression(value);

        // TODO get type from awaitable
        Type::Unknown
    }

    fn infer_await_expression(&mut self, await_expression: &ast::ExprAwait) -> Type<'db> {
        let ast::ExprAwait { range: _, value } = await_expression;

        self.infer_expression(value);

        // TODO awaitable type
        Type::Unknown
    }

    fn infer_name_expression(&mut self, name: &ast::ExprName) -> Type<'db> {
        let ast::ExprName { range: _, id, ctx } = name;

        match ctx {
            ExprContext::Load => {
                let file_scope_id = self.scope.file_scope_id(self.db);
                let use_def = self.index.use_def_map(file_scope_id);
                let use_id = name.scoped_use_id(self.db, self.scope);
                let may_be_unbound = use_def.use_may_be_unbound(use_id);

                let unbound_ty = if may_be_unbound {
                    let symbols = self.index.symbol_table(file_scope_id);
                    // SAFETY: the symbol table always creates a symbol for every Name node.
                    let symbol = symbols.symbol_by_name(id).unwrap();
                    if !symbol.is_defined() || !self.scope.is_function_like(self.db) {
                        // implicit global
                        let unbound_ty = if file_scope_id == FileScopeId::global() {
                            Type::Unbound
                        } else {
                            global_symbol_ty_by_name(self.db, self.file, id)
                        };
                        // fallback to builtins
                        if unbound_ty.may_be_unbound(self.db)
                            && Some(self.scope) != builtins_scope(self.db)
                        {
                            Some(unbound_ty.replace_unbound_with(
                                self.db,
                                builtins_symbol_ty_by_name(self.db, id),
                            ))
                        } else {
                            Some(unbound_ty)
                        }
                    } else {
                        Some(Type::Unbound)
                    }
                } else {
                    None
                };

                definitions_ty(self.db, use_def.use_definitions(use_id), unbound_ty)
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
        let member_ty = value_ty.member(self.db, &Name::new(&attr.id));

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
            (UnaryOp::USub, Type::IntLiteral(value)) => Type::IntLiteral(-value),
            _ => Type::Unknown, // TODO other unary op types
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

        match (left_ty, right_ty, op) {
            (Type::Any, _, _) | (_, Type::Any, _) => Type::Any,
            (Type::Unknown, _, _) | (_, Type::Unknown, _) => Type::Unknown,

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::Add) => n
                .checked_add(m)
                .map(Type::IntLiteral)
                .unwrap_or_else(|| builtins_symbol_ty_by_name(self.db, "int").instance()),

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::Sub) => n
                .checked_sub(m)
                .map(Type::IntLiteral)
                .unwrap_or_else(|| builtins_symbol_ty_by_name(self.db, "int").instance()),

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::Mult) => n
                .checked_mul(m)
                .map(Type::IntLiteral)
                .unwrap_or_else(|| builtins_symbol_ty_by_name(self.db, "int").instance()),

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::Div) => n
                .checked_div(m)
                .map(Type::IntLiteral)
                .unwrap_or_else(|| builtins_symbol_ty_by_name(self.db, "int").instance()),

            (Type::IntLiteral(n), Type::IntLiteral(m), ast::Operator::Mod) => n
                .checked_rem(m)
                .map(Type::IntLiteral)
                // TODO division by zero error
                .unwrap_or(Type::Unknown),

            (Type::BytesLiteral(lhs), Type::BytesLiteral(rhs), ast::Operator::Add) => {
                Type::BytesLiteral(BytesLiteralType::new(
                    self.db,
                    [lhs.value(self.db).as_ref(), rhs.value(self.db).as_ref()]
                        .concat()
                        .into_boxed_slice(),
                ))
            }

            (Type::StringLiteral(lhs), Type::StringLiteral(rhs), ast::Operator::Add) => {
                Type::StringLiteral(StringLiteralType::new(self.db, {
                    let lhs_value = lhs.value(self.db).to_string();
                    let rhs_value = rhs.value(self.db).as_ref();
                    (lhs_value + rhs_value).into()
                }))
            }

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

            _ => Type::Unknown, // TODO
        }
    }

    fn infer_boolean_expression(&mut self, bool_op: &ast::ExprBoolOp) -> Type<'db> {
        let ast::ExprBoolOp {
            range: _,
            op: _,
            values,
        } = bool_op;

        for value in values {
            self.infer_expression(value);
        }

        // TODO resolve bool op
        Type::Unknown
    }

    fn infer_compare_expression(&mut self, compare: &ast::ExprCompare) -> Type<'db> {
        let ast::ExprCompare {
            range: _,
            left,
            ops: _,
            comparators,
        } = compare;

        self.infer_expression(left);
        // TODO actually handle ops and return correct type
        for right in comparators.as_ref() {
            self.infer_expression(right);
        }
        Type::Unknown
    }

    fn infer_subscript_expression(&mut self, subscript: &ast::ExprSubscript) -> Type<'db> {
        let ast::ExprSubscript {
            range: _,
            value,
            slice,
            ctx: _,
        } = subscript;

        self.infer_expression(slice);
        self.infer_expression(value);

        // TODO actual subscript support
        Type::Unknown
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
        Type::Unknown
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
    fn add_diagnostic(&mut self, node: AnyNodeRef, rule: &str, message: std::fmt::Arguments) {
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

#[cfg(test)]
mod tests {

    use anyhow::Context;

    use ruff_db::files::{system_path_to_file, File};
    use ruff_db::parsed::parsed_module;
    use ruff_db::system::{DbWithTestSystem, SystemPathBuf};
    use ruff_db::testing::assert_function_query_was_not_run;
    use ruff_python_ast::name::Name;

    use crate::builtins::builtins_scope;
    use crate::db::tests::TestDb;
    use crate::program::{Program, SearchPathSettings};
    use crate::python_version::PythonVersion;
    use crate::semantic_index::definition::Definition;
    use crate::semantic_index::symbol::FileScopeId;
    use crate::semantic_index::{global_scope, semantic_index, symbol_table, use_def_map};
    use crate::types::{global_symbol_ty_by_name, infer_definition_types, symbol_ty_by_name, Type};
    use crate::{HasTy, ProgramSettings, SemanticModel};

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
        let file = system_path_to_file(db, file_name).expect("Expected file to exist.");

        let ty = global_symbol_ty_by_name(db, file, symbol_name);
        assert_eq!(ty.display(db).to_string(), expected);
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

        let mod_file = system_path_to_file(&db, "src/mod.py").expect("Expected file to exist.");
        let ty = global_symbol_ty_by_name(&db, mod_file, "Sub");

        let Type::Class(class) = ty else {
            panic!("Sub is not a Class")
        };

        let base_names: Vec<_> = class
            .bases(&db)
            .iter()
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
        let ty = global_symbol_ty_by_name(&db, mod_file, "C");

        let Type::Class(class_id) = ty else {
            panic!("C is not a Class");
        };

        let member_ty = class_id.class_member(&db, &Name::new_static("f"));

        let Type::Function(func) = member_ty else {
            panic!("C.f is not a Function");
        };

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

        // TODO: update this once `infer_ellipsis_literal_expression` correctly
        // infers `types.EllipsisType`.
        assert_public_ty(&db, "src/a.py", "x", "Unknown | Literal[EllipsisType]");

        Ok(())
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
    fn literal_int_arithmetic() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/a.py",
            "
            a = 2 + 1
            b = a - 4
            c = a * b
            d = c / 3
            e = 5 % 3
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "a", "Literal[3]");
        assert_public_ty(&db, "src/a.py", "b", "Literal[-1]");
        assert_public_ty(&db, "src/a.py", "c", "Literal[-3]");
        assert_public_ty(&db, "src/a.py", "d", "Literal[-1]");
        assert_public_ty(&db, "src/a.py", "e", "Literal[2]");

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

        assert_public_ty(&db, "src/a.py", "x", "Literal[3] | Unbound");
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
        assert_public_ty(&db, "src/a.py", "r", "Literal[2] | Unbound");
        assert_public_ty(&db, "src/a.py", "s", "Literal[5] | Unbound");
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

        let a = system_path_to_file(&db, "src/a.py").expect("Expected file to exist.");
        let c_ty = global_symbol_ty_by_name(&db, a, "C");
        let Type::Class(c_class) = c_ty else {
            panic!("C is not a Class")
        };
        let c_bases = c_class.bases(&db);
        let b_ty = c_bases.first().unwrap();
        let Type::Class(b_class) = b_ty else {
            panic!("B is not a Class")
        };
        assert_eq!(b_class.name(&db), "B");
        let b_bases = b_class.bases(&db);
        let a_ty = b_bases.first().unwrap();
        let Type::Class(a_class) = a_ty else {
            panic!("A is not a Class")
        };
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

        let file = system_path_to_file(&db, "src/a.py").expect("Expected file to exist.");
        let index = semantic_index(&db, file);
        let function_scope = index
            .child_scopes(FileScopeId::global())
            .next()
            .unwrap()
            .0
            .to_scope_id(&db, file);
        let y_ty = symbol_ty_by_name(&db, function_scope, "y");
        let x_ty = symbol_ty_by_name(&db, function_scope, "x");

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

        let file = system_path_to_file(&db, "src/a.py").expect("Expected file to exist.");
        let index = semantic_index(&db, file);
        let function_scope = index
            .child_scopes(FileScopeId::global())
            .next()
            .unwrap()
            .0
            .to_scope_id(&db, file);
        let y_ty = symbol_ty_by_name(&db, function_scope, "y");
        let x_ty = symbol_ty_by_name(&db, function_scope, "x");

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

        let file = system_path_to_file(&db, "src/a.py").expect("Expected file to exist.");
        let index = semantic_index(&db, file);
        let function_scope = index
            .child_scopes(FileScopeId::global())
            .next()
            .unwrap()
            .0
            .to_scope_id(&db, file);
        let y_ty = symbol_ty_by_name(&db, function_scope, "y");

        assert_eq!(
            y_ty.display(&db).to_string(),
            "Literal[1] | Literal[copyright]"
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

        let file = system_path_to_file(&db, "src/a.py").expect("Expected file to exist.");
        let index = semantic_index(&db, file);
        let class_scope = index
            .child_scopes(FileScopeId::global())
            .next()
            .unwrap()
            .0
            .to_scope_id(&db, file);
        let y_ty = symbol_ty_by_name(&db, class_scope, "y");
        let x_ty = symbol_ty_by_name(&db, class_scope, "x");

        assert_eq!(x_ty.display(&db).to_string(), "Literal[2] | Unbound");
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
        let file = system_path_to_file(&db, "/src/a.py").expect("Expected file to exist.");
        let builtins_ty = global_symbol_ty_by_name(&db, file, "builtins");
        let Type::Module(builtins_file) = builtins_ty else {
            panic!("Builtins are not a module?");
        };
        let implicit_builtins_file = builtins_scope(&db).expect("builtins to exist").file(&db);
        assert_eq!(builtins_file, implicit_builtins_file);

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

        assert_public_ty(&db, "/src/a.py", "x", "Literal[1] | None");
        assert_public_ty(&db, "/src/a.py", "y", "Literal[0, 1]");

        Ok(())
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

    fn first_public_def<'db>(db: &'db TestDb, file: File, name: &str) -> Definition<'db> {
        let scope = global_scope(db, file);
        use_def_map(db, scope)
            .public_definitions(symbol_table(db, scope).symbol_id_by_name(name).unwrap())
            .next()
            .unwrap()
            .definition
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
    fn tuple_literal() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            x = ()
            ",
        )?;

        // TODO should be a generic type
        assert_public_ty(&db, "/src/a.py", "x", "tuple");

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
    fn dependency_public_symbol_type_change() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("/src/a.py", "from foo import x"),
            ("/src/foo.py", "x = 10\ndef foo(): ..."),
        ])?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();
        let x_ty = global_symbol_ty_by_name(&db, a, "x");

        assert_eq!(x_ty.display(&db).to_string(), "Literal[10]");

        // Change `x` to a different value
        db.write_file("/src/foo.py", "x = 20\ndef foo(): ...")?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();

        let x_ty_2 = global_symbol_ty_by_name(&db, a, "x");

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
        let x_ty = global_symbol_ty_by_name(&db, a, "x");

        assert_eq!(x_ty.display(&db).to_string(), "Literal[10]");

        db.write_file("/src/foo.py", "x = 10\ndef foo(): pass")?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();

        db.clear_salsa_events();

        let x_ty_2 = global_symbol_ty_by_name(&db, a, "x");

        assert_eq!(x_ty_2.display(&db).to_string(), "Literal[10]");

        let events = db.take_salsa_events();

        assert_function_query_was_not_run(
            &db,
            infer_definition_types,
            first_public_def(&db, a, "x"),
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
        let x_ty = global_symbol_ty_by_name(&db, a, "x");

        assert_eq!(x_ty.display(&db).to_string(), "Literal[10]");

        db.write_file("/src/foo.py", "x = 10\ny = 30")?;

        let a = system_path_to_file(&db, "/src/a.py").unwrap();

        db.clear_salsa_events();

        let x_ty_2 = global_symbol_ty_by_name(&db, a, "x");

        assert_eq!(x_ty_2.display(&db).to_string(), "Literal[10]");

        let events = db.take_salsa_events();

        assert_function_query_was_not_run(
            &db,
            infer_definition_types,
            first_public_def(&db, a, "x"),
            &events,
        );
        Ok(())
    }
}
