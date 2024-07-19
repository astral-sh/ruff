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
use rustc_hash::FxHashMap;
use salsa;

use red_knot_module_resolver::{resolve_module, ModuleName};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_python_ast::{ExprContext, TypeParams};

use crate::semantic_index::ast_ids::{HasScopedAstId, HasScopedUseId, ScopedExpressionId};
use crate::semantic_index::definition::{Definition, DefinitionKind, DefinitionNodeKey};
use crate::semantic_index::expression::Expression;
use crate::semantic_index::semantic_index;
use crate::semantic_index::symbol::NodeWithScopeKind;
use crate::semantic_index::symbol::{NodeWithScopeRef, ScopeId};
use crate::semantic_index::SemanticIndex;
use crate::types::{
    definitions_ty, global_symbol_ty_by_name, ClassType, FunctionType, Name, Type, UnionTypeBuilder,
};
use crate::Db;

/// Infer all types for a [`ScopeId`], including all definitions and expressions in that scope.
/// Use when checking a scope, or needing to provide a type for an arbitrary expression in the
/// scope.
#[salsa::tracked(return_ref)]
pub(crate) fn infer_scope_types<'db>(db: &'db dyn Db, scope: ScopeId<'db>) -> TypeInference<'db> {
    let file = scope.file(db);
    let _span = tracing::trace_span!("infer_scope_types", ?scope, ?file).entered();

    // Using the index here is fine because the code below depends on the AST anyway.
    // The isolation of the query is by the return inferred types.
    let index = semantic_index(db, file);

    TypeInferenceBuilder::new(db, InferenceRegion::Scope(scope), index).finish()
}

/// Infer all types for a [`Definition`] (including sub-expressions).
/// Use when resolving a symbol name use or public type of a symbol.
#[salsa::tracked(return_ref)]
pub(crate) fn infer_definition_types<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> TypeInference<'db> {
    let file = definition.file(db);
    let _span = tracing::trace_span!("infer_definition_types", ?definition, ?file,).entered();

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
    let _span = tracing::trace_span!("infer_expression_types", ?expression, ?file).entered();

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
#[derive(Debug, Eq, PartialEq, Default, Clone)]
pub(crate) struct TypeInference<'db> {
    /// The types of every expression in this region.
    expressions: FxHashMap<ScopedExpressionId, Type<'db>>,

    /// The types of every definition in this region.
    definitions: FxHashMap<Definition<'db>, Type<'db>>,
}

impl<'db> TypeInference<'db> {
    #[allow(unused)]
    pub(crate) fn expression_ty(&self, expression: ScopedExpressionId) -> Type<'db> {
        self.expressions[&expression]
    }

    pub(crate) fn definition_ty(&self, definition: Definition<'db>) -> Type<'db> {
        self.definitions[&definition]
    }

    fn shrink_to_fit(&mut self) {
        self.expressions.shrink_to_fit();
        self.definitions.shrink_to_fit();
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
            NodeWithScopeKind::Class(class) => self.infer_class_body(class.node()),
            NodeWithScopeKind::ClassTypeParameters(class) => {
                self.infer_class_type_params(class.node());
            }
            NodeWithScopeKind::FunctionTypeParameters(function) => {
                self.infer_function_type_params(function.node());
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
                self.infer_assignment_definition(assignment.assignment(), definition);
            }
            DefinitionKind::AnnotatedAssignment(annotated_assignment) => {
                self.infer_annotated_assignment_definition(annotated_assignment.node(), definition);
            }
            DefinitionKind::NamedExpression(named_expression) => {
                self.infer_named_expression_definition(named_expression.node(), definition);
            }
        }
    }

    fn infer_region_expression(&mut self, expression: Expression<'db>) {
        self.infer_expression(expression.node(self.db));
    }

    fn infer_module(&mut self, module: &ast::ModModule) {
        self.infer_body(&module.body);
    }

    fn infer_class_type_params(&mut self, class: &ast::StmtClassDef) {
        if let Some(type_params) = class.type_params.as_deref() {
            self.infer_type_parameters(type_params);
        }
    }

    fn infer_class_body(&mut self, class: &ast::StmtClassDef) {
        self.infer_body(&class.body);
    }

    fn infer_function_type_params(&mut self, function: &ast::StmtFunctionDef) {
        if let Some(type_params) = function.type_params.as_deref() {
            self.infer_type_parameters(type_params);
        }
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
            ast::Stmt::Assign(assign) => self.infer_assignment_statement(assign),
            ast::Stmt::AnnAssign(assign) => self.infer_annotated_assignment_statement(assign),
            ast::Stmt::For(for_statement) => self.infer_for_statement(for_statement),
            ast::Stmt::Import(import) => self.infer_import_statement(import),
            ast::Stmt::ImportFrom(import) => self.infer_import_from_statement(import),
            ast::Stmt::Break(_) | ast::Stmt::Continue(_) | ast::Stmt::Pass(_) => {
                // No-op
            }
            _ => {}
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
            type_params: _,
            parameters: _,
            returns,
            body: _,
            decorator_list,
        } = function;

        let decorator_tys = decorator_list
            .iter()
            .map(|decorator| self.infer_decorator(decorator))
            .collect();

        // TODO: Infer parameters

        if let Some(return_expr) = returns {
            self.infer_expression(return_expr);
        }

        let function_ty =
            Type::Function(FunctionType::new(self.db, name.id.clone(), decorator_tys));

        self.types.definitions.insert(definition, function_ty);
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

            if let Some(test) = &test {
                self.infer_expression(test);
            }

            self.infer_body(body);
        }
    }

    fn infer_assignment_statement(&mut self, assignment: &ast::StmtAssign) {
        let ast::StmtAssign {
            range: _,
            targets,
            value: _,
        } = assignment;

        for target in targets {
            match target {
                ast::Expr::Name(name) => {
                    self.infer_definition(name);
                }
                _ => todo!("support unpacking assignment"),
            }
        }
    }

    fn infer_assignment_definition(
        &mut self,
        assignment: &ast::StmtAssign,
        definition: Definition<'db>,
    ) {
        let expression = self.index.expression(assignment.value.as_ref());
        let result = infer_expression_types(self.db, expression);
        self.extend(result);
        let value_ty = self
            .types
            .expression_ty(assignment.value.scoped_ast_id(self.db, self.scope));
        self.types.definitions.insert(definition, value_ty);
    }

    fn infer_annotated_assignment_statement(&mut self, assignment: &ast::StmtAnnAssign) {
        self.infer_definition(assignment);
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

        if let Some(value) = value {
            let _ = self.infer_expression(value);
        }

        let annotation_ty = self.infer_expression(annotation);

        self.infer_expression(target);

        self.types.definitions.insert(definition, annotation_ty);
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
        self.infer_expression(target);
        self.infer_body(body);
        self.infer_body(orelse);
    }

    fn infer_import_statement(&mut self, import: &ast::StmtImport) {
        let ast::StmtImport { range: _, names } = import;

        for alias in names {
            self.infer_definition(alias);
        }
    }

    fn infer_import_definition(&mut self, alias: &ast::Alias, definition: Definition<'db>) {
        let ast::Alias {
            range: _,
            name,
            asname: _,
        } = alias;

        let module_ty = self.module_ty_from_name(name);
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

    fn infer_import_from_definition(
        &mut self,
        import_from: &ast::StmtImportFrom,
        alias: &ast::Alias,
        definition: Definition<'db>,
    ) {
        let ast::StmtImportFrom { module, .. } = import_from;
        let module_ty =
            self.module_ty_from_name(module.as_ref().expect("Support relative imports"));

        let ast::Alias {
            range: _,
            name,
            asname: _,
        } = alias;

        let ty = module_ty.member(self.db, &Name::new(&name.id));

        self.types.definitions.insert(definition, ty);
    }

    fn module_ty_from_name(&self, name: &ast::Identifier) -> Type<'db> {
        let module_name = ModuleName::new(&name.id);
        let module =
            module_name.and_then(|module_name| resolve_module(self.db.upcast(), module_name));
        module
            .map(|module| Type::Module(module.file()))
            .unwrap_or(Type::Unbound)
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

    fn infer_expression(&mut self, expression: &ast::Expr) -> Type<'db> {
        let ty = match expression {
            ast::Expr::NoneLiteral(ast::ExprNoneLiteral { range: _ }) => Type::None,
            ast::Expr::NumberLiteral(literal) => self.infer_number_literal_expression(literal),
            ast::Expr::Name(name) => self.infer_name_expression(name),
            ast::Expr::Attribute(attribute) => self.infer_attribute_expression(attribute),
            ast::Expr::BinOp(binary) => self.infer_binary_expression(binary),
            ast::Expr::Named(named) => self.infer_named_expression(named),
            ast::Expr::If(if_expression) => self.infer_if_expression(if_expression),

            _ => todo!("expression type resolution for {:?}", expression),
        };

        let expr_id = expression.scoped_ast_id(self.db, self.scope);
        self.types.expressions.insert(expr_id, ty);

        ty
    }

    #[allow(clippy::unused_self)]
    fn infer_number_literal_expression(&mut self, literal: &ast::ExprNumberLiteral) -> Type<'db> {
        let ast::ExprNumberLiteral { range: _, value } = literal;

        match value {
            ast::Number::Int(n) => {
                // TODO support big int literals
                n.as_i64().map(Type::IntLiteral).unwrap_or(Type::Unknown)
            }
            // TODO builtins.float or builtins.complex
            _ => Type::Unknown,
        }
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

        let union = UnionTypeBuilder::new(self.db)
            .add(body_ty)
            .add(orelse_ty)
            .build();

        Type::Union(union)
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
                        Some(global_symbol_ty_by_name(self.db, self.file, id))
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

    fn infer_binary_expression(&mut self, binary: &ast::ExprBinOp) -> Type<'db> {
        let ast::ExprBinOp {
            left,
            op,
            right,
            range: _,
        } = binary;

        let left_ty = self.infer_expression(left);
        let right_ty = self.infer_expression(right);

        match left_ty {
            Type::Any => Type::Any,
            Type::Unknown => Type::Unknown,
            Type::IntLiteral(n) => {
                match right_ty {
                    Type::IntLiteral(m) => {
                        match op {
                            ast::Operator::Add => n
                                .checked_add(m)
                                .map(Type::IntLiteral)
                                // TODO builtins.int
                                .unwrap_or(Type::Unknown),
                            ast::Operator::Sub => n
                                .checked_sub(m)
                                .map(Type::IntLiteral)
                                // TODO builtins.int
                                .unwrap_or(Type::Unknown),
                            ast::Operator::Mult => n
                                .checked_mul(m)
                                .map(Type::IntLiteral)
                                // TODO builtins.int
                                .unwrap_or(Type::Unknown),
                            ast::Operator::Div => n
                                .checked_div(m)
                                .map(Type::IntLiteral)
                                // TODO builtins.int
                                .unwrap_or(Type::Unknown),
                            ast::Operator::Mod => n
                                .checked_rem(m)
                                .map(Type::IntLiteral)
                                // TODO division by zero error
                                .unwrap_or(Type::Unknown),
                            _ => todo!("complete binop op support for IntLiteral"),
                        }
                    }
                    _ => todo!("complete binop right_ty support for IntLiteral"),
                }
            }
            _ => todo!("complete binop support"),
        }
    }

    fn infer_type_parameters(&mut self, _type_parameters: &TypeParams) {
        todo!("Infer type parameters")
    }

    pub(super) fn finish(mut self) -> TypeInference<'db> {
        self.infer_region();
        self.types.shrink_to_fit();
        self.types
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::files::{system_path_to_file, File};
    use ruff_db::parsed::parsed_module;
    use ruff_db::program::{Program, SearchPathSettings, TargetVersion};
    use ruff_db::system::{DbWithTestSystem, SystemPathBuf};
    use ruff_db::testing::assert_function_query_was_not_run;
    use ruff_python_ast::name::Name;

    use crate::db::tests::TestDb;
    use crate::semantic_index::definition::Definition;
    use crate::semantic_index::semantic_index;
    use crate::semantic_index::symbol::FileScopeId;
    use crate::types::{
        global_scope, global_symbol_ty_by_name, infer_definition_types, symbol_table,
        symbol_ty_by_name, use_def_map, Type,
    };
    use crate::{HasTy, SemanticModel};

    fn setup_db() -> TestDb {
        let db = TestDb::new();

        Program::new(
            &db,
            TargetVersion::Py38,
            SearchPathSettings {
                extra_paths: Vec::new(),
                workspace_root: SystemPathBuf::from("/src"),
                site_packages: None,
                custom_typeshed: None,
            },
        );

        db
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
    fn resolve_literal() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file("src/a.py", "x = 1")?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[1]");

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

    fn first_public_def<'db>(db: &'db TestDb, file: File, name: &str) -> Definition<'db> {
        let scope = global_scope(db, file);
        *use_def_map(db, scope)
            .public_definitions(symbol_table(db, scope).symbol_id_by_name(name).unwrap())
            .first()
            .unwrap()
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

        assert_function_query_was_not_run::<infer_definition_types, _, _>(
            &db,
            |ty| &ty.function,
            &first_public_def(&db, a, "x"),
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

        assert_function_query_was_not_run::<infer_definition_types, _, _>(
            &db,
            |ty| &ty.function,
            &first_public_def(&db, a, "x"),
            &events,
        );
        Ok(())
    }
}
