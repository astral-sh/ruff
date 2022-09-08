use rustpython_parser::ast::{
    Alias, Arg, Arguments, Boolop, Cmpop, Comprehension, Constant, Excepthandler,
    ExcepthandlerKind, Expr, ExprContext, ExprKind, Keyword, MatchCase, Operator, Pattern,
    PatternKind, Stmt, StmtKind, Unaryop, Withitem,
};

pub trait Visitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        walk_stmt(self, stmt);
    }
    fn visit_annotation(&mut self, expr: &'a Expr) {
        walk_expr(self, expr);
    }
    fn visit_expr(&mut self, expr: &'a Expr) {
        walk_expr(self, expr);
    }
    fn visit_constant(&mut self, constant: &'a Constant) {
        walk_constant(self, constant);
    }
    fn visit_expr_context(&mut self, expr_content: &'a ExprContext) {
        walk_expr_context(self, expr_content);
    }
    fn visit_boolop(&mut self, boolop: &'a Boolop) {
        walk_boolop(self, boolop);
    }
    fn visit_operator(&mut self, operator: &'a Operator) {
        walk_operator(self, operator);
    }
    fn visit_unaryop(&mut self, unaryop: &'a Unaryop) {
        walk_unaryop(self, unaryop);
    }
    fn visit_cmpop(&mut self, cmpop: &'a Cmpop) {
        walk_cmpop(self, cmpop);
    }
    fn visit_comprehension(&mut self, comprehension: &'a Comprehension) {
        walk_comprehension(self, comprehension);
    }
    fn visit_excepthandler(&mut self, excepthandler: &'a Excepthandler) {
        walk_excepthandler(self, excepthandler);
    }
    fn visit_arguments(&mut self, arguments: &'a Arguments) {
        walk_arguments(self, arguments);
    }
    fn visit_arg(&mut self, arg: &'a Arg) {
        walk_arg(self, arg);
    }
    fn visit_keyword(&mut self, keyword: &'a Keyword) {
        walk_keyword(self, keyword);
    }
    fn visit_alias(&mut self, alias: &'a Alias) {
        walk_alias(self, alias);
    }
    fn visit_withitem(&mut self, withitem: &'a Withitem) {
        walk_withitem(self, withitem);
    }
    fn visit_match_case(&mut self, match_case: &'a MatchCase) {
        walk_match_case(self, match_case);
    }
    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        walk_pattern(self, pattern);
    }
}

pub fn walk_stmt<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, stmt: &'a Stmt) {
    match &stmt.node {
        StmtKind::FunctionDef {
            args,
            body,
            decorator_list,
            returns,
            ..
        } => {
            visitor.visit_arguments(args);
            for expr in decorator_list {
                visitor.visit_expr(expr);
            }
            for expr in returns {
                visitor.visit_annotation(expr);
            }
            for stmt in body {
                visitor.visit_stmt(stmt);
            }
        }
        StmtKind::AsyncFunctionDef {
            args,
            body,
            decorator_list,
            returns,
            ..
        } => {
            visitor.visit_arguments(args);
            for expr in decorator_list {
                visitor.visit_expr(expr);
            }
            for expr in returns {
                visitor.visit_annotation(expr);
            }
            for stmt in body {
                visitor.visit_stmt(stmt);
            }
        }
        StmtKind::ClassDef {
            bases,
            keywords,
            body,
            decorator_list,
            ..
        } => {
            for expr in bases {
                visitor.visit_expr(expr);
            }
            for keyword in keywords {
                visitor.visit_keyword(keyword);
            }
            for expr in decorator_list {
                visitor.visit_expr(expr);
            }
            for stmt in body {
                visitor.visit_stmt(stmt);
            }
        }
        StmtKind::Return { value } => {
            if let Some(expr) = value {
                visitor.visit_expr(expr);
            }
        }
        StmtKind::Delete { targets } => {
            for expr in targets {
                visitor.visit_expr(expr);
            }
        }
        StmtKind::Assign { targets, value, .. } => {
            visitor.visit_expr(value);
            for expr in targets {
                visitor.visit_expr(expr);
            }
        }
        StmtKind::AugAssign { target, op, value } => {
            visitor.visit_expr(target);
            visitor.visit_operator(op);
            visitor.visit_expr(value);
        }
        StmtKind::AnnAssign {
            target,
            annotation,
            value,
            ..
        } => {
            visitor.visit_annotation(annotation);
            if let Some(expr) = value {
                visitor.visit_expr(expr);
            }
            visitor.visit_expr(target);
        }
        StmtKind::For {
            target,
            iter,
            body,
            orelse,
            ..
        } => {
            visitor.visit_expr(target);
            visitor.visit_expr(iter);
            for stmt in body {
                visitor.visit_stmt(stmt);
            }
            for stmt in orelse {
                visitor.visit_stmt(stmt);
            }
        }
        StmtKind::AsyncFor {
            target,
            iter,
            body,
            orelse,
            ..
        } => {
            visitor.visit_expr(target);
            visitor.visit_expr(iter);
            for stmt in body {
                visitor.visit_stmt(stmt);
            }
            for stmt in orelse {
                visitor.visit_stmt(stmt);
            }
        }
        StmtKind::While { test, body, orelse } => {
            visitor.visit_expr(test);
            for stmt in body {
                visitor.visit_stmt(stmt);
            }
            for stmt in orelse {
                visitor.visit_stmt(stmt);
            }
        }
        StmtKind::If { test, body, orelse } => {
            visitor.visit_expr(test);
            for stmt in body {
                visitor.visit_stmt(stmt);
            }
            for stmt in orelse {
                visitor.visit_stmt(stmt);
            }
        }
        StmtKind::With { items, body, .. } => {
            for withitem in items {
                visitor.visit_withitem(withitem);
            }
            for stmt in body {
                visitor.visit_stmt(stmt);
            }
        }
        StmtKind::AsyncWith { items, body, .. } => {
            for withitem in items {
                visitor.visit_withitem(withitem);
            }
            for stmt in body {
                visitor.visit_stmt(stmt);
            }
        }
        StmtKind::Match { subject, cases } => {
            // TODO(charlie): Handle `cases`.
            visitor.visit_expr(subject);
            for match_case in cases {
                visitor.visit_match_case(match_case);
            }
        }
        StmtKind::Raise { exc, cause } => {
            if let Some(expr) = exc {
                visitor.visit_expr(expr);
            };
            if let Some(expr) = cause {
                visitor.visit_expr(expr);
            };
        }
        StmtKind::Try {
            body,
            handlers,
            orelse,
            finalbody,
        } => {
            for stmt in body {
                visitor.visit_stmt(stmt);
            }
            for excepthandler in handlers {
                visitor.visit_excepthandler(excepthandler)
            }
            for stmt in orelse {
                visitor.visit_stmt(stmt);
            }
            for stmt in finalbody {
                visitor.visit_stmt(stmt);
            }
        }
        StmtKind::Assert { test, msg } => {
            visitor.visit_expr(test);
            if let Some(expr) = msg {
                visitor.visit_expr(expr);
            }
        }
        StmtKind::Import { names } => {
            for alias in names {
                visitor.visit_alias(alias);
            }
        }
        StmtKind::ImportFrom { names, .. } => {
            for alias in names {
                visitor.visit_alias(alias);
            }
        }
        StmtKind::Global { .. } => {}
        StmtKind::Nonlocal { .. } => {}
        StmtKind::Expr { value } => visitor.visit_expr(value),
        StmtKind::Pass => {}
        StmtKind::Break => {}
        StmtKind::Continue => {}
    }
}

pub fn walk_expr<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, expr: &'a Expr) {
    match &expr.node {
        ExprKind::BoolOp { op, values } => {
            visitor.visit_boolop(op);
            for expr in values {
                visitor.visit_expr(expr);
            }
        }
        ExprKind::NamedExpr { target, value } => {
            visitor.visit_expr(target);
            visitor.visit_expr(value);
        }
        ExprKind::BinOp { left, op, right } => {
            visitor.visit_expr(left);
            visitor.visit_operator(op);
            visitor.visit_expr(right);
        }
        ExprKind::UnaryOp { op, operand } => {
            visitor.visit_unaryop(op);
            visitor.visit_expr(operand);
        }
        ExprKind::Lambda { args, body } => {
            visitor.visit_arguments(args);
            visitor.visit_expr(body);
        }
        ExprKind::IfExp { test, body, orelse } => {
            visitor.visit_expr(test);
            visitor.visit_expr(body);
            visitor.visit_expr(orelse);
        }
        ExprKind::Dict { keys, values } => {
            for expr in keys {
                visitor.visit_expr(expr);
            }
            for expr in values {
                visitor.visit_expr(expr);
            }
        }
        ExprKind::Set { elts } => {
            for expr in elts {
                visitor.visit_expr(expr);
            }
        }
        ExprKind::ListComp { elt, generators } => {
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
            visitor.visit_expr(elt);
        }
        ExprKind::SetComp { elt, generators } => {
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
            visitor.visit_expr(elt);
        }
        ExprKind::DictComp {
            key,
            value,
            generators,
        } => {
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
            visitor.visit_expr(key);
            visitor.visit_expr(value);
        }
        ExprKind::GeneratorExp { elt, generators } => {
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
            visitor.visit_expr(elt);
        }
        ExprKind::Await { value } => visitor.visit_expr(value),
        ExprKind::Yield { value } => {
            if let Some(expr) = value {
                visitor.visit_expr(expr);
            }
        }
        ExprKind::YieldFrom { value } => visitor.visit_expr(value),
        ExprKind::Compare {
            left,
            ops,
            comparators,
        } => {
            visitor.visit_expr(left);
            for cmpop in ops {
                visitor.visit_cmpop(cmpop);
            }
            for expr in comparators {
                visitor.visit_expr(expr);
            }
        }
        ExprKind::Call {
            func,
            args,
            keywords,
        } => {
            visitor.visit_expr(func);
            for expr in args {
                visitor.visit_expr(expr);
            }
            for keyword in keywords {
                visitor.visit_keyword(keyword);
            }
        }
        ExprKind::FormattedValue {
            value, format_spec, ..
        } => {
            visitor.visit_expr(value);
            if let Some(expr) = format_spec {
                visitor.visit_expr(expr);
            }
        }
        ExprKind::JoinedStr { values } => {
            for expr in values {
                visitor.visit_expr(expr);
            }
        }
        ExprKind::Constant { value, .. } => visitor.visit_constant(value),
        ExprKind::Attribute { value, ctx, .. } => {
            visitor.visit_expr(value);
            visitor.visit_expr_context(ctx);
        }
        ExprKind::Subscript { value, slice, ctx } => {
            visitor.visit_expr(value);
            visitor.visit_expr(slice);
            visitor.visit_expr_context(ctx);
        }
        ExprKind::Starred { value, ctx } => {
            visitor.visit_expr(value);
            visitor.visit_expr_context(ctx);
        }
        ExprKind::Name { ctx, .. } => {
            visitor.visit_expr_context(ctx);
        }
        ExprKind::List { elts, ctx } => {
            for expr in elts {
                visitor.visit_expr(expr);
            }
            visitor.visit_expr_context(ctx);
        }
        ExprKind::Tuple { elts, ctx } => {
            for expr in elts {
                visitor.visit_expr(expr);
            }
            visitor.visit_expr_context(ctx);
        }
        ExprKind::Slice { lower, upper, step } => {
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
    }
}

pub fn walk_constant<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, constant: &'a Constant) {
    if let Constant::Tuple(constants) = constant {
        for constant in constants {
            visitor.visit_constant(constant)
        }
    }
}

pub fn walk_comprehension<'a, V: Visitor<'a> + ?Sized>(
    visitor: &mut V,
    comprehension: &'a Comprehension,
) {
    visitor.visit_expr(&comprehension.target);
    visitor.visit_expr(&comprehension.iter);
    for expr in &comprehension.ifs {
        visitor.visit_expr(expr);
    }
}

pub fn walk_excepthandler<'a, V: Visitor<'a> + ?Sized>(
    visitor: &mut V,
    excepthandler: &'a Excepthandler,
) {
    match &excepthandler.node {
        ExcepthandlerKind::ExceptHandler { type_, body, .. } => {
            if let Some(expr) = type_ {
                visitor.visit_expr(expr);
            }
            for stmt in body {
                visitor.visit_stmt(stmt);
            }
        }
    }
}

pub fn walk_arguments<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, arguments: &'a Arguments) {
    for arg in &arguments.posonlyargs {
        visitor.visit_arg(arg);
    }
    for arg in &arguments.args {
        visitor.visit_arg(arg);
    }
    if let Some(arg) = &arguments.vararg {
        visitor.visit_arg(arg);
    }
    for arg in &arguments.kwonlyargs {
        visitor.visit_arg(arg);
    }
    for expr in &arguments.kw_defaults {
        visitor.visit_expr(expr);
    }
    if let Some(arg) = &arguments.kwarg {
        visitor.visit_arg(arg);
    }
    for expr in &arguments.defaults {
        visitor.visit_expr(expr);
    }
}

pub fn walk_arg<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, arg: &'a Arg) {
    if let Some(expr) = &arg.node.annotation {
        visitor.visit_annotation(expr);
    }
}

pub fn walk_keyword<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, keyword: &'a Keyword) {
    visitor.visit_expr(&keyword.node.value);
}

pub fn walk_withitem<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, withitem: &'a Withitem) {
    visitor.visit_expr(&withitem.context_expr);
    if let Some(expr) = &withitem.optional_vars {
        visitor.visit_expr(expr);
    }
}

pub fn walk_match_case<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, match_case: &'a MatchCase) {
    visitor.visit_pattern(&match_case.pattern);
    if let Some(expr) = &match_case.guard {
        visitor.visit_expr(expr);
    }
    for stmt in &match_case.body {
        visitor.visit_stmt(stmt);
    }
}

pub fn walk_pattern<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, pattern: &'a Pattern) {
    match &pattern.node {
        PatternKind::MatchValue { value } => visitor.visit_expr(value),
        PatternKind::MatchSingleton { value } => visitor.visit_constant(value),
        PatternKind::MatchSequence { patterns } => {
            for pattern in patterns {
                visitor.visit_pattern(pattern);
            }
        }
        PatternKind::MatchMapping { keys, patterns, .. } => {
            for expr in keys {
                visitor.visit_expr(expr);
            }
            for pattern in patterns {
                visitor.visit_pattern(pattern);
            }
        }
        PatternKind::MatchClass {
            cls,
            patterns,
            kwd_patterns,
            ..
        } => {
            visitor.visit_expr(cls);
            for pattern in patterns {
                visitor.visit_pattern(pattern);
            }

            for pattern in kwd_patterns {
                visitor.visit_pattern(pattern);
            }
        }
        PatternKind::MatchStar { .. } => {}
        PatternKind::MatchAs { pattern, .. } => {
            if let Some(pattern) = pattern {
                visitor.visit_pattern(pattern);
            }
        }
        PatternKind::MatchOr { patterns } => {
            for pattern in patterns {
                visitor.visit_pattern(pattern);
            }
        }
    }
}

#[allow(unused_variables)]
#[inline(always)]
pub fn walk_expr_context<'a, V: Visitor<'a> + ?Sized>(
    visitor: &mut V,
    expr_context: &'a ExprContext,
) {
}

#[allow(unused_variables)]
#[inline(always)]
pub fn walk_boolop<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, boolop: &'a Boolop) {}

#[allow(unused_variables)]
#[inline(always)]
pub fn walk_operator<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, operator: &'a Operator) {}

#[allow(unused_variables)]
#[inline(always)]
pub fn walk_unaryop<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, unaryop: &'a Unaryop) {}

#[allow(unused_variables)]
#[inline(always)]
pub fn walk_cmpop<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, cmpop: &'a Cmpop) {}

#[allow(unused_variables)]
#[inline(always)]
pub fn walk_alias<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, alias: &'a Alias) {}
