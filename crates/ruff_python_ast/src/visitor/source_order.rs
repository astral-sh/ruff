use crate::{
    Alias, Arguments, BoolOp, BytesLiteral, CmpOp, Comprehension, Decorator, ElifElseClause,
    ExceptHandler, Expr, FString, FStringElement, Keyword, MatchCase, Mod, Operator, Parameter,
    ParameterWithDefault, Parameters, Pattern, PatternArguments, PatternKeyword, Singleton, Stmt,
    StringLiteral, TypeParam, TypeParams, UnaryOp, WithItem,
};
use crate::{AnyNodeRef, AstNode};

/// Visitor that traverses all nodes recursively in the order they appear in the source.
///
/// If you need a visitor that visits the nodes in the order they're evaluated at runtime,
/// use [`Visitor`](super::Visitor) instead.
pub trait SourceOrderVisitor<'a> {
    #[inline]
    fn enter_node(&mut self, _node: AnyNodeRef<'a>) -> TraversalSignal {
        TraversalSignal::Traverse
    }

    #[inline(always)]
    fn leave_node(&mut self, _node: AnyNodeRef<'a>) {}

    #[inline]
    fn visit_mod(&mut self, module: &'a Mod) {
        walk_module(self, module);
    }

    #[inline]
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        walk_stmt(self, stmt);
    }

    #[inline]
    fn visit_annotation(&mut self, expr: &'a Expr) {
        walk_annotation(self, expr);
    }

    #[inline]
    fn visit_expr(&mut self, expr: &'a Expr) {
        walk_expr(self, expr);
    }

    #[inline]
    fn visit_decorator(&mut self, decorator: &'a Decorator) {
        walk_decorator(self, decorator);
    }

    #[inline]
    fn visit_singleton(&mut self, _singleton: &'a Singleton) {}

    #[inline]
    fn visit_bool_op(&mut self, bool_op: &'a BoolOp) {
        walk_bool_op(self, bool_op);
    }

    #[inline]
    fn visit_operator(&mut self, operator: &'a Operator) {
        walk_operator(self, operator);
    }

    #[inline]
    fn visit_unary_op(&mut self, unary_op: &'a UnaryOp) {
        walk_unary_op(self, unary_op);
    }

    #[inline]
    fn visit_cmp_op(&mut self, cmp_op: &'a CmpOp) {
        walk_cmp_op(self, cmp_op);
    }

    #[inline]
    fn visit_comprehension(&mut self, comprehension: &'a Comprehension) {
        walk_comprehension(self, comprehension);
    }

    #[inline]
    fn visit_except_handler(&mut self, except_handler: &'a ExceptHandler) {
        walk_except_handler(self, except_handler);
    }

    #[inline]
    fn visit_arguments(&mut self, arguments: &'a Arguments) {
        walk_arguments(self, arguments);
    }

    #[inline]
    fn visit_parameters(&mut self, parameters: &'a Parameters) {
        walk_parameters(self, parameters);
    }

    #[inline]
    fn visit_parameter(&mut self, arg: &'a Parameter) {
        walk_parameter(self, arg);
    }

    fn visit_parameter_with_default(&mut self, parameter_with_default: &'a ParameterWithDefault) {
        walk_parameter_with_default(self, parameter_with_default);
    }

    #[inline]
    fn visit_keyword(&mut self, keyword: &'a Keyword) {
        walk_keyword(self, keyword);
    }

    #[inline]
    fn visit_alias(&mut self, alias: &'a Alias) {
        walk_alias(self, alias);
    }

    #[inline]
    fn visit_with_item(&mut self, with_item: &'a WithItem) {
        walk_with_item(self, with_item);
    }

    #[inline]
    fn visit_type_params(&mut self, type_params: &'a TypeParams) {
        walk_type_params(self, type_params);
    }

    #[inline]
    fn visit_type_param(&mut self, type_param: &'a TypeParam) {
        walk_type_param(self, type_param);
    }

    #[inline]
    fn visit_match_case(&mut self, match_case: &'a MatchCase) {
        walk_match_case(self, match_case);
    }

    #[inline]
    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        walk_pattern(self, pattern);
    }

    #[inline]
    fn visit_pattern_arguments(&mut self, pattern_arguments: &'a PatternArguments) {
        walk_pattern_arguments(self, pattern_arguments);
    }

    #[inline]
    fn visit_pattern_keyword(&mut self, pattern_keyword: &'a PatternKeyword) {
        walk_pattern_keyword(self, pattern_keyword);
    }

    #[inline]
    fn visit_body(&mut self, body: &'a [Stmt]) {
        walk_body(self, body);
    }

    #[inline]
    fn visit_elif_else_clause(&mut self, elif_else_clause: &'a ElifElseClause) {
        walk_elif_else_clause(self, elif_else_clause);
    }

    #[inline]
    fn visit_f_string(&mut self, f_string: &'a FString) {
        walk_f_string(self, f_string);
    }

    #[inline]
    fn visit_f_string_element(&mut self, f_string_element: &'a FStringElement) {
        walk_f_string_element(self, f_string_element);
    }

    #[inline]
    fn visit_string_literal(&mut self, string_literal: &'a StringLiteral) {
        walk_string_literal(self, string_literal);
    }

    #[inline]
    fn visit_bytes_literal(&mut self, bytes_literal: &'a BytesLiteral) {
        walk_bytes_literal(self, bytes_literal);
    }
}

pub fn walk_module<'a, V>(visitor: &mut V, module: &'a Mod)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(module);
    if visitor.enter_node(node).is_traverse() {
        match module {
            Mod::Module(module) => module.visit_source_order(visitor),
            Mod::Expression(module) => module.visit_source_order(visitor),
        }
    }

    visitor.leave_node(node);
}

pub fn walk_body<'a, V>(visitor: &mut V, body: &'a [Stmt])
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    for stmt in body {
        visitor.visit_stmt(stmt);
    }
}

pub fn walk_stmt<'a, V>(visitor: &mut V, stmt: &'a Stmt)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(stmt);

    if visitor.enter_node(node).is_traverse() {
        match stmt {
            Stmt::Expr(stmt) => stmt.visit_source_order(visitor),
            Stmt::FunctionDef(stmt) => stmt.visit_source_order(visitor),
            Stmt::ClassDef(stmt) => stmt.visit_source_order(visitor),
            Stmt::Return(stmt) => stmt.visit_source_order(visitor),
            Stmt::Delete(stmt) => stmt.visit_source_order(visitor),
            Stmt::TypeAlias(stmt) => stmt.visit_source_order(visitor),
            Stmt::Assign(stmt) => stmt.visit_source_order(visitor),
            Stmt::AugAssign(stmt) => stmt.visit_source_order(visitor),
            Stmt::AnnAssign(stmt) => stmt.visit_source_order(visitor),
            Stmt::For(stmt) => stmt.visit_source_order(visitor),
            Stmt::While(stmt) => stmt.visit_source_order(visitor),
            Stmt::If(stmt) => stmt.visit_source_order(visitor),
            Stmt::With(stmt) => stmt.visit_source_order(visitor),
            Stmt::Match(stmt) => stmt.visit_source_order(visitor),
            Stmt::Raise(stmt) => stmt.visit_source_order(visitor),
            Stmt::Try(stmt) => stmt.visit_source_order(visitor),
            Stmt::Assert(stmt) => stmt.visit_source_order(visitor),
            Stmt::Import(stmt) => stmt.visit_source_order(visitor),
            Stmt::ImportFrom(stmt) => stmt.visit_source_order(visitor),
            Stmt::Pass(stmt) => stmt.visit_source_order(visitor),
            Stmt::Break(stmt) => stmt.visit_source_order(visitor),
            Stmt::Continue(stmt) => stmt.visit_source_order(visitor),
            Stmt::Global(stmt) => stmt.visit_source_order(visitor),
            Stmt::Nonlocal(stmt) => stmt.visit_source_order(visitor),
            Stmt::IpyEscapeCommand(stmt) => stmt.visit_source_order(visitor),
        }
    }

    visitor.leave_node(node);
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum TraversalSignal {
    Traverse,
    Skip,
}

impl TraversalSignal {
    pub const fn is_traverse(self) -> bool {
        matches!(self, TraversalSignal::Traverse)
    }
}

pub fn walk_annotation<'a, V: SourceOrderVisitor<'a> + ?Sized>(visitor: &mut V, expr: &'a Expr) {
    let node = AnyNodeRef::from(expr);
    if visitor.enter_node(node).is_traverse() {
        visitor.visit_expr(expr);
    }

    visitor.leave_node(node);
}

pub fn walk_decorator<'a, V>(visitor: &mut V, decorator: &'a Decorator)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(decorator);
    if visitor.enter_node(node).is_traverse() {
        decorator.visit_source_order(visitor);
    }

    visitor.leave_node(node);
}

pub fn walk_expr<'a, V>(visitor: &mut V, expr: &'a Expr)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(expr);
    if visitor.enter_node(node).is_traverse() {
        match expr {
            Expr::BoolOp(expr) => expr.visit_source_order(visitor),
            Expr::Named(expr) => expr.visit_source_order(visitor),
            Expr::BinOp(expr) => expr.visit_source_order(visitor),
            Expr::UnaryOp(expr) => expr.visit_source_order(visitor),
            Expr::Lambda(expr) => expr.visit_source_order(visitor),
            Expr::If(expr) => expr.visit_source_order(visitor),
            Expr::Dict(expr) => expr.visit_source_order(visitor),
            Expr::Set(expr) => expr.visit_source_order(visitor),
            Expr::ListComp(expr) => expr.visit_source_order(visitor),
            Expr::SetComp(expr) => expr.visit_source_order(visitor),
            Expr::DictComp(expr) => expr.visit_source_order(visitor),
            Expr::Generator(expr) => expr.visit_source_order(visitor),
            Expr::Await(expr) => expr.visit_source_order(visitor),
            Expr::Yield(expr) => expr.visit_source_order(visitor),
            Expr::YieldFrom(expr) => expr.visit_source_order(visitor),
            Expr::Compare(expr) => expr.visit_source_order(visitor),
            Expr::Call(expr) => expr.visit_source_order(visitor),
            Expr::FString(expr) => expr.visit_source_order(visitor),
            Expr::StringLiteral(expr) => expr.visit_source_order(visitor),
            Expr::BytesLiteral(expr) => expr.visit_source_order(visitor),
            Expr::NumberLiteral(expr) => expr.visit_source_order(visitor),
            Expr::BooleanLiteral(expr) => expr.visit_source_order(visitor),
            Expr::NoneLiteral(expr) => expr.visit_source_order(visitor),
            Expr::EllipsisLiteral(expr) => expr.visit_source_order(visitor),
            Expr::Attribute(expr) => expr.visit_source_order(visitor),
            Expr::Subscript(expr) => expr.visit_source_order(visitor),
            Expr::Starred(expr) => expr.visit_source_order(visitor),
            Expr::Name(expr) => expr.visit_source_order(visitor),
            Expr::List(expr) => expr.visit_source_order(visitor),
            Expr::Tuple(expr) => expr.visit_source_order(visitor),
            Expr::Slice(expr) => expr.visit_source_order(visitor),
            Expr::IpyEscapeCommand(expr) => expr.visit_source_order(visitor),
        }
    }

    visitor.leave_node(node);
}

pub fn walk_comprehension<'a, V>(visitor: &mut V, comprehension: &'a Comprehension)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(comprehension);
    if visitor.enter_node(node).is_traverse() {
        comprehension.visit_source_order(visitor);
    }

    visitor.leave_node(node);
}

pub fn walk_elif_else_clause<'a, V>(visitor: &mut V, elif_else_clause: &'a ElifElseClause)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(elif_else_clause);
    if visitor.enter_node(node).is_traverse() {
        elif_else_clause.visit_source_order(visitor);
    }

    visitor.leave_node(node);
}

pub fn walk_except_handler<'a, V>(visitor: &mut V, except_handler: &'a ExceptHandler)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(except_handler);
    if visitor.enter_node(node).is_traverse() {
        match except_handler {
            ExceptHandler::ExceptHandler(except_handler) => {
                except_handler.visit_source_order(visitor);
            }
        }
    }
    visitor.leave_node(node);
}

pub fn walk_format_spec<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    format_spec: &'a Expr,
) {
    let node = AnyNodeRef::from(format_spec);
    if visitor.enter_node(node).is_traverse() {
        visitor.visit_expr(format_spec);
    }

    visitor.leave_node(node);
}

pub fn walk_arguments<'a, V>(visitor: &mut V, arguments: &'a Arguments)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(arguments);
    if visitor.enter_node(node).is_traverse() {
        arguments.visit_source_order(visitor);
    }

    visitor.leave_node(node);
}

pub fn walk_parameters<'a, V>(visitor: &mut V, parameters: &'a Parameters)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(parameters);
    if visitor.enter_node(node).is_traverse() {
        parameters.visit_source_order(visitor);
    }

    visitor.leave_node(node);
}

pub fn walk_parameter<'a, V>(visitor: &mut V, parameter: &'a Parameter)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(parameter);

    if visitor.enter_node(node).is_traverse() {
        parameter.visit_source_order(visitor);
    }
    visitor.leave_node(node);
}

pub fn walk_parameter_with_default<'a, V>(
    visitor: &mut V,
    parameter_with_default: &'a ParameterWithDefault,
) where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(parameter_with_default);
    if visitor.enter_node(node).is_traverse() {
        parameter_with_default.visit_source_order(visitor);
    }

    visitor.leave_node(node);
}

#[inline]
pub fn walk_keyword<'a, V>(visitor: &mut V, keyword: &'a Keyword)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(keyword);

    if visitor.enter_node(node).is_traverse() {
        keyword.visit_source_order(visitor);
    }
    visitor.leave_node(node);
}

pub fn walk_with_item<'a, V>(visitor: &mut V, with_item: &'a WithItem)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(with_item);
    if visitor.enter_node(node).is_traverse() {
        with_item.visit_source_order(visitor);
    }
    visitor.leave_node(node);
}

pub fn walk_type_params<'a, V>(visitor: &mut V, type_params: &'a TypeParams)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(type_params);
    if visitor.enter_node(node).is_traverse() {
        type_params.visit_source_order(visitor);
    }
    visitor.leave_node(node);
}

pub fn walk_type_param<'a, V>(visitor: &mut V, type_param: &'a TypeParam)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(type_param);
    if visitor.enter_node(node).is_traverse() {
        match type_param {
            TypeParam::TypeVar(type_param) => type_param.visit_source_order(visitor),
            TypeParam::TypeVarTuple(type_param) => type_param.visit_source_order(visitor),
            TypeParam::ParamSpec(type_param) => type_param.visit_source_order(visitor),
        }
    }
    visitor.leave_node(node);
}

pub fn walk_match_case<'a, V>(visitor: &mut V, match_case: &'a MatchCase)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(match_case);
    if visitor.enter_node(node).is_traverse() {
        match_case.visit_source_order(visitor);
    }
    visitor.leave_node(node);
}

pub fn walk_pattern<'a, V>(visitor: &mut V, pattern: &'a Pattern)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(pattern);
    if visitor.enter_node(node).is_traverse() {
        match pattern {
            Pattern::MatchValue(pattern) => pattern.visit_source_order(visitor),
            Pattern::MatchSingleton(pattern) => pattern.visit_source_order(visitor),
            Pattern::MatchSequence(pattern) => pattern.visit_source_order(visitor),
            Pattern::MatchMapping(pattern) => pattern.visit_source_order(visitor),
            Pattern::MatchClass(pattern) => pattern.visit_source_order(visitor),
            Pattern::MatchStar(pattern) => pattern.visit_source_order(visitor),
            Pattern::MatchAs(pattern) => pattern.visit_source_order(visitor),
            Pattern::MatchOr(pattern) => pattern.visit_source_order(visitor),
        }
    }
    visitor.leave_node(node);
}

pub fn walk_pattern_arguments<'a, V>(visitor: &mut V, pattern_arguments: &'a PatternArguments)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(pattern_arguments);
    if visitor.enter_node(node).is_traverse() {
        for pattern in &pattern_arguments.patterns {
            visitor.visit_pattern(pattern);
        }
        for keyword in &pattern_arguments.keywords {
            visitor.visit_pattern_keyword(keyword);
        }
    }
    visitor.leave_node(node);
}

pub fn walk_pattern_keyword<'a, V>(visitor: &mut V, pattern_keyword: &'a PatternKeyword)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(pattern_keyword);
    if visitor.enter_node(node).is_traverse() {
        visitor.visit_pattern(&pattern_keyword.pattern);
    }
    visitor.leave_node(node);
}

pub fn walk_f_string_element<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    f_string_element: &'a FStringElement,
) {
    let node = AnyNodeRef::from(f_string_element);
    if visitor.enter_node(node).is_traverse() {
        match f_string_element {
            FStringElement::Expression(element) => element.visit_source_order(visitor),
            FStringElement::Literal(element) => element.visit_source_order(visitor),
        }
    }
    visitor.leave_node(node);
}

pub fn walk_bool_op<'a, V>(_visitor: &mut V, _bool_op: &'a BoolOp)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
}

#[inline]
pub fn walk_operator<'a, V>(_visitor: &mut V, _operator: &'a Operator)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
}

#[inline]
pub fn walk_unary_op<'a, V>(_visitor: &mut V, _unary_op: &'a UnaryOp)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
}

#[inline]
pub fn walk_cmp_op<'a, V>(_visitor: &mut V, _cmp_op: &'a CmpOp)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
}

#[inline]
pub fn walk_f_string<'a, V>(visitor: &mut V, f_string: &'a FString)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(f_string);
    if visitor.enter_node(node).is_traverse() {
        f_string.visit_source_order(visitor);
    }
    visitor.leave_node(node);
}

#[inline]
pub fn walk_string_literal<'a, V>(visitor: &mut V, string_literal: &'a StringLiteral)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(string_literal);
    if visitor.enter_node(node).is_traverse() {
        string_literal.visit_source_order(visitor);
    }
    visitor.leave_node(node);
}

#[inline]
pub fn walk_bytes_literal<'a, V>(visitor: &mut V, bytes_literal: &'a BytesLiteral)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(bytes_literal);
    if visitor.enter_node(node).is_traverse() {
        bytes_literal.visit_source_order(visitor);
    }
    visitor.leave_node(node);
}

#[inline]
pub fn walk_alias<'a, V>(visitor: &mut V, alias: &'a Alias)
where
    V: SourceOrderVisitor<'a> + ?Sized,
{
    let node = AnyNodeRef::from(alias);
    if visitor.enter_node(node).is_traverse() {
        alias.visit_source_order(visitor);
    }
    visitor.leave_node(node);
}
