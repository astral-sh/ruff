use rustpython_parser::ast::{
    Alias, Arg, Arguments, Boolop, Cmpop, Comprehension, Constant, Excepthandler,
    ExcepthandlerKind, Expr, ExprContext, ExprKind, Keyword, MatchCase, Operator, Pattern,
    PatternKind, Stmt, StmtKind, Unaryop, Withitem,
};

pub trait Visitor {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        walk_stmt(self, stmt);
    }
    fn visit_annotation(&mut self, expr: &Expr) {
        walk_expr(self, expr);
    }
    fn visit_expr(&mut self, expr: &Expr, _parent: Option<&Stmt>) {
        walk_expr(self, expr);
    }
    fn visit_constant(&mut self, constant: &Constant) {
        walk_constant(self, constant);
    }
    fn visit_expr_context(&mut self, expr_content: &ExprContext) {
        walk_expr_context(self, expr_content);
    }
    fn visit_boolop(&mut self, boolop: &Boolop) {
        walk_boolop(self, boolop);
    }
    fn visit_operator(&mut self, operator: &Operator) {
        walk_operator(self, operator);
    }
    fn visit_unaryop(&mut self, unaryop: &Unaryop) {
        walk_unaryop(self, unaryop);
    }
    fn visit_cmpop(&mut self, cmpop: &Cmpop) {
        walk_cmpop(self, cmpop);
    }
    fn visit_comprehension(&mut self, comprehension: &Comprehension) {
        walk_comprehension(self, comprehension);
    }
    fn visit_excepthandler(&mut self, excepthandler: &Excepthandler) {
        walk_excepthandler(self, excepthandler);
    }
    fn visit_arguments(&mut self, arguments: &Arguments) {
        walk_arguments(self, arguments);
    }
    fn visit_arg(&mut self, arg: &Arg) {
        walk_arg(self, arg);
    }
    fn visit_keyword(&mut self, keyword: &Keyword) {
        walk_keyword(self, keyword);
    }
    fn visit_alias(&mut self, alias: &Alias) {
        walk_alias(self, alias);
    }
    fn visit_withitem(&mut self, withitem: &Withitem) {
        walk_withitem(self, withitem);
    }
    fn visit_match_case(&mut self, match_case: &MatchCase) {
        walk_match_case(self, match_case);
    }
    fn visit_pattern(&mut self, pattern: &Pattern) {
        walk_pattern(self, pattern);
    }
}

pub fn walk_stmt<V: Visitor + ?Sized>(visitor: &mut V, stmt: &Stmt) {
    match &stmt.node {
        StmtKind::FunctionDef { args, body, .. } => {
            visitor.visit_arguments(args);
            for stmt in body {
                visitor.visit_stmt(stmt)
            }
        }
        StmtKind::AsyncFunctionDef { args, body, .. } => {
            visitor.visit_arguments(args);
            for stmt in body {
                visitor.visit_stmt(stmt)
            }
        }
        StmtKind::ClassDef { body, .. } => {
            for stmt in body {
                visitor.visit_stmt(stmt)
            }
        }
        StmtKind::Return { value } => {
            if let Some(expr) = value {
                visitor.visit_expr(expr, Some(stmt))
            }
        }
        StmtKind::Delete { targets } => {
            for expr in targets {
                visitor.visit_expr(expr, Some(stmt))
            }
        }
        StmtKind::Assign { targets, value, .. } => {
            for expr in targets {
                visitor.visit_expr(expr, Some(stmt))
            }
            visitor.visit_expr(value, Some(stmt))
        }
        StmtKind::AugAssign { target, op, value } => {
            visitor.visit_expr(target, Some(stmt));
            visitor.visit_operator(op);
            visitor.visit_expr(value, Some(stmt));
        }
        StmtKind::AnnAssign {
            target,
            annotation,
            value,
            ..
        } => {
            visitor.visit_expr(target, Some(stmt));
            visitor.visit_annotation(annotation);
            if let Some(expr) = value {
                visitor.visit_expr(expr, Some(stmt))
            }
        }
        StmtKind::For {
            target,
            iter,
            body,
            orelse,
            ..
        } => {
            visitor.visit_expr(target, Some(stmt));
            visitor.visit_expr(iter, Some(stmt));
            for stmt in body {
                visitor.visit_stmt(stmt)
            }
            for stmt in orelse {
                visitor.visit_stmt(stmt)
            }
        }
        StmtKind::AsyncFor {
            target,
            iter,
            body,
            orelse,
            ..
        } => {
            visitor.visit_expr(target, Some(stmt));
            visitor.visit_expr(iter, Some(stmt));
            for stmt in body {
                visitor.visit_stmt(stmt)
            }
            for stmt in orelse {
                visitor.visit_stmt(stmt)
            }
        }
        StmtKind::While { test, body, orelse } => {
            visitor.visit_expr(test, Some(stmt));
            for stmt in body {
                visitor.visit_stmt(stmt)
            }
            for stmt in orelse {
                visitor.visit_stmt(stmt)
            }
        }
        StmtKind::If { test, body, orelse } => {
            visitor.visit_expr(test, Some(stmt));
            for stmt in body {
                visitor.visit_stmt(stmt)
            }
            for stmt in orelse {
                visitor.visit_stmt(stmt)
            }
        }
        StmtKind::With { items, body, .. } => {
            for withitem in items {
                visitor.visit_withitem(withitem);
            }
            for stmt in body {
                visitor.visit_stmt(stmt)
            }
        }
        StmtKind::AsyncWith { items, body, .. } => {
            for withitem in items {
                visitor.visit_withitem(withitem);
            }
            for stmt in body {
                visitor.visit_stmt(stmt)
            }
        }
        StmtKind::Match { subject, cases } => {
            // TODO(charlie): Handle `cases`.
            visitor.visit_expr(subject, Some(stmt));
            for match_case in cases {
                visitor.visit_match_case(match_case);
            }
        }
        StmtKind::Raise { exc, cause } => {
            if let Some(expr) = exc {
                visitor.visit_expr(expr, Some(stmt))
            };
            if let Some(expr) = cause {
                visitor.visit_expr(expr, Some(stmt))
            };
        }
        StmtKind::Try {
            body,
            handlers,
            orelse,
            finalbody,
        } => {
            for stmt in body {
                visitor.visit_stmt(stmt)
            }
            for excepthandler in handlers {
                visitor.visit_excepthandler(excepthandler)
            }
            for stmt in orelse {
                visitor.visit_stmt(stmt)
            }
            for stmt in finalbody {
                visitor.visit_stmt(stmt)
            }
        }
        StmtKind::Assert { test, msg } => {
            visitor.visit_expr(test, None);
            if let Some(expr) = msg {
                visitor.visit_expr(expr, Some(stmt))
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
        StmtKind::Expr { value } => visitor.visit_expr(value, Some(stmt)),
        StmtKind::Pass => {}
        StmtKind::Break => {}
        StmtKind::Continue => {}
    }
}

pub fn walk_expr<V: Visitor + ?Sized>(visitor: &mut V, expr: &Expr) {
    match &expr.node {
        ExprKind::BoolOp { op, values } => {
            visitor.visit_boolop(op);
            for expr in values {
                visitor.visit_expr(expr, None)
            }
        }
        ExprKind::NamedExpr { target, value } => {
            visitor.visit_expr(target, None);
            visitor.visit_expr(value, None);
        }
        ExprKind::BinOp { left, op, right } => {
            visitor.visit_expr(left, None);
            visitor.visit_operator(op);
            visitor.visit_expr(right, None);
        }
        ExprKind::UnaryOp { op, operand } => {
            visitor.visit_unaryop(op);
            visitor.visit_expr(operand, None);
        }
        ExprKind::Lambda { args, body } => {
            visitor.visit_arguments(args);
            visitor.visit_expr(body, None);
        }
        ExprKind::IfExp { test, body, orelse } => {
            visitor.visit_expr(test, None);
            visitor.visit_expr(body, None);
            visitor.visit_expr(orelse, None);
        }
        ExprKind::Dict { keys, values } => {
            for expr in keys {
                visitor.visit_expr(expr, None)
            }
            for expr in values {
                visitor.visit_expr(expr, None)
            }
        }
        ExprKind::Set { elts } => {
            for expr in elts {
                visitor.visit_expr(expr, None)
            }
        }
        ExprKind::ListComp { elt, generators } => {
            for comprehension in generators {
                visitor.visit_comprehension(comprehension)
            }
            visitor.visit_expr(elt, None);
        }
        ExprKind::SetComp { elt, generators } => {
            for comprehension in generators {
                visitor.visit_comprehension(comprehension)
            }
            visitor.visit_expr(elt, None);
        }
        ExprKind::DictComp {
            key,
            value,
            generators,
        } => {
            for comprehension in generators {
                visitor.visit_comprehension(comprehension)
            }
            visitor.visit_expr(key, None);
            visitor.visit_expr(value, None);
        }
        ExprKind::GeneratorExp { elt, generators } => {
            for comprehension in generators {
                visitor.visit_comprehension(comprehension)
            }
            visitor.visit_expr(elt, None);
        }
        ExprKind::Await { value } => visitor.visit_expr(value, None),
        ExprKind::Yield { value } => {
            if let Some(expr) = value {
                visitor.visit_expr(expr, None)
            }
        }
        ExprKind::YieldFrom { value } => visitor.visit_expr(value, None),
        ExprKind::Compare {
            left,
            ops,
            comparators,
        } => {
            visitor.visit_expr(left, None);
            for cmpop in ops {
                visitor.visit_cmpop(cmpop);
            }
            for expr in comparators {
                visitor.visit_expr(expr, None)
            }
        }
        ExprKind::Call {
            func,
            args,
            keywords,
        } => {
            visitor.visit_expr(func, None);
            for expr in args {
                visitor.visit_expr(expr, None);
            }
            for keyword in keywords {
                visitor.visit_keyword(keyword);
            }
        }
        ExprKind::FormattedValue {
            value, format_spec, ..
        } => {
            visitor.visit_expr(value, None);
            if let Some(expr) = format_spec {
                visitor.visit_expr(expr, None)
            }
        }
        ExprKind::JoinedStr { values } => {
            for expr in values {
                visitor.visit_expr(expr, None)
            }
        }
        ExprKind::Constant { value, .. } => visitor.visit_constant(value),
        ExprKind::Attribute { value, ctx, .. } => {
            visitor.visit_expr(value, None);
            visitor.visit_expr_context(ctx);
        }
        ExprKind::Subscript { value, slice, ctx } => {
            visitor.visit_expr(value, None);
            visitor.visit_expr(slice, None);
            visitor.visit_expr_context(ctx);
        }
        ExprKind::Starred { value, ctx } => {
            visitor.visit_expr(value, None);
            visitor.visit_expr_context(ctx);
        }
        ExprKind::Name { ctx, .. } => {
            visitor.visit_expr_context(ctx);
        }
        ExprKind::List { elts, ctx } => {
            for expr in elts {
                visitor.visit_expr(expr, None);
            }
            visitor.visit_expr_context(ctx);
        }
        ExprKind::Tuple { elts, ctx } => {
            for expr in elts {
                visitor.visit_expr(expr, None);
            }
            visitor.visit_expr_context(ctx);
        }
        ExprKind::Slice { lower, upper, step } => {
            if let Some(expr) = lower {
                visitor.visit_expr(expr, None);
            }
            if let Some(expr) = upper {
                visitor.visit_expr(expr, None);
            }
            if let Some(expr) = step {
                visitor.visit_expr(expr, None);
            }
        }
    }
}

pub fn walk_constant<V: Visitor + ?Sized>(visitor: &mut V, constant: &Constant) {
    if let Constant::Tuple(constants) = constant {
        for constant in constants {
            visitor.visit_constant(constant)
        }
    }
}

pub fn walk_comprehension<V: Visitor + ?Sized>(visitor: &mut V, comprehension: &Comprehension) {
    visitor.visit_expr(&comprehension.target, None);
    visitor.visit_expr(&comprehension.iter, None);
    for expr in &comprehension.ifs {
        visitor.visit_expr(expr, None);
    }
}

pub fn walk_excepthandler<V: Visitor + ?Sized>(visitor: &mut V, excepthandler: &Excepthandler) {
    match &excepthandler.node {
        ExcepthandlerKind::ExceptHandler { type_, body, .. } => {
            if let Some(expr) = type_ {
                visitor.visit_expr(expr, None);
            }
            for stmt in body {
                visitor.visit_stmt(stmt);
            }
        }
    }
}

pub fn walk_arguments<V: Visitor + ?Sized>(visitor: &mut V, arguments: &Arguments) {
    for arg in &arguments.posonlyargs {
        visitor.visit_arg(arg);
    }
    for arg in &arguments.args {
        visitor.visit_arg(arg);
    }
    if let Some(arg) = &arguments.vararg {
        visitor.visit_arg(arg)
    }
    for arg in &arguments.kwonlyargs {
        visitor.visit_arg(arg);
    }
    for expr in &arguments.kw_defaults {
        visitor.visit_expr(expr, None)
    }
    if let Some(arg) = &arguments.kwarg {
        visitor.visit_arg(arg)
    }
    for expr in &arguments.defaults {
        visitor.visit_expr(expr, None)
    }
}

pub fn walk_arg<V: Visitor + ?Sized>(visitor: &mut V, arg: &Arg) {
    if let Some(expr) = &arg.node.annotation {
        visitor.visit_annotation(expr)
    }
}

pub fn walk_keyword<V: Visitor + ?Sized>(visitor: &mut V, keyword: &Keyword) {
    visitor.visit_expr(&keyword.node.value, None);
}

pub fn walk_withitem<V: Visitor + ?Sized>(visitor: &mut V, withitem: &Withitem) {
    visitor.visit_expr(&withitem.context_expr, None);
    if let Some(expr) = &withitem.optional_vars {
        visitor.visit_expr(expr, None);
    }
}

pub fn walk_match_case<V: Visitor + ?Sized>(visitor: &mut V, match_case: &MatchCase) {
    visitor.visit_pattern(&match_case.pattern);
    if let Some(expr) = &match_case.guard {
        visitor.visit_expr(expr, None);
    }
    for stmt in &match_case.body {
        visitor.visit_stmt(stmt);
    }
}

pub fn walk_pattern<V: Visitor + ?Sized>(visitor: &mut V, pattern: &Pattern) {
    match &pattern.node {
        PatternKind::MatchValue { value } => visitor.visit_expr(value, None),
        PatternKind::MatchSingleton { value } => visitor.visit_constant(value),
        PatternKind::MatchSequence { patterns } => {
            for pattern in patterns {
                visitor.visit_pattern(pattern)
            }
        }
        PatternKind::MatchMapping { keys, patterns, .. } => {
            for expr in keys {
                visitor.visit_expr(expr, None);
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
            visitor.visit_expr(cls, None);
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
                visitor.visit_pattern(pattern)
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
pub fn walk_expr_context<V: Visitor + ?Sized>(visitor: &mut V, expr_context: &ExprContext) {}

#[allow(unused_variables)]
#[inline(always)]
pub fn walk_boolop<V: Visitor + ?Sized>(visitor: &mut V, boolop: &Boolop) {}

#[allow(unused_variables)]
#[inline(always)]
pub fn walk_operator<V: Visitor + ?Sized>(visitor: &mut V, operator: &Operator) {}

#[allow(unused_variables)]
#[inline(always)]
pub fn walk_unaryop<V: Visitor + ?Sized>(visitor: &mut V, unaryop: &Unaryop) {}

#[allow(unused_variables)]
#[inline(always)]
pub fn walk_cmpop<V: Visitor + ?Sized>(visitor: &mut V, cmpop: &Cmpop) {}

#[allow(unused_variables)]
#[inline(always)]
pub fn walk_alias<V: Visitor + ?Sized>(visitor: &mut V, alias: &Alias) {}
