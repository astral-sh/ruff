use crate::Identifier;
use crate::flat::{AstNode, AstNodeId, List};
use crate::{
    Alias, Arguments, BoolOp, BytesLiteral, CmpOp, Comprehension, Decorator, DictItem,
    ElifElseClause, ExceptHandler, Expr, FString, InterpolatedStringElement, Keyword, MatchCase,
    Mod, Operator, Parameter, ParameterWithDefault, Parameters, Pattern, PatternArguments,
    PatternKeyword, Singleton, Stmt, StringLiteral, TString, TypeParam, TypeParams, UnaryOp,
    WithItem,
};

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

/// Visitor that traverses all nodes recursively in the order they appear in the source.
///
/// If you need a visitor that visits the nodes in the order they're evaluated at runtime,
/// use [`Visitor`](super::Visitor) instead.
pub trait SourceOrderVisitor<'a> {
    #[inline]
    fn enter_node(&mut self, _node: crate::SyntaxNode<'a>) -> TraversalSignal {
        TraversalSignal::Traverse
    }

    #[inline]
    fn leave_node(&mut self, _node: crate::SyntaxNode<'a>) {}

    #[inline]
    fn visit_mod(&mut self, module: Mod<'a>) {
        walk_module(self, module);
    }

    #[inline]
    fn visit_stmt(&mut self, stmt: Stmt<'a>) {
        walk_stmt(self, stmt);
    }

    #[inline]
    fn visit_annotation(&mut self, expr: Expr<'a>) {
        walk_annotation(self, expr);
    }

    #[inline]
    fn visit_expr(&mut self, expr: Expr<'a>) {
        walk_expr(self, expr);
    }

    #[inline]
    fn visit_decorator(&mut self, decorator: Decorator<'a>) {
        walk_decorator(self, decorator);
    }

    #[inline]
    fn visit_singleton(&mut self, _singleton: Singleton) {}

    #[inline]
    fn visit_bool_op(&mut self, bool_op: BoolOp) {
        walk_bool_op(self, bool_op);
    }

    #[inline]
    fn visit_operator(&mut self, operator: Operator) {
        walk_operator(self, operator);
    }

    #[inline]
    fn visit_unary_op(&mut self, unary_op: UnaryOp) {
        walk_unary_op(self, unary_op);
    }

    #[inline]
    fn visit_cmp_op(&mut self, cmp_op: CmpOp) {
        walk_cmp_op(self, cmp_op);
    }

    #[inline]
    fn visit_comprehension(&mut self, comprehension: Comprehension<'a>) {
        walk_comprehension(self, comprehension);
    }

    #[inline]
    fn visit_except_handler(&mut self, except_handler: ExceptHandler<'a>) {
        walk_except_handler(self, except_handler);
    }

    #[inline]
    fn visit_arguments(&mut self, arguments: Arguments<'a>) {
        walk_arguments(self, arguments);
    }

    #[inline]
    fn visit_parameters(&mut self, parameters: Parameters<'a>) {
        walk_parameters(self, parameters);
    }

    #[inline]
    fn visit_parameter(&mut self, arg: Parameter<'a>) {
        walk_parameter(self, arg);
    }

    fn visit_parameter_with_default(&mut self, parameter_with_default: ParameterWithDefault<'a>) {
        walk_parameter_with_default(self, parameter_with_default);
    }

    #[inline]
    fn visit_dict_item(&mut self, item: DictItem<'a>) {
        walk_dict_item(self, item);
    }
    #[inline]
    fn visit_keyword(&mut self, keyword: Keyword<'a>) {
        walk_keyword(self, keyword);
    }

    #[inline]
    fn visit_alias(&mut self, alias: Alias<'a>) {
        walk_alias(self, alias);
    }

    #[inline]
    fn visit_with_item(&mut self, with_item: WithItem<'a>) {
        walk_with_item(self, with_item);
    }

    #[inline]
    fn visit_type_params(&mut self, type_params: TypeParams<'a>) {
        walk_type_params(self, type_params);
    }

    #[inline]
    fn visit_type_param(&mut self, type_param: TypeParam<'a>) {
        walk_type_param(self, type_param);
    }

    #[inline]
    fn visit_match_case(&mut self, match_case: MatchCase<'a>) {
        walk_match_case(self, match_case);
    }

    #[inline]
    fn visit_pattern(&mut self, pattern: Pattern<'a>) {
        walk_pattern(self, pattern);
    }

    #[inline]
    fn visit_pattern_arguments(&mut self, pattern_arguments: PatternArguments<'a>) {
        walk_pattern_arguments(self, pattern_arguments);
    }

    #[inline]
    fn visit_pattern_keyword(&mut self, pattern_keyword: PatternKeyword<'a>) {
        walk_pattern_keyword(self, pattern_keyword);
    }

    #[inline]
    fn visit_body(&mut self, body: List<'a, AstNodeId<crate::kind::Stmt>>) {
        walk_body(self, body);
    }

    #[inline]
    fn visit_elif_else_clause(&mut self, elif_else_clause: ElifElseClause<'a>) {
        walk_elif_else_clause(self, elif_else_clause);
    }

    #[inline]
    fn visit_f_string(&mut self, f_string: FString<'a>) {
        walk_f_string(self, f_string);
    }

    #[inline]
    fn visit_interpolated_string_element(
        &mut self,
        interpolated_string_element: InterpolatedStringElement<'a>,
    ) {
        walk_interpolated_string_element(self, interpolated_string_element);
    }

    #[inline]
    fn visit_t_string(&mut self, t_string: TString<'a>) {
        walk_t_string(self, t_string);
    }

    #[inline]
    fn visit_string_literal(&mut self, string_literal: StringLiteral<'a>) {
        walk_string_literal(self, string_literal);
    }

    #[inline]
    fn visit_bytes_literal(&mut self, bytes_literal: BytesLiteral<'a>) {
        walk_bytes_literal(self, bytes_literal);
    }

    #[inline]
    fn visit_identifier(&mut self, identifier: Identifier<'a>) {
        walk_identifier(self, identifier);
    }
}

pub fn walk_module<'a, V: SourceOrderVisitor<'a> + ?Sized>(visitor: &mut V, node: Mod<'a>) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_stmt<'a, V: SourceOrderVisitor<'a> + ?Sized>(visitor: &mut V, node: Stmt<'a>) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_annotation<'a, V: SourceOrderVisitor<'a> + ?Sized>(visitor: &mut V, node: Expr<'a>) {
    visitor.visit_expr(node);
}
pub fn walk_expr<'a, V: SourceOrderVisitor<'a> + ?Sized>(visitor: &mut V, node: Expr<'a>) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_decorator<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: Decorator<'a>,
) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_bool_op<'a, V: SourceOrderVisitor<'a> + ?Sized>(_: &mut V, _: BoolOp) {}
pub fn walk_operator<'a, V: SourceOrderVisitor<'a> + ?Sized>(_: &mut V, _: Operator) {}
pub fn walk_unary_op<'a, V: SourceOrderVisitor<'a> + ?Sized>(_: &mut V, _: UnaryOp) {}
pub fn walk_cmp_op<'a, V: SourceOrderVisitor<'a> + ?Sized>(_: &mut V, _: CmpOp) {}
pub fn walk_comprehension<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: Comprehension<'a>,
) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_except_handler<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: ExceptHandler<'a>,
) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_arguments<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: Arguments<'a>,
) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_parameters<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: Parameters<'a>,
) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_parameter<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: Parameter<'a>,
) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_parameter_with_default<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: ParameterWithDefault<'a>,
) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_dict_item<'a, V: SourceOrderVisitor<'a> + ?Sized>(visitor: &mut V, node: DictItem<'a>) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_keyword<'a, V: SourceOrderVisitor<'a> + ?Sized>(visitor: &mut V, node: Keyword<'a>) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_alias<'a, V: SourceOrderVisitor<'a> + ?Sized>(visitor: &mut V, node: Alias<'a>) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_with_item<'a, V: SourceOrderVisitor<'a> + ?Sized>(visitor: &mut V, node: WithItem<'a>) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_type_params<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: TypeParams<'a>,
) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_type_param<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: TypeParam<'a>,
) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_match_case<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: MatchCase<'a>,
) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_pattern<'a, V: SourceOrderVisitor<'a> + ?Sized>(visitor: &mut V, node: Pattern<'a>) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_pattern_arguments<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: PatternArguments<'a>,
) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_pattern_keyword<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: PatternKeyword<'a>,
) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_elif_else_clause<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: ElifElseClause<'a>,
) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_f_string<'a, V: SourceOrderVisitor<'a> + ?Sized>(visitor: &mut V, node: FString<'a>) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_interpolated_string_element<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: InterpolatedStringElement<'a>,
) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_t_string<'a, V: SourceOrderVisitor<'a> + ?Sized>(visitor: &mut V, node: TString<'a>) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_string_literal<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: StringLiteral<'a>,
) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_bytes_literal<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: BytesLiteral<'a>,
) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_identifier<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: Identifier<'a>,
) {
    walk_node(visitor, node.syntax_node());
}
pub fn walk_body<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    body: List<'a, AstNodeId<crate::kind::Stmt>>,
) {
    for stmt in body {
        visitor.visit_stmt(stmt);
    }
}

pub fn walk_node<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: crate::SyntaxNode<'a>,
) {
    if visitor.enter_node(node).is_traverse() {
        node.visit_source_order(visitor);
    }
    visitor.leave_node(node);
}
