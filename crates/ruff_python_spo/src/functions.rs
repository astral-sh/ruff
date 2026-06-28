//! Method analysis: decorators → `@api.depends` args; body → reads / raises /
//! traverses.
//!
//! The body walk uses ruff's own [`Visitor`] (the existing mechanism) rather
//! than hand-rolling recursion. `Visitor` walks in evaluation order, so a
//! `for record in self:` loop binds `record` as a record-variable *before* its
//! body is visited — that's what lets `record.attr` register as a field read.

use std::collections::HashSet;

use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::{Expr, Stmt, StmtFunctionDef};

use crate::{RawMethod, expr_str};

/// Analyse a method into its decorator + body facts.
pub(crate) fn analyze_method(func: &StmtFunctionDef) -> RawMethod {
    let mut depends = Vec::new();
    for decorator in &func.decorator_list {
        if let Expr::Call(call) = &decorator.expression
            && terminal_name(&call.func) == Some("depends")
        {
            depends.extend(call.arguments.args.iter().filter_map(expr_str));
        }
    }

    let mut walker = BodyWalker::new();
    walker.visit_body(&func.body);

    RawMethod {
        name: func.name.id.as_str().to_string(),
        depends,
        reads: dedup(walker.reads),
        raises: dedup(walker.raises),
        traverses: dedup(walker.traverses),
    }
}

/// Walks a method body collecting record-variable attribute reads, raised
/// exception types, and `self.<rel>` loop traversals.
struct BodyWalker {
    /// Variables bound to a recordset: `self` plus loop variables iterating
    /// over `self` or `self.<rel>`.
    record_vars: HashSet<String>,
    reads: Vec<String>,
    raises: Vec<String>,
    traverses: Vec<String>,
}

impl BodyWalker {
    fn new() -> Self {
        Self {
            record_vars: HashSet::from(["self".to_string()]),
            reads: Vec::new(),
            raises: Vec::new(),
            traverses: Vec::new(),
        }
    }

    /// Whether `expr` iterates a recordset (`self`, a known record var, or
    /// `self.<rel>`) — i.e. its loop variable is itself a record.
    fn iterates_record(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Name(n) => self.record_vars.contains(n.id.as_str()),
            _ => self_attr(expr).is_some(),
        }
    }
}

impl<'a> Visitor<'a> for BodyWalker {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::For(for_stmt) => {
                if let Some(rel) = self_attr(&for_stmt.iter) {
                    self.traverses.push(rel);
                }
                if self.iterates_record(&for_stmt.iter)
                    && let Expr::Name(target) = &*for_stmt.target
                {
                    self.record_vars.insert(target.id.as_str().to_string());
                }
                walk_stmt(self, stmt);
            }
            Stmt::Raise(raise) => {
                if let Some(exc) = &raise.exc
                    && let Expr::Call(call) = &**exc
                    && let Some(name) = terminal_name(&call.func)
                {
                    self.raises.push(name.to_string());
                }
                walk_stmt(self, stmt);
            }
            _ => walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Attribute(attr) = expr
            && let Expr::Name(base) = &*attr.value
            && self.record_vars.contains(base.id.as_str())
        {
            self.reads.push(attr.attr.id.as_str().to_string());
        }
        walk_expr(self, expr);
    }
}

/// `self.<attr>` → `Some("<attr>")`, else `None`.
fn self_attr(expr: &Expr) -> Option<String> {
    let Expr::Attribute(attr) = expr else {
        return None;
    };
    match &*attr.value {
        Expr::Name(n) if n.id.as_str() == "self" => Some(attr.attr.id.as_str().to_string()),
        _ => None,
    }
}

/// The terminal identifier of a callee expression: `f` for `f(...)`,
/// `attr` for `a.b.attr(...)`. Used for decorator names and raised types.
fn terminal_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Name(n) => Some(n.id.as_str()),
        Expr::Attribute(a) => Some(a.attr.id.as_str()),
        _ => None,
    }
}

/// Order-preserving de-duplication.
fn dedup(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|v| seen.insert(v.clone()))
        .collect()
}
