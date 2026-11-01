//! AST visitor trait and walk functions.

pub mod source_order;

use itertools::Either;

use super::{AstNode, AstNodeId, FStringPartView, List};
use crate::flat;

use crate::{
    Alias, Arguments, BoolOp, BytesLiteral, CmpOp, Comprehension, Decorator, DictItem,
    ElifElseClause, ExceptHandler, Expr, ExprContext, FString, InterpolatedStringElement, Keyword,
    MatchCase, Operator, Parameter, Parameters, Pattern, PatternArguments, PatternKeyword, Stmt,
    StringLiteral, TString, TypeParam, TypeParams, UnaryOp, WithItem,
};

/// A trait for AST visitors. Visits all nodes in the AST recursively in evaluation-order.
///
/// Use [`SourceOrderVisitor`](source_order::SourceOrderVisitor) to visit nodes in source order.
///
/// Use [`Builder::rewrite_node`](crate::flat::Builder::rewrite_node) to build a modified subtree.
pub trait Visitor<'a> {
    fn visit_stmt(&mut self, stmt: Stmt<'a>) {
        walk_stmt(self, stmt);
    }
    fn visit_annotation(&mut self, expr: Expr<'a>) {
        walk_annotation(self, expr);
    }
    fn visit_decorator(&mut self, decorator: Decorator<'a>) {
        walk_decorator(self, decorator);
    }
    fn visit_expr(&mut self, expr: Expr<'a>) {
        walk_expr(self, expr);
    }
    fn visit_expr_context(&mut self, expr_context: ExprContext) {
        walk_expr_context(self, expr_context);
    }
    fn visit_bool_op(&mut self, bool_op: BoolOp) {
        walk_bool_op(self, bool_op);
    }
    fn visit_operator(&mut self, operator: Operator) {
        walk_operator(self, operator);
    }
    fn visit_unary_op(&mut self, unary_op: UnaryOp) {
        walk_unary_op(self, unary_op);
    }
    fn visit_cmp_op(&mut self, cmp_op: CmpOp) {
        walk_cmp_op(self, cmp_op);
    }
    fn visit_comprehension(&mut self, comprehension: Comprehension<'a>) {
        walk_comprehension(self, comprehension);
    }
    fn visit_except_handler(&mut self, except_handler: ExceptHandler<'a>) {
        walk_except_handler(self, except_handler);
    }
    fn visit_arguments(&mut self, arguments: Arguments<'a>) {
        walk_arguments(self, arguments);
    }
    fn visit_parameters(&mut self, parameters: Parameters<'a>) {
        walk_parameters(self, parameters);
    }
    fn visit_parameter(&mut self, parameter: Parameter<'a>) {
        walk_parameter(self, parameter);
    }
    fn visit_dict_item(&mut self, item: DictItem<'a>) {
        walk_dict_item(self, item);
    }
    fn visit_keyword(&mut self, keyword: Keyword<'a>) {
        walk_keyword(self, keyword);
    }
    fn visit_alias(&mut self, alias: Alias<'a>) {
        walk_alias(self, alias);
    }
    fn visit_with_item(&mut self, with_item: WithItem<'a>) {
        walk_with_item(self, with_item);
    }
    fn visit_type_params(&mut self, type_params: TypeParams<'a>) {
        walk_type_params(self, type_params);
    }
    fn visit_type_param(&mut self, type_param: TypeParam<'a>) {
        walk_type_param(self, type_param);
    }
    fn visit_match_case(&mut self, match_case: MatchCase<'a>) {
        walk_match_case(self, match_case);
    }
    fn visit_pattern(&mut self, pattern: Pattern<'a>) {
        walk_pattern(self, pattern);
    }
    fn visit_pattern_arguments(&mut self, pattern_arguments: PatternArguments<'a>) {
        walk_pattern_arguments(self, pattern_arguments);
    }
    fn visit_pattern_keyword(&mut self, pattern_keyword: PatternKeyword<'a>) {
        walk_pattern_keyword(self, pattern_keyword);
    }
    fn visit_body(&mut self, body: List<'a, AstNodeId<crate::kind::Stmt>>) {
        walk_body(self, body);
    }
    fn visit_elif_else_clause(&mut self, elif_else_clause: ElifElseClause<'a>) {
        walk_elif_else_clause(self, elif_else_clause);
    }
    fn visit_f_string(&mut self, f_string: FString<'a>) {
        walk_f_string(self, f_string);
    }
    fn visit_interpolated_string_element(
        &mut self,
        interpolated_string_element: InterpolatedStringElement<'a>,
    ) {
        walk_interpolated_string_element(self, interpolated_string_element);
    }
    fn visit_t_string(&mut self, t_string: TString<'a>) {
        walk_t_string(self, t_string);
    }
    fn visit_string_literal(&mut self, string_literal: StringLiteral<'a>) {
        walk_string_literal(self, string_literal);
    }
    fn visit_bytes_literal(&mut self, bytes_literal: BytesLiteral<'a>) {
        walk_bytes_literal(self, bytes_literal);
    }
}

pub fn walk_body<'a, V: Visitor<'a> + ?Sized>(
    visitor: &mut V,
    body: List<'a, AstNodeId<crate::kind::Stmt>>,
) {
    for stmt in body {
        visitor.visit_stmt(stmt);
    }
}

pub fn walk_elif_else_clause<'a, V: Visitor<'a> + ?Sized>(
    visitor: &mut V,
    elif_else_clause: ElifElseClause<'a>,
) {
    if let Some(test) = elif_else_clause.test() {
        visitor.visit_expr(test);
    }
    visitor.visit_body(elif_else_clause.body());
}

pub fn walk_stmt<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, stmt: Stmt<'a>) {
    match stmt.classify() {
        flat::StmtKind::FunctionDef(stmt_function_def) => {
            let parameters = stmt_function_def.parameters();
            let body = stmt_function_def.body();
            let decorator_list = stmt_function_def.decorator_list();
            let returns = stmt_function_def.returns();
            let type_params = stmt_function_def.type_params();

            for decorator in decorator_list {
                visitor.visit_decorator(decorator);
            }
            if let Some(type_params) = type_params {
                visitor.visit_type_params(type_params);
            }
            visitor.visit_parameters(parameters);
            if let Some(expr) = returns {
                visitor.visit_annotation(expr);
            }
            visitor.visit_body(body);
        }
        flat::StmtKind::ClassDef(stmt_class_def) => {
            let arguments = stmt_class_def.arguments();
            let body = stmt_class_def.body();
            let decorator_list = stmt_class_def.decorator_list();
            let type_params = stmt_class_def.type_params();

            for decorator in decorator_list {
                visitor.visit_decorator(decorator);
            }
            if let Some(type_params) = type_params {
                visitor.visit_type_params(type_params);
            }
            if let Some(arguments) = arguments {
                visitor.visit_arguments(arguments);
            }
            visitor.visit_body(body);
        }
        flat::StmtKind::Return(stmt_return) => {
            let value = stmt_return.value();

            if let Some(expr) = value {
                visitor.visit_expr(expr);
            }
        }
        flat::StmtKind::Delete(stmt_delete) => {
            let targets = stmt_delete.targets();

            for expr in targets {
                visitor.visit_expr(expr);
            }
        }
        flat::StmtKind::TypeAlias(stmt_type_alias) => {
            let name = stmt_type_alias.name();
            let type_params = stmt_type_alias.type_params();
            let value = stmt_type_alias.value();

            visitor.visit_expr(value);
            if let Some(type_params) = type_params {
                visitor.visit_type_params(type_params);
            }
            visitor.visit_expr(name);
        }
        flat::StmtKind::Assign(stmt_assign) => {
            let targets = stmt_assign.targets();
            let value = stmt_assign.value();

            visitor.visit_expr(value);
            for expr in targets {
                visitor.visit_expr(expr);
            }
        }
        flat::StmtKind::AugAssign(stmt_aug_assign) => {
            let target = stmt_aug_assign.target();
            let op = stmt_aug_assign.op();
            let value = stmt_aug_assign.value();

            visitor.visit_expr(value);
            visitor.visit_operator(op);
            visitor.visit_expr(target);
        }
        flat::StmtKind::AnnAssign(stmt_ann_assign) => {
            let target = stmt_ann_assign.target();
            let annotation = stmt_ann_assign.annotation();
            let value = stmt_ann_assign.value();

            if let Some(expr) = value {
                visitor.visit_expr(expr);
            }
            visitor.visit_annotation(annotation);
            visitor.visit_expr(target);
        }
        flat::StmtKind::For(stmt_for) => {
            let target = stmt_for.target();
            let iter = stmt_for.iter();
            let body = stmt_for.body();
            let orelse = stmt_for.orelse();

            visitor.visit_expr(iter);
            visitor.visit_expr(target);
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }
        flat::StmtKind::While(stmt_while) => {
            let test = stmt_while.test();
            let body = stmt_while.body();
            let orelse = stmt_while.orelse();

            visitor.visit_expr(test);
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }
        flat::StmtKind::If(stmt_if) => {
            let test = stmt_if.test();
            let body = stmt_if.body();
            let elif_else_clauses = stmt_if.elif_else_clauses();

            visitor.visit_expr(test);
            visitor.visit_body(body);
            for clause in elif_else_clauses {
                visitor.visit_elif_else_clause(clause);
            }
        }
        flat::StmtKind::With(stmt_with) => {
            let items = stmt_with.items();
            let body = stmt_with.body();

            for with_item in items {
                visitor.visit_with_item(with_item);
            }
            visitor.visit_body(body);
        }
        flat::StmtKind::Match(stmt_match) => {
            let subject = stmt_match.subject();
            let cases = stmt_match.cases();

            visitor.visit_expr(subject);
            for match_case in cases {
                visitor.visit_match_case(match_case);
            }
        }
        flat::StmtKind::Raise(stmt_raise) => {
            let exc = stmt_raise.exc();
            let cause = stmt_raise.cause();

            if let Some(expr) = exc {
                visitor.visit_expr(expr);
            }
            if let Some(expr) = cause {
                visitor.visit_expr(expr);
            }
        }
        flat::StmtKind::Try(stmt_try) => {
            let body = stmt_try.body();
            let handlers = stmt_try.handlers();
            let orelse = stmt_try.orelse();
            let finalbody = stmt_try.finalbody();

            visitor.visit_body(body);
            for except_handler in handlers {
                visitor.visit_except_handler(except_handler);
            }
            visitor.visit_body(orelse);
            visitor.visit_body(finalbody);
        }
        flat::StmtKind::Assert(stmt_assert) => {
            let test = stmt_assert.test();
            let msg = stmt_assert.msg();

            visitor.visit_expr(test);
            if let Some(expr) = msg {
                visitor.visit_expr(expr);
            }
        }
        flat::StmtKind::Import(stmt_import) => {
            let names = stmt_import.names();

            for alias in names {
                visitor.visit_alias(alias);
            }
        }
        flat::StmtKind::ImportFrom(stmt_import_from) => {
            let names = stmt_import_from.names();

            for alias in names {
                visitor.visit_alias(alias);
            }
        }
        flat::StmtKind::Global(_) => {}
        flat::StmtKind::Nonlocal(_) => {}
        flat::StmtKind::Expr(stmt_expr) => {
            let value = stmt_expr.value();
            visitor.visit_expr(value);
        }
        flat::StmtKind::Pass(_)
        | flat::StmtKind::Break(_)
        | flat::StmtKind::Continue(_)
        | flat::StmtKind::IpyEscapeCommand(_) => {}
    }
}

pub fn walk_annotation<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, expr: Expr<'a>) {
    visitor.visit_expr(expr);
}

pub fn walk_decorator<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, decorator: Decorator<'a>) {
    visitor.visit_expr(decorator.expression());
}

pub fn walk_expr<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, expr: Expr<'a>) {
    match expr.classify() {
        flat::ExprKind::BoolOp(bool_op) => {
            visitor.visit_bool_op(bool_op.op());
            for value in bool_op.values() {
                visitor.visit_expr(value);
            }
        }
        flat::ExprKind::Named(named) => {
            visitor.visit_expr(named.value());
            visitor.visit_expr(named.target());
        }
        flat::ExprKind::BinOp(bin_op) => {
            visitor.visit_expr(bin_op.left());
            visitor.visit_operator(bin_op.op());
            visitor.visit_expr(bin_op.right());
        }
        flat::ExprKind::UnaryOp(unary_op) => {
            visitor.visit_unary_op(unary_op.op());
            visitor.visit_expr(unary_op.operand());
        }
        flat::ExprKind::Lambda(lambda) => {
            if let Some(parameters) = lambda.parameters() {
                visitor.visit_parameters(parameters);
            }
            visitor.visit_expr(lambda.body());
        }
        flat::ExprKind::If(if_expr) => {
            visitor.visit_expr(if_expr.test());
            visitor.visit_expr(if_expr.body());
            visitor.visit_expr(if_expr.orelse());
        }
        flat::ExprKind::Dict(dict) => {
            for item in dict.items() {
                visitor.visit_dict_item(item);
            }
        }
        flat::ExprKind::Set(set) => {
            for element in set.elts() {
                visitor.visit_expr(element);
            }
        }
        flat::ExprKind::ListComp(comp) => {
            for comprehension in comp.generators() {
                visitor.visit_comprehension(comprehension);
            }
            visitor.visit_expr(comp.elt());
        }
        flat::ExprKind::SetComp(comp) => {
            for comprehension in comp.generators() {
                visitor.visit_comprehension(comprehension);
            }
            visitor.visit_expr(comp.elt());
        }
        flat::ExprKind::DictComp(comp) => {
            for comprehension in comp.generators() {
                visitor.visit_comprehension(comprehension);
            }
            if let Some(key) = comp.key() {
                visitor.visit_expr(key);
            }
            visitor.visit_expr(comp.value());
        }
        flat::ExprKind::Generator(generator) => {
            for comprehension in generator.generators() {
                visitor.visit_comprehension(comprehension);
            }
            visitor.visit_expr(generator.elt());
        }
        flat::ExprKind::Await(await_expr) => visitor.visit_expr(await_expr.value()),
        flat::ExprKind::Yield(yield_expr) => {
            if let Some(value) = yield_expr.value() {
                visitor.visit_expr(value);
            }
        }
        flat::ExprKind::YieldFrom(yield_from) => visitor.visit_expr(yield_from.value()),
        flat::ExprKind::Compare(compare) => {
            let mut operands = compare.operands().iter();
            if let Some(left) = operands.next() {
                visitor.visit_expr(left);
            }
            for op in compare.ops() {
                visitor.visit_cmp_op(op);
            }
            for right in operands {
                visitor.visit_expr(right);
            }
        }
        flat::ExprKind::Call(call) => {
            visitor.visit_expr(call.func());
            visitor.visit_arguments(call.arguments());
        }
        flat::ExprKind::FString(f_string) => {
            for part in f_string.value() {
                match part {
                    FStringPartView::Literal(literal) => visitor.visit_string_literal(literal),
                    FStringPartView::FString(f_string) => visitor.visit_f_string(f_string),
                }
            }
        }
        flat::ExprKind::TString(t_string) => {
            for part in t_string.value() {
                visitor.visit_t_string(part);
            }
        }
        flat::ExprKind::StringLiteral(string) => {
            for part in string.value() {
                visitor.visit_string_literal(part);
            }
        }
        flat::ExprKind::BytesLiteral(bytes) => {
            for part in bytes.value() {
                visitor.visit_bytes_literal(part);
            }
        }
        flat::ExprKind::NumberLiteral(_)
        | flat::ExprKind::BooleanLiteral(_)
        | flat::ExprKind::NoneLiteral(_)
        | flat::ExprKind::EllipsisLiteral(_)
        | flat::ExprKind::IpyEscapeCommand(_) => {}
        flat::ExprKind::Attribute(attribute) => {
            visitor.visit_expr(attribute.value());
            visitor.visit_expr_context(attribute.ctx());
        }
        flat::ExprKind::Subscript(subscript) => {
            visitor.visit_expr(subscript.value());
            visitor.visit_expr(subscript.slice());
            visitor.visit_expr_context(subscript.ctx());
        }
        flat::ExprKind::Starred(starred) => {
            visitor.visit_expr(starred.value());
            visitor.visit_expr_context(starred.ctx());
        }
        flat::ExprKind::Name(name) => visitor.visit_expr_context(name.ctx()),
        flat::ExprKind::List(list) => {
            for element in list.elts() {
                visitor.visit_expr(element);
            }
            visitor.visit_expr_context(list.ctx());
        }
        flat::ExprKind::Tuple(tuple) => {
            for element in tuple.elts() {
                visitor.visit_expr(element);
            }
            visitor.visit_expr_context(tuple.ctx());
        }
        flat::ExprKind::Slice(slice) => {
            if let Some(lower) = slice.lower() {
                visitor.visit_expr(lower);
            }
            if let Some(upper) = slice.upper() {
                visitor.visit_expr(upper);
            }
            if let Some(step) = slice.step() {
                visitor.visit_expr(step);
            }
        }
    }
}

pub fn walk_comprehension<'a, V: Visitor<'a> + ?Sized>(
    visitor: &mut V,
    comprehension: Comprehension<'a>,
) {
    visitor.visit_expr(comprehension.iter());
    visitor.visit_expr(comprehension.target());
    for expr in comprehension.ifs() {
        visitor.visit_expr(expr);
    }
}

pub fn walk_except_handler<'a, V: Visitor<'a> + ?Sized>(
    visitor: &mut V,
    except_handler: ExceptHandler<'a>,
) {
    match except_handler.classify() {
        flat::ExceptHandlerKind::ExceptHandler(except_handler_except_handler) => {
            let type_ = except_handler_except_handler.type_();
            let body = except_handler_except_handler.body();

            if let Some(expr) = type_ {
                visitor.visit_expr(expr);
            }
            visitor.visit_body(body);
        }
    }
}

pub fn walk_arguments<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, arguments: Arguments<'a>) {
    // Note that there might be keywords before the last arg, e.g. in
    // f(*args, a=2, *args2, **kwargs)`, but we follow Python in evaluating first `args` and then
    // `keywords`. See also [Arguments::arguments_source_order`].
    for arg in arguments.args() {
        visitor.visit_expr(arg);
    }
    for keyword in arguments.keywords() {
        visitor.visit_keyword(keyword);
    }
}

pub fn walk_parameters<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, parameters: Parameters<'a>) {
    // Defaults are evaluated before annotations.
    for default in parameters
        .iter_non_variadic_params()
        .filter_map(crate::ParameterWithDefault::default)
    {
        visitor.visit_expr(default);
    }

    for parameter in parameters.all_parameters() {
        visitor.visit_parameter(parameter);
    }
}

pub fn walk_parameter<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, parameter: Parameter<'a>) {
    if let Some(expr) = parameter.annotation() {
        visitor.visit_annotation(expr);
    }
}

pub fn walk_dict_item<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, item: DictItem<'a>) {
    if let Some(key) = item.key() {
        visitor.visit_expr(key);
    }
    visitor.visit_expr(item.value());
}

pub fn walk_keyword<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, keyword: Keyword<'a>) {
    visitor.visit_expr(keyword.value());
}

pub fn walk_with_item<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, with_item: WithItem<'a>) {
    visitor.visit_expr(with_item.context_expr());
    if let Some(expr) = with_item.optional_vars() {
        visitor.visit_expr(expr);
    }
}

pub fn walk_type_params<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, type_params: TypeParams<'a>) {
    for type_param in type_params.type_params() {
        visitor.visit_type_param(type_param);
    }
}

pub fn walk_type_param<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, type_param: TypeParam<'a>) {
    match type_param.classify() {
        flat::TypeParamKind::TypeVar(type_param_type_var) => {
            let bound = type_param_type_var.bound();
            let default = type_param_type_var.default();

            if let Some(expr) = bound {
                visitor.visit_expr(expr);
            }
            if let Some(expr) = default {
                visitor.visit_expr(expr);
            }
        }
        flat::TypeParamKind::TypeVarTuple(type_param_type_var_tuple) => {
            let default = type_param_type_var_tuple.default();

            if let Some(expr) = default {
                visitor.visit_expr(expr);
            }
        }
        flat::TypeParamKind::ParamSpec(type_param_param_spec) => {
            let default = type_param_param_spec.default();

            if let Some(expr) = default {
                visitor.visit_expr(expr);
            }
        }
    }
}

pub fn walk_match_case<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, match_case: MatchCase<'a>) {
    visitor.visit_pattern(match_case.pattern());
    if let Some(expr) = match_case.guard() {
        visitor.visit_expr(expr);
    }
    visitor.visit_body(match_case.body());
}

pub fn walk_pattern<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, pattern: Pattern<'a>) {
    match pattern.classify() {
        flat::PatternKind::MatchValue(pattern_match_value) => {
            let value = pattern_match_value.value();

            visitor.visit_expr(value);
        }
        flat::PatternKind::MatchSingleton(_) => {}
        flat::PatternKind::MatchSequence(pattern_match_sequence) => {
            let patterns = pattern_match_sequence.patterns();

            for pattern in patterns {
                visitor.visit_pattern(pattern);
            }
        }
        flat::PatternKind::MatchMapping(pattern_match_mapping) => {
            let items = pattern_match_mapping.items();
            for (key, _) in items.entries() {
                visitor.visit_expr(key);
            }
            for (_, pattern) in items.entries() {
                visitor.visit_pattern(pattern);
            }
        }
        flat::PatternKind::MatchClass(pattern_match_class) => {
            let cls = pattern_match_class.cls();
            let arguments = pattern_match_class.arguments();

            visitor.visit_expr(cls);
            visitor.visit_pattern_arguments(arguments);
        }
        flat::PatternKind::MatchStar(_) => {}
        flat::PatternKind::MatchAs(pattern_match_as) => {
            let pattern = pattern_match_as.pattern();

            if let Some(pattern) = pattern {
                visitor.visit_pattern(pattern);
            }
        }
        flat::PatternKind::MatchOr(pattern_match_or) => {
            let patterns = pattern_match_or.patterns();

            for pattern in patterns {
                visitor.visit_pattern(pattern);
            }
        }
    }
}

pub fn walk_pattern_arguments<'a, V: Visitor<'a> + ?Sized>(
    visitor: &mut V,
    pattern_arguments: PatternArguments<'a>,
) {
    for pattern in pattern_arguments.patterns() {
        visitor.visit_pattern(pattern);
    }
    for keyword in pattern_arguments.keywords() {
        visitor.visit_pattern_keyword(keyword);
    }
}

pub fn walk_pattern_keyword<'a, V: Visitor<'a> + ?Sized>(
    visitor: &mut V,
    pattern_keyword: PatternKeyword<'a>,
) {
    visitor.visit_pattern(pattern_keyword.pattern());
}

pub fn walk_f_string<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, f_string: FString<'a>) {
    for interpolated_string_element in f_string.elements() {
        visitor.visit_interpolated_string_element(interpolated_string_element);
    }
}

pub fn walk_interpolated_string_element<'a, V: Visitor<'a> + ?Sized>(
    visitor: &mut V,
    interpolated_string_element: InterpolatedStringElement<'a>,
) {
    if let Some(interpolation) = interpolated_string_element.cast::<crate::InterpolatedElement>() {
        visitor.visit_expr(interpolation.expression());
        if let Some(format_spec) = interpolation.format_spec() {
            for spec_element in format_spec.elements() {
                visitor.visit_interpolated_string_element(spec_element);
            }
        }
    }
}

pub fn walk_t_string<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, t_string: TString<'a>) {
    for t_string_element in t_string.elements() {
        visitor.visit_interpolated_string_element(t_string_element);
    }
}

pub fn walk_expr_context<'a, V: Visitor<'a> + ?Sized>(_visitor: &V, _expr_context: ExprContext) {}

pub fn walk_bool_op<'a, V: Visitor<'a> + ?Sized>(_visitor: &V, _bool_op: BoolOp) {}

pub fn walk_operator<'a, V: Visitor<'a> + ?Sized>(_visitor: &V, _operator: Operator) {}

pub fn walk_unary_op<'a, V: Visitor<'a> + ?Sized>(_visitor: &V, _unary_op: UnaryOp) {}

pub fn walk_cmp_op<'a, V: Visitor<'a> + ?Sized>(_visitor: &V, _cmp_op: CmpOp) {}

pub fn walk_alias<'a, V: Visitor<'a> + ?Sized>(_visitor: &V, _alias: Alias<'a>) {}

pub fn walk_string_literal<'a, V: Visitor<'a> + ?Sized>(
    _visitor: &V,
    _string_literal: StringLiteral<'a>,
) {
}

pub fn walk_bytes_literal<'a, V: Visitor<'a> + ?Sized>(
    _visitor: &V,
    _bytes_literal: BytesLiteral<'a>,
) {
}

impl<'a> Parameters<'a> {
    pub fn iter_non_variadic_params(
        self,
    ) -> impl DoubleEndedIterator<Item = crate::ParameterWithDefault<'a>> {
        if self.has_only_regular_args() {
            return Either::Left(self.args().iter());
        }
        Either::Right(
            self.posonlyargs()
                .iter()
                .chain(self.args())
                .chain(self.kwonlyargs()),
        )
    }

    pub fn all_parameters(self) -> impl DoubleEndedIterator<Item = Parameter<'a>> {
        if self.has_only_regular_args() {
            return Either::Left(
                self.args()
                    .iter()
                    .map(super::super::generated::ParameterWithDefault::parameter),
            );
        }
        Either::Right(
            self.posonlyargs()
                .iter()
                .chain(self.args())
                .map(super::super::generated::ParameterWithDefault::parameter)
                .chain(self.vararg())
                .chain(
                    self.kwonlyargs()
                        .iter()
                        .map(super::super::generated::ParameterWithDefault::parameter),
                )
                .chain(self.kwarg()),
        )
    }
}
