use crate::Identifier;
use crate::flat::{Any, Handle, List, View};
pub use crate::visitor::source_order::TraversalSignal;
use crate::{
    Alias, Arguments, BoolOp, BytesLiteral, CmpOp, Comprehension, Decorator, ElifElseClause,
    ExceptHandler, Expr, FString, InterpolatedStringElement, Keyword, MatchCase, Mod, Operator,
    Parameter, ParameterWithDefault, Parameters, Pattern, PatternArguments, PatternKeyword,
    Singleton, Stmt, StringLiteral, TString, TypeParam, TypeParams, UnaryOp, WithItem,
};

/// Visitor that traverses all nodes recursively in the order they appear in the source.
///
/// If you need a visitor that visits the nodes in the order they're evaluated at runtime,
/// use [`Visitor`](super::Visitor) instead.
pub trait SourceOrderVisitor<'a> {
    #[inline]
    fn enter_node(&mut self, _node: View<'a, Any>) -> TraversalSignal {
        TraversalSignal::Traverse
    }

    #[inline(always)]
    fn leave_node(&mut self, _node: View<'a, Any>) {}

    #[inline]
    fn visit_mod(&mut self, module: View<'a, Mod>) {
        walk_module(self, module);
    }

    #[inline]
    fn visit_stmt(&mut self, stmt: View<'a, Stmt>) {
        walk_stmt(self, stmt);
    }

    #[inline]
    fn visit_annotation(&mut self, expr: View<'a, Expr>) {
        walk_annotation(self, expr);
    }

    #[inline]
    fn visit_expr(&mut self, expr: View<'a, Expr>) {
        walk_expr(self, expr);
    }

    #[inline]
    fn visit_decorator(&mut self, decorator: View<'a, Decorator>) {
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
    fn visit_comprehension(&mut self, comprehension: View<'a, Comprehension>) {
        walk_comprehension(self, comprehension);
    }

    #[inline]
    fn visit_except_handler(&mut self, except_handler: View<'a, ExceptHandler>) {
        walk_except_handler(self, except_handler);
    }

    #[inline]
    fn visit_arguments(&mut self, arguments: View<'a, Arguments>) {
        walk_arguments(self, arguments);
    }

    #[inline]
    fn visit_parameters(&mut self, parameters: View<'a, Parameters>) {
        walk_parameters(self, parameters);
    }

    #[inline]
    fn visit_parameter(&mut self, arg: View<'a, Parameter>) {
        walk_parameter(self, arg);
    }

    fn visit_parameter_with_default(
        &mut self,
        parameter_with_default: View<'a, ParameterWithDefault>,
    ) {
        walk_parameter_with_default(self, parameter_with_default);
    }

    #[inline]
    fn visit_keyword(&mut self, keyword: View<'a, Keyword>) {
        walk_keyword(self, keyword);
    }

    #[inline]
    fn visit_alias(&mut self, alias: View<'a, Alias>) {
        walk_alias(self, alias);
    }

    #[inline]
    fn visit_with_item(&mut self, with_item: View<'a, WithItem>) {
        walk_with_item(self, with_item);
    }

    #[inline]
    fn visit_type_params(&mut self, type_params: View<'a, TypeParams>) {
        walk_type_params(self, type_params);
    }

    #[inline]
    fn visit_type_param(&mut self, type_param: View<'a, TypeParam>) {
        walk_type_param(self, type_param);
    }

    #[inline]
    fn visit_match_case(&mut self, match_case: View<'a, MatchCase>) {
        walk_match_case(self, match_case);
    }

    #[inline]
    fn visit_pattern(&mut self, pattern: View<'a, Pattern>) {
        walk_pattern(self, pattern);
    }

    #[inline]
    fn visit_pattern_arguments(&mut self, pattern_arguments: View<'a, PatternArguments>) {
        walk_pattern_arguments(self, pattern_arguments);
    }

    #[inline]
    fn visit_pattern_keyword(&mut self, pattern_keyword: View<'a, PatternKeyword>) {
        walk_pattern_keyword(self, pattern_keyword);
    }

    #[inline]
    fn visit_body(&mut self, body: List<'a, Handle<Stmt>>) {
        walk_body(self, body);
    }

    #[inline]
    fn visit_elif_else_clause(&mut self, elif_else_clause: View<'a, ElifElseClause>) {
        walk_elif_else_clause(self, elif_else_clause);
    }

    #[inline]
    fn visit_f_string(&mut self, f_string: View<'a, FString>) {
        walk_f_string(self, f_string);
    }

    #[inline]
    fn visit_interpolated_string_element(
        &mut self,
        interpolated_string_element: View<'a, InterpolatedStringElement>,
    ) {
        walk_interpolated_string_element(self, interpolated_string_element);
    }

    #[inline]
    fn visit_t_string(&mut self, t_string: View<'a, TString>) {
        walk_t_string(self, t_string);
    }

    #[inline]
    fn visit_string_literal(&mut self, string_literal: View<'a, StringLiteral>) {
        walk_string_literal(self, string_literal);
    }

    #[inline]
    fn visit_bytes_literal(&mut self, bytes_literal: View<'a, BytesLiteral>) {
        walk_bytes_literal(self, bytes_literal);
    }

    #[inline]
    fn visit_identifier(&mut self, identifier: View<'a, Identifier>) {
        walk_identifier(self, identifier);
    }
}

pub fn walk_module<'a, V: SourceOrderVisitor<'a> + ?Sized>(visitor: &mut V, node: View<'a, Mod>) {
    walk_node(visitor, node.erase());
}
pub fn walk_stmt<'a, V: SourceOrderVisitor<'a> + ?Sized>(visitor: &mut V, node: View<'a, Stmt>) {
    walk_node(visitor, node.erase());
}
pub fn walk_annotation<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, Expr>,
) {
    visitor.visit_expr(node);
}
pub fn walk_expr<'a, V: SourceOrderVisitor<'a> + ?Sized>(visitor: &mut V, node: View<'a, Expr>) {
    walk_node(visitor, node.erase());
}
pub fn walk_decorator<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, Decorator>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_bool_op<'a, V: SourceOrderVisitor<'a> + ?Sized>(_: &mut V, _: BoolOp) {}
pub fn walk_operator<'a, V: SourceOrderVisitor<'a> + ?Sized>(_: &mut V, _: Operator) {}
pub fn walk_unary_op<'a, V: SourceOrderVisitor<'a> + ?Sized>(_: &mut V, _: UnaryOp) {}
pub fn walk_cmp_op<'a, V: SourceOrderVisitor<'a> + ?Sized>(_: &mut V, _: CmpOp) {}
pub fn walk_comprehension<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, Comprehension>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_except_handler<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, ExceptHandler>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_arguments<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, Arguments>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_parameters<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, Parameters>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_parameter<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, Parameter>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_parameter_with_default<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, ParameterWithDefault>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_keyword<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, Keyword>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_alias<'a, V: SourceOrderVisitor<'a> + ?Sized>(visitor: &mut V, node: View<'a, Alias>) {
    walk_node(visitor, node.erase());
}
pub fn walk_with_item<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, WithItem>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_type_params<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, TypeParams>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_type_param<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, TypeParam>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_match_case<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, MatchCase>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_pattern<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, Pattern>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_pattern_arguments<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, PatternArguments>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_pattern_keyword<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, PatternKeyword>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_elif_else_clause<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, ElifElseClause>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_f_string<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, FString>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_interpolated_string_element<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, InterpolatedStringElement>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_t_string<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, TString>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_string_literal<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, StringLiteral>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_bytes_literal<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, BytesLiteral>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_identifier<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    node: View<'a, Identifier>,
) {
    walk_node(visitor, node.erase());
}
pub fn walk_body<'a, V: SourceOrderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    body: List<'a, Handle<Stmt>>,
) {
    for stmt in body {
        visitor.visit_stmt(stmt);
    }
}

pub fn walk_node<'a, V: SourceOrderVisitor<'a> + ?Sized>(visitor: &mut V, node: View<'a, Any>) {
    if visitor.enter_node(node).is_traverse() {
        node.visit_source_order(visitor);
    }
    visitor.leave_node(node);
}

impl View<'_, Any> {
    pub fn visit_children(self, mut callback: impl FnMut(Self)) {
        struct Children<F>(F);
        impl<'a, F: FnMut(View<'a, Any>)> SourceOrderVisitor<'a> for Children<F> {
            #[inline]
            fn enter_node(&mut self, node: View<'a, Any>) -> TraversalSignal {
                self.0(node);
                TraversalSignal::Skip
            }
        }
        self.visit_source_order(&mut Children(&mut callback));
    }
}
