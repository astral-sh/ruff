use rustpython_parser::ast::{
    Alias, Arg, Arguments, Boolop, Cmpop, Comprehension, Constant, Excepthandler,
    ExcepthandlerKind, Expr, ExprContext, ExprKind, Keyword, Operator, Stmt, StmtKind, Unaryop,
    Withitem,
};

pub trait Visitor {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        walk_stmt(self, stmt);
    }
    fn visit_expr(&mut self, expr: &Expr) {
        walk_expr(self, expr);
    }
    fn visit_ident(&mut self, ident: &str) {
        walk_ident(self, ident);
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
}

#[allow(unused_variables)]
pub fn walk_stmt<V: Visitor + ?Sized>(visitor: &mut V, stmt: &Stmt) {
    match &stmt.node {
        StmtKind::FunctionDef {
            name,
            args,
            body,
            decorator_list,
            returns,
            type_comment,
        } => {
            visitor.visit_ident(name);
            visitor.visit_arguments(args);
            for stmt in body {
                visitor.visit_stmt(stmt)
            }
            for expr in decorator_list {
                visitor.visit_expr(expr)
            }
            for expr in returns {
                visitor.visit_expr(expr);
            }
        }
        StmtKind::AsyncFunctionDef {
            name,
            args,
            body,
            decorator_list,
            returns,
            type_comment,
        } => {
            visitor.visit_ident(name);
            visitor.visit_arguments(args);
            for stmt in body {
                visitor.visit_stmt(stmt)
            }
            for expr in decorator_list {
                visitor.visit_expr(expr)
            }
            for expr in returns {
                visitor.visit_expr(expr);
            }
        }
        StmtKind::ClassDef {
            name,
            bases,
            keywords,
            body,
            decorator_list,
        } => {
            visitor.visit_ident(name);
            for expr in bases {
                visitor.visit_expr(expr)
            }
            for keyword in keywords {
                visitor.visit_keyword(keyword)
            }
            for stmt in body {
                visitor.visit_stmt(stmt)
            }
            for expr in decorator_list {
                visitor.visit_expr(expr)
            }
        }
        StmtKind::Return { value } => {
            if let Some(expr) = value {
                visitor.visit_expr(expr)
            }
        }
        StmtKind::Delete { targets } => {
            for expr in targets {
                visitor.visit_expr(expr)
            }
        }
        StmtKind::Assign {
            targets,
            value,
            type_comment,
        } => {
            for expr in targets {
                visitor.visit_expr(expr)
            }
            visitor.visit_expr(value)
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
            simple,
        } => {
            visitor.visit_expr(target);
            visitor.visit_expr(annotation);
            if let Some(expr) = value {
                visitor.visit_expr(expr)
            }
        }
        StmtKind::For {
            target,
            iter,
            body,
            orelse,
            type_comment,
        } => {
            visitor.visit_expr(target);
            visitor.visit_expr(iter);
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
            type_comment,
        } => {
            visitor.visit_expr(target);
            visitor.visit_expr(iter);
            for stmt in body {
                visitor.visit_stmt(stmt)
            }
            for stmt in orelse {
                visitor.visit_stmt(stmt)
            }
        }
        StmtKind::While { test, body, orelse } => {
            visitor.visit_expr(test);
            for stmt in body {
                visitor.visit_stmt(stmt)
            }
            for stmt in orelse {
                visitor.visit_stmt(stmt)
            }
        }
        StmtKind::If { test, body, orelse } => {
            visitor.visit_expr(test);
            for stmt in body {
                visitor.visit_stmt(stmt)
            }
            for stmt in orelse {
                visitor.visit_stmt(stmt)
            }
        }
        StmtKind::With {
            items,
            body,
            type_comment,
        } => {
            for withitem in items {
                visitor.visit_withitem(withitem);
            }
            for stmt in body {
                visitor.visit_stmt(stmt)
            }
        }
        StmtKind::AsyncWith {
            items,
            body,
            type_comment,
        } => {
            for withitem in items {
                visitor.visit_withitem(withitem);
            }
            for stmt in body {
                visitor.visit_stmt(stmt)
            }
        }
        StmtKind::Raise { exc, cause } => {
            if let Some(expr) = exc {
                visitor.visit_expr(expr)
            };
            if let Some(expr) = cause {
                visitor.visit_expr(expr)
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
            visitor.visit_expr(test);
            if let Some(expr) = msg {
                visitor.visit_expr(expr)
            }
        }
        StmtKind::Import { names } => {
            for alias in names {
                visitor.visit_alias(alias);
            }
        }
        StmtKind::ImportFrom {
            module,
            names,
            level,
        } => {
            for alias in names {
                visitor.visit_alias(alias);
            }
        }
        StmtKind::Global { names } => {
            for ident in names {
                visitor.visit_ident(ident)
            }
        }
        StmtKind::Nonlocal { names } => {
            for ident in names {
                visitor.visit_ident(ident)
            }
        }
        StmtKind::Expr { value } => visitor.visit_expr(value),
        StmtKind::Pass => {}
        StmtKind::Break => {}
        StmtKind::Continue => {}
    }
}

#[allow(unused_variables)]
pub fn walk_expr<V: Visitor + ?Sized>(visitor: &mut V, expr: &Expr) {
    match &expr.node {
        ExprKind::BoolOp { op, values } => {
            visitor.visit_boolop(op);
            for expr in values {
                visitor.visit_expr(expr)
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
            for expr in keys.iter().flatten() {
                visitor.visit_expr(expr)
            }
            for expr in values {
                visitor.visit_expr(expr)
            }
        }
        ExprKind::Set { elts } => {
            for expr in elts {
                visitor.visit_expr(expr)
            }
        }
        ExprKind::ListComp { elt, generators } => {
            visitor.visit_expr(elt);
            for comprehension in generators {
                visitor.visit_comprehension(comprehension)
            }
        }
        ExprKind::SetComp { elt, generators } => {
            visitor.visit_expr(elt);
            for comprehension in generators {
                visitor.visit_comprehension(comprehension)
            }
        }
        ExprKind::DictComp {
            key,
            value,
            generators,
        } => {
            visitor.visit_expr(key);
            visitor.visit_expr(value);
            for comprehension in generators {
                visitor.visit_comprehension(comprehension)
            }
        }
        ExprKind::GeneratorExp { elt, generators } => {
            visitor.visit_expr(elt);
            for comprehension in generators {
                visitor.visit_comprehension(comprehension)
            }
        }
        ExprKind::Await { value } => visitor.visit_expr(value),
        ExprKind::Yield { value } => {
            if let Some(expr) = value {
                visitor.visit_expr(expr)
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
                visitor.visit_expr(expr)
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
            value,
            conversion,
            format_spec,
        } => {
            visitor.visit_expr(value);
            if let Some(expr) = format_spec {
                visitor.visit_expr(expr)
            }
        }
        ExprKind::JoinedStr { values } => {
            for expr in values {
                visitor.visit_expr(expr)
            }
        }
        ExprKind::Constant { value, kind } => visitor.visit_constant(value),
        ExprKind::Attribute { value, attr, ctx } => {
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
        ExprKind::Name { id, ctx } => {
            visitor.visit_ident(id);
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

#[allow(unused_variables)]
pub fn walk_constant<V: Visitor + ?Sized>(visitor: &mut V, constant: &Constant) {
    if let Constant::Tuple(constants) = constant {
        for constant in constants {
            visitor.visit_constant(constant)
        }
    }
}

#[allow(unused_variables)]
pub fn walk_comprehension<V: Visitor + ?Sized>(visitor: &mut V, comprehension: &Comprehension) {
    visitor.visit_expr(&comprehension.target);
    visitor.visit_expr(&comprehension.iter);
    for expr in &comprehension.ifs {
        visitor.visit_expr(expr);
    }
}

#[allow(unused_variables)]
pub fn walk_excepthandler<V: Visitor + ?Sized>(visitor: &mut V, excepthandler: &Excepthandler) {
    match &excepthandler.node {
        ExcepthandlerKind::ExceptHandler { type_, name, body } => {
            if let Some(expr) = type_ {
                visitor.visit_expr(expr)
            }
            for stmt in body {
                visitor.visit_stmt(stmt)
            }
        }
    }
}

#[allow(unused_variables)]
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
    for expr in arguments.kw_defaults.iter().flatten() {
        visitor.visit_expr(expr)
    }
    if let Some(arg) = &arguments.kwarg {
        visitor.visit_arg(arg)
    }
    for expr in &arguments.defaults {
        visitor.visit_expr(expr)
    }
}

#[allow(unused_variables)]
pub fn walk_arg<V: Visitor + ?Sized>(visitor: &mut V, arg: &Arg) {
    if let Some(expr) = &arg.node.annotation {
        visitor.visit_expr(expr)
    }
}

#[allow(unused_variables)]
pub fn walk_keyword<V: Visitor + ?Sized>(visitor: &mut V, keyword: &Keyword) {
    visitor.visit_expr(&keyword.node.value);
}

#[allow(unused_variables)]
pub fn walk_withitem<V: Visitor + ?Sized>(visitor: &mut V, withitem: &Withitem) {
    visitor.visit_expr(&withitem.context_expr);
    if let Some(expr) = &withitem.optional_vars {
        visitor.visit_expr(expr);
    }
}

#[allow(unused_variables)]
pub fn walk_ident<V: Visitor + ?Sized>(visitor: &mut V, ident: &str) {}

#[allow(unused_variables)]
pub fn walk_expr_context<V: Visitor + ?Sized>(visitor: &mut V, expr_context: &ExprContext) {}

#[allow(unused_variables)]
pub fn walk_boolop<V: Visitor + ?Sized>(visitor: &mut V, boolop: &Boolop) {}

#[allow(unused_variables)]
pub fn walk_operator<V: Visitor + ?Sized>(visitor: &mut V, operator: &Operator) {}

#[allow(unused_variables)]
pub fn walk_unaryop<V: Visitor + ?Sized>(visitor: &mut V, unaryop: &Unaryop) {}

#[allow(unused_variables)]
pub fn walk_cmpop<V: Visitor + ?Sized>(visitor: &mut V, cmpop: &Cmpop) {}

#[allow(unused_variables)]
pub fn walk_alias<V: Visitor + ?Sized>(visitor: &mut V, alias: &Alias) {}
