use crate::{
    self as ast, Alias, Arguments, BoolOp, BytesLiteral, CmpOp, Comprehension, Decorator,
    ElifElseClause, ExceptHandler, Expr, ExprContext, FString, FStringElement, Keyword, MatchCase,
    Operator, Parameter, Parameters, Pattern, PatternArguments, PatternKeyword, Stmt,
    StringLiteral, TypeParam, TypeParamParamSpec, TypeParamTypeVar, TypeParamTypeVarTuple,
    TypeParams, UnaryOp, WithItem,
};

/// A trait for transforming ASTs. Visits all nodes in the AST recursively in evaluation-order.
pub trait Transformer {
    fn visit_stmt(&self, stmt: &mut Stmt) {
        walk_stmt(self, stmt);
    }
    fn visit_annotation(&self, expr: &mut Expr) {
        walk_annotation(self, expr);
    }
    fn visit_decorator(&self, decorator: &mut Decorator) {
        walk_decorator(self, decorator);
    }
    fn visit_expr(&self, expr: &mut Expr) {
        walk_expr(self, expr);
    }
    fn visit_expr_context(&self, expr_context: &mut ExprContext) {
        walk_expr_context(self, expr_context);
    }
    fn visit_bool_op(&self, bool_op: &mut BoolOp) {
        walk_bool_op(self, bool_op);
    }
    fn visit_operator(&self, operator: &mut Operator) {
        walk_operator(self, operator);
    }
    fn visit_unary_op(&self, unary_op: &mut UnaryOp) {
        walk_unary_op(self, unary_op);
    }
    fn visit_cmp_op(&self, cmp_op: &mut CmpOp) {
        walk_cmp_op(self, cmp_op);
    }
    fn visit_comprehension(&self, comprehension: &mut Comprehension) {
        walk_comprehension(self, comprehension);
    }
    fn visit_except_handler(&self, except_handler: &mut ExceptHandler) {
        walk_except_handler(self, except_handler);
    }
    fn visit_arguments(&self, arguments: &mut Arguments) {
        walk_arguments(self, arguments);
    }
    fn visit_parameters(&self, parameters: &mut Parameters) {
        walk_parameters(self, parameters);
    }
    fn visit_parameter(&self, parameter: &mut Parameter) {
        walk_parameter(self, parameter);
    }
    fn visit_keyword(&self, keyword: &mut Keyword) {
        walk_keyword(self, keyword);
    }
    fn visit_alias(&self, alias: &mut Alias) {
        walk_alias(self, alias);
    }
    fn visit_with_item(&self, with_item: &mut WithItem) {
        walk_with_item(self, with_item);
    }
    fn visit_type_params(&self, type_params: &mut TypeParams) {
        walk_type_params(self, type_params);
    }
    fn visit_type_param(&self, type_param: &mut TypeParam) {
        walk_type_param(self, type_param);
    }
    fn visit_match_case(&self, match_case: &mut MatchCase) {
        walk_match_case(self, match_case);
    }
    fn visit_pattern(&self, pattern: &mut Pattern) {
        walk_pattern(self, pattern);
    }
    fn visit_pattern_arguments(&self, pattern_arguments: &mut PatternArguments) {
        walk_pattern_arguments(self, pattern_arguments);
    }
    fn visit_pattern_keyword(&self, pattern_keyword: &mut PatternKeyword) {
        walk_pattern_keyword(self, pattern_keyword);
    }
    fn visit_body(&self, body: &mut [Stmt]) {
        walk_body(self, body);
    }
    fn visit_elif_else_clause(&self, elif_else_clause: &mut ElifElseClause) {
        walk_elif_else_clause(self, elif_else_clause);
    }
    fn visit_f_string(&self, f_string: &mut FString) {
        walk_f_string(self, f_string);
    }
    fn visit_f_string_element(&self, f_string_element: &mut FStringElement) {
        walk_f_string_element(self, f_string_element);
    }
    fn visit_string_literal(&self, string_literal: &mut StringLiteral) {
        walk_string_literal(self, string_literal);
    }
    fn visit_bytes_literal(&self, bytes_literal: &mut BytesLiteral) {
        walk_bytes_literal(self, bytes_literal);
    }
}

pub fn walk_body<V: Transformer + ?Sized>(visitor: &V, body: &mut [Stmt]) {
    for stmt in body {
        visitor.visit_stmt(stmt);
    }
}

pub fn walk_elif_else_clause<V: Transformer + ?Sized>(
    visitor: &V,
    elif_else_clause: &mut ElifElseClause,
) {
    if let Some(test) = &mut elif_else_clause.test {
        visitor.visit_expr(test);
    }
    visitor.visit_body(&mut elif_else_clause.body);
}

pub fn walk_stmt<V: Transformer + ?Sized>(visitor: &V, stmt: &mut Stmt) {
    match stmt {
        Stmt::FunctionDef(ast::StmtFunctionDef {
            parameters,
            body,
            decorator_list,
            returns,
            type_params,
            ..
        }) => {
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
        Stmt::ClassDef(ast::StmtClassDef {
            arguments,
            body,
            decorator_list,
            type_params,
            ..
        }) => {
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
        Stmt::Return(ast::StmtReturn { value, range: _ }) => {
            if let Some(expr) = value {
                visitor.visit_expr(expr);
            }
        }
        Stmt::Delete(ast::StmtDelete { targets, range: _ }) => {
            for expr in targets {
                visitor.visit_expr(expr);
            }
        }
        Stmt::TypeAlias(ast::StmtTypeAlias {
            range: _,
            name,
            type_params,
            value,
        }) => {
            visitor.visit_expr(value);
            if let Some(type_params) = type_params {
                visitor.visit_type_params(type_params);
            }
            visitor.visit_expr(name);
        }
        Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
            visitor.visit_expr(value);
            for expr in targets {
                visitor.visit_expr(expr);
            }
        }
        Stmt::AugAssign(ast::StmtAugAssign {
            target,
            op,
            value,
            range: _,
        }) => {
            visitor.visit_expr(value);
            visitor.visit_operator(op);
            visitor.visit_expr(target);
        }
        Stmt::AnnAssign(ast::StmtAnnAssign {
            target,
            annotation,
            value,
            ..
        }) => {
            if let Some(expr) = value {
                visitor.visit_expr(expr);
            }
            visitor.visit_annotation(annotation);
            visitor.visit_expr(target);
        }
        Stmt::For(ast::StmtFor {
            target,
            iter,
            body,
            orelse,
            ..
        }) => {
            visitor.visit_expr(iter);
            visitor.visit_expr(target);
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }
        Stmt::While(ast::StmtWhile {
            test,
            body,
            orelse,
            range: _,
        }) => {
            visitor.visit_expr(test);
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }
        Stmt::If(ast::StmtIf {
            test,
            body,
            elif_else_clauses,
            range: _,
        }) => {
            visitor.visit_expr(test);
            visitor.visit_body(body);
            for clause in elif_else_clauses {
                if let Some(test) = &mut clause.test {
                    visitor.visit_expr(test);
                }
                walk_elif_else_clause(visitor, clause);
            }
        }
        Stmt::With(ast::StmtWith { items, body, .. }) => {
            for with_item in items {
                visitor.visit_with_item(with_item);
            }
            visitor.visit_body(body);
        }
        Stmt::Match(ast::StmtMatch {
            subject,
            cases,
            range: _,
        }) => {
            visitor.visit_expr(subject);
            for match_case in cases {
                visitor.visit_match_case(match_case);
            }
        }
        Stmt::Raise(ast::StmtRaise {
            exc,
            cause,
            range: _,
        }) => {
            if let Some(expr) = exc {
                visitor.visit_expr(expr);
            }
            if let Some(expr) = cause {
                visitor.visit_expr(expr);
            }
        }
        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            is_star: _,
            range: _,
        }) => {
            visitor.visit_body(body);
            for except_handler in handlers {
                visitor.visit_except_handler(except_handler);
            }
            visitor.visit_body(orelse);
            visitor.visit_body(finalbody);
        }
        Stmt::Assert(ast::StmtAssert {
            test,
            msg,
            range: _,
        }) => {
            visitor.visit_expr(test);
            if let Some(expr) = msg {
                visitor.visit_expr(expr);
            }
        }
        Stmt::Import(ast::StmtImport { names, range: _ }) => {
            for alias in names {
                visitor.visit_alias(alias);
            }
        }
        Stmt::ImportFrom(ast::StmtImportFrom { names, .. }) => {
            for alias in names {
                visitor.visit_alias(alias);
            }
        }
        Stmt::Global(_) => {}
        Stmt::Nonlocal(_) => {}
        Stmt::Expr(ast::StmtExpr { value, range: _ }) => visitor.visit_expr(value),
        Stmt::Pass(_) | Stmt::Break(_) | Stmt::Continue(_) | Stmt::IpyEscapeCommand(_) => {}
    }
}

pub fn walk_annotation<V: Transformer + ?Sized>(visitor: &V, expr: &mut Expr) {
    visitor.visit_expr(expr);
}

pub fn walk_decorator<V: Transformer + ?Sized>(visitor: &V, decorator: &mut Decorator) {
    visitor.visit_expr(&mut decorator.expression);
}

pub fn walk_expr<V: Transformer + ?Sized>(visitor: &V, expr: &mut Expr) {
    match expr {
        Expr::BoolOp(ast::ExprBoolOp {
            op,
            values,
            range: _,
        }) => {
            visitor.visit_bool_op(op);
            for expr in values {
                visitor.visit_expr(expr);
            }
        }
        Expr::Named(ast::ExprNamed {
            target,
            value,
            range: _,
        }) => {
            visitor.visit_expr(value);
            visitor.visit_expr(target);
        }
        Expr::BinOp(ast::ExprBinOp {
            left,
            op,
            right,
            range: _,
        }) => {
            visitor.visit_expr(left);
            visitor.visit_operator(op);
            visitor.visit_expr(right);
        }
        Expr::UnaryOp(ast::ExprUnaryOp {
            op,
            operand,
            range: _,
        }) => {
            visitor.visit_unary_op(op);
            visitor.visit_expr(operand);
        }
        Expr::Lambda(ast::ExprLambda {
            parameters,
            body,
            range: _,
        }) => {
            if let Some(parameters) = parameters {
                visitor.visit_parameters(parameters);
            }
            visitor.visit_expr(body);
        }
        Expr::If(ast::ExprIf {
            test,
            body,
            orelse,
            range: _,
        }) => {
            visitor.visit_expr(test);
            visitor.visit_expr(body);
            visitor.visit_expr(orelse);
        }
        Expr::Dict(ast::ExprDict { items, range: _ }) => {
            for ast::DictItem { key, value } in items {
                if let Some(key) = key {
                    visitor.visit_expr(key);
                }
                visitor.visit_expr(value);
            }
        }
        Expr::Set(ast::ExprSet { elts, range: _ }) => {
            for expr in elts {
                visitor.visit_expr(expr);
            }
        }
        Expr::ListComp(ast::ExprListComp {
            elt,
            generators,
            range: _,
        }) => {
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
            visitor.visit_expr(elt);
        }
        Expr::SetComp(ast::ExprSetComp {
            elt,
            generators,
            range: _,
        }) => {
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
            visitor.visit_expr(elt);
        }
        Expr::DictComp(ast::ExprDictComp {
            key,
            value,
            generators,
            range: _,
        }) => {
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
            visitor.visit_expr(key);
            visitor.visit_expr(value);
        }
        Expr::Generator(ast::ExprGenerator {
            elt,
            generators,
            range: _,
            parenthesized: _,
        }) => {
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
            visitor.visit_expr(elt);
        }
        Expr::Await(ast::ExprAwait { value, range: _ }) => visitor.visit_expr(value),
        Expr::Yield(ast::ExprYield { value, range: _ }) => {
            if let Some(expr) = value {
                visitor.visit_expr(expr);
            }
        }
        Expr::YieldFrom(ast::ExprYieldFrom { value, range: _ }) => visitor.visit_expr(value),
        Expr::Compare(ast::ExprCompare {
            left,
            ops,
            comparators,
            range: _,
        }) => {
            visitor.visit_expr(left);
            for cmp_op in &mut **ops {
                visitor.visit_cmp_op(cmp_op);
            }
            for expr in &mut **comparators {
                visitor.visit_expr(expr);
            }
        }
        Expr::Call(ast::ExprCall {
            func,
            arguments,
            range: _,
        }) => {
            visitor.visit_expr(func);
            visitor.visit_arguments(arguments);
        }
        Expr::FString(ast::ExprFString { value, .. }) => {
            for f_string_part in value.iter_mut() {
                match f_string_part {
                    ast::FStringPart::Literal(string_literal) => {
                        visitor.visit_string_literal(string_literal);
                    }
                    ast::FStringPart::FString(f_string) => {
                        visitor.visit_f_string(f_string);
                    }
                }
            }
        }
        Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => {
            for string_literal in value.iter_mut() {
                visitor.visit_string_literal(string_literal);
            }
        }
        Expr::BytesLiteral(ast::ExprBytesLiteral { value, .. }) => {
            for bytes_literal in value.iter_mut() {
                visitor.visit_bytes_literal(bytes_literal);
            }
        }
        Expr::NumberLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::NoneLiteral(_)
        | Expr::EllipsisLiteral(_) => {}
        Expr::Attribute(ast::ExprAttribute { value, ctx, .. }) => {
            visitor.visit_expr(value);
            visitor.visit_expr_context(ctx);
        }
        Expr::Subscript(ast::ExprSubscript {
            value,
            slice,
            ctx,
            range: _,
        }) => {
            visitor.visit_expr(value);
            visitor.visit_expr(slice);
            visitor.visit_expr_context(ctx);
        }
        Expr::Starred(ast::ExprStarred {
            value,
            ctx,
            range: _,
        }) => {
            visitor.visit_expr(value);
            visitor.visit_expr_context(ctx);
        }
        Expr::Name(ast::ExprName { ctx, .. }) => {
            visitor.visit_expr_context(ctx);
        }
        Expr::List(ast::ExprList {
            elts,
            ctx,
            range: _,
        }) => {
            for expr in elts {
                visitor.visit_expr(expr);
            }
            visitor.visit_expr_context(ctx);
        }
        Expr::Tuple(ast::ExprTuple {
            elts,
            ctx,
            range: _,
            parenthesized: _,
        }) => {
            for expr in elts {
                visitor.visit_expr(expr);
            }
            visitor.visit_expr_context(ctx);
        }
        Expr::Slice(ast::ExprSlice {
            lower,
            upper,
            step,
            range: _,
        }) => {
            if let Some(expr) = lower {
                visitor.visit_expr(expr);
            }
            if let Some(expr) = upper {
                visitor.visit_expr(expr);
            }
            if let Some(expr) = step {
                visitor.visit_expr(expr);
            }
        }
        Expr::IpyEscapeCommand(_) => {}
    }
}

pub fn walk_comprehension<V: Transformer + ?Sized>(visitor: &V, comprehension: &mut Comprehension) {
    visitor.visit_expr(&mut comprehension.iter);
    visitor.visit_expr(&mut comprehension.target);
    for expr in &mut comprehension.ifs {
        visitor.visit_expr(expr);
    }
}

pub fn walk_except_handler<V: Transformer + ?Sized>(
    visitor: &V,
    except_handler: &mut ExceptHandler,
) {
    match except_handler {
        ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { type_, body, .. }) => {
            if let Some(expr) = type_ {
                visitor.visit_expr(expr);
            }
            visitor.visit_body(body);
        }
    }
}

pub fn walk_arguments<V: Transformer + ?Sized>(visitor: &V, arguments: &mut Arguments) {
    // Note that the there might be keywords before the last arg, e.g. in
    // f(*args, a=2, *args2, **kwargs)`, but we follow Python in evaluating first `args` and then
    // `keywords`. See also [Arguments::arguments_source_order`].
    for arg in &mut *arguments.args {
        visitor.visit_expr(arg);
    }
    for keyword in &mut *arguments.keywords {
        visitor.visit_keyword(keyword);
    }
}

pub fn walk_parameters<V: Transformer + ?Sized>(visitor: &V, parameters: &mut Parameters) {
    // Defaults are evaluated before annotations.
    for arg in &mut parameters.posonlyargs {
        if let Some(default) = &mut arg.default {
            visitor.visit_expr(default);
        }
    }
    for arg in &mut parameters.args {
        if let Some(default) = &mut arg.default {
            visitor.visit_expr(default);
        }
    }
    for arg in &mut parameters.kwonlyargs {
        if let Some(default) = &mut arg.default {
            visitor.visit_expr(default);
        }
    }

    for arg in &mut parameters.posonlyargs {
        visitor.visit_parameter(&mut arg.parameter);
    }
    for arg in &mut parameters.args {
        visitor.visit_parameter(&mut arg.parameter);
    }
    if let Some(arg) = &mut parameters.vararg {
        visitor.visit_parameter(arg);
    }
    for arg in &mut parameters.kwonlyargs {
        visitor.visit_parameter(&mut arg.parameter);
    }
    if let Some(arg) = &mut parameters.kwarg {
        visitor.visit_parameter(arg);
    }
}

pub fn walk_parameter<V: Transformer + ?Sized>(visitor: &V, parameter: &mut Parameter) {
    if let Some(expr) = &mut parameter.annotation {
        visitor.visit_annotation(expr);
    }
}

pub fn walk_keyword<V: Transformer + ?Sized>(visitor: &V, keyword: &mut Keyword) {
    visitor.visit_expr(&mut keyword.value);
}

pub fn walk_with_item<V: Transformer + ?Sized>(visitor: &V, with_item: &mut WithItem) {
    visitor.visit_expr(&mut with_item.context_expr);
    if let Some(expr) = &mut with_item.optional_vars {
        visitor.visit_expr(expr);
    }
}

pub fn walk_type_params<V: Transformer + ?Sized>(visitor: &V, type_params: &mut TypeParams) {
    for type_param in &mut type_params.type_params {
        visitor.visit_type_param(type_param);
    }
}

pub fn walk_type_param<V: Transformer + ?Sized>(visitor: &V, type_param: &mut TypeParam) {
    match type_param {
        TypeParam::TypeVar(TypeParamTypeVar {
            bound,
            default,
            name: _,
            range: _,
        }) => {
            if let Some(expr) = bound {
                visitor.visit_expr(expr);
            }
            if let Some(expr) = default {
                visitor.visit_expr(expr);
            }
        }
        TypeParam::TypeVarTuple(TypeParamTypeVarTuple {
            default,
            name: _,
            range: _,
        }) => {
            if let Some(expr) = default {
                visitor.visit_expr(expr);
            }
        }
        TypeParam::ParamSpec(TypeParamParamSpec {
            default,
            name: _,
            range: _,
        }) => {
            if let Some(expr) = default {
                visitor.visit_expr(expr);
            }
        }
    }
}

pub fn walk_match_case<V: Transformer + ?Sized>(visitor: &V, match_case: &mut MatchCase) {
    visitor.visit_pattern(&mut match_case.pattern);
    if let Some(expr) = &mut match_case.guard {
        visitor.visit_expr(expr);
    }
    visitor.visit_body(&mut match_case.body);
}

pub fn walk_pattern<V: Transformer + ?Sized>(visitor: &V, pattern: &mut Pattern) {
    match pattern {
        Pattern::MatchValue(ast::PatternMatchValue { value, .. }) => {
            visitor.visit_expr(value);
        }
        Pattern::MatchSingleton(_) => {}
        Pattern::MatchSequence(ast::PatternMatchSequence { patterns, .. }) => {
            for pattern in patterns {
                visitor.visit_pattern(pattern);
            }
        }
        Pattern::MatchMapping(ast::PatternMatchMapping { keys, patterns, .. }) => {
            for expr in keys {
                visitor.visit_expr(expr);
            }
            for pattern in patterns {
                visitor.visit_pattern(pattern);
            }
        }
        Pattern::MatchClass(ast::PatternMatchClass { cls, arguments, .. }) => {
            visitor.visit_expr(cls);
            visitor.visit_pattern_arguments(arguments);
        }
        Pattern::MatchStar(_) => {}
        Pattern::MatchAs(ast::PatternMatchAs { pattern, .. }) => {
            if let Some(pattern) = pattern {
                visitor.visit_pattern(pattern);
            }
        }
        Pattern::MatchOr(ast::PatternMatchOr { patterns, .. }) => {
            for pattern in patterns {
                visitor.visit_pattern(pattern);
            }
        }
    }
}

pub fn walk_pattern_arguments<V: Transformer + ?Sized>(
    visitor: &V,
    pattern_arguments: &mut PatternArguments,
) {
    for pattern in &mut pattern_arguments.patterns {
        visitor.visit_pattern(pattern);
    }
    for keyword in &mut pattern_arguments.keywords {
        visitor.visit_pattern_keyword(keyword);
    }
}

pub fn walk_pattern_keyword<V: Transformer + ?Sized>(
    visitor: &V,
    pattern_keyword: &mut PatternKeyword,
) {
    visitor.visit_pattern(&mut pattern_keyword.pattern);
}

pub fn walk_f_string<V: Transformer + ?Sized>(visitor: &V, f_string: &mut FString) {
    for element in &mut f_string.elements {
        visitor.visit_f_string_element(element);
    }
}

pub fn walk_f_string_element<V: Transformer + ?Sized>(
    visitor: &V,
    f_string_element: &mut FStringElement,
) {
    if let ast::FStringElement::Expression(ast::FStringExpressionElement {
        expression,
        format_spec,
        ..
    }) = f_string_element
    {
        visitor.visit_expr(expression);
        if let Some(format_spec) = format_spec {
            for spec_element in &mut format_spec.elements {
                visitor.visit_f_string_element(spec_element);
            }
        }
    }
}

pub fn walk_expr_context<V: Transformer + ?Sized>(_visitor: &V, _expr_context: &mut ExprContext) {}

pub fn walk_bool_op<V: Transformer + ?Sized>(_visitor: &V, _bool_op: &mut BoolOp) {}

pub fn walk_operator<V: Transformer + ?Sized>(_visitor: &V, _operator: &mut Operator) {}

pub fn walk_unary_op<V: Transformer + ?Sized>(_visitor: &V, _unary_op: &mut UnaryOp) {}

pub fn walk_cmp_op<V: Transformer + ?Sized>(_visitor: &V, _cmp_op: &mut CmpOp) {}

pub fn walk_alias<V: Transformer + ?Sized>(_visitor: &V, _alias: &mut Alias) {}

pub fn walk_string_literal<V: Transformer + ?Sized>(
    _visitor: &V,
    _string_literal: &mut StringLiteral,
) {
}

pub fn walk_bytes_literal<V: Transformer + ?Sized>(
    _visitor: &V,
    _bytes_literal: &mut BytesLiteral,
) {
}
