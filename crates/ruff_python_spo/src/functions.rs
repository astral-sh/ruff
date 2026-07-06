//! Method analysis: decorators → `@api.depends` args; body → reads / raises /
//! traverses.
//!
//! The body walk uses ruff's own [`Visitor`] (the existing mechanism) rather
//! than hand-rolling recursion. `Visitor` walks in evaluation order, so a
//! `for line in self.line_ids:` loop binds `line` (carrying its `line_ids`
//! relation prefix) *before* its body is visited — that's what lets
//! `line.amount` register as a read of `line_ids.amount`, preserving the
//! relation hop instead of collapsing it to a bare `amount`.

use std::collections::{HashMap, HashSet};

use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::{Expr, Stmt, StmtFunctionDef};

use crate::{RawMethod, expr_str};

/// Analyse a method into its decorator + body facts.
pub(crate) fn analyze_method(func: &StmtFunctionDef) -> RawMethod {
    let mut depends = Vec::new();
    let mut constrains = Vec::new();
    let mut onchange = Vec::new();
    for decorator in &func.decorator_list {
        if let Expr::Call(call) = &decorator.expression {
            let args = || call.arguments.args.iter().filter_map(expr_str);
            match terminal_name(&call.func) {
                Some("depends") => depends.extend(args()),
                Some("constrains") => constrains.extend(args()),
                Some("onchange") => onchange.extend(args()),
                _ => {}
            }
        }
    }

    let mut walker = BodyWalker::new();
    walker.visit_body(&func.body);

    RawMethod {
        name: func.name.id.as_str().to_string(),
        depends,
        constrains,
        onchange,
        reads: dedup(walker.reads),
        raises: dedup(walker.raises),
        traverses: dedup(walker.traverses),
    }
}

/// Walks a method body collecting record-variable field reads, raised
/// exception types, and relation traversals.
struct BodyWalker {
    /// Variables bound to a recordset, mapped to the **relation prefix** that
    /// reaches them: `self` → `""` (direct), and a loop variable from
    /// `for line in self.line_ids:` → `"line_ids"`, so `line.amount` reads
    /// `line_ids.amount`. Nested loops compose the prefix.
    record_vars: HashMap<String, String>,
    reads: Vec<String>,
    raises: Vec<String>,
    traverses: Vec<String>,
}

impl BodyWalker {
    fn new() -> Self {
        Self {
            record_vars: HashMap::from([("self".to_string(), String::new())]),
            reads: Vec::new(),
            raises: Vec::new(),
            traverses: Vec::new(),
        }
    }

    /// If `expr` is `<record-var>.<attr>`, the relation path it denotes
    /// (`self.line_ids` → `"line_ids"`; `line.tax_ids` with `line`→`line_ids`
    /// → `"line_ids.tax_ids"`). `None` if the base isn't a known record var.
    fn relation_path(&self, expr: &Expr) -> Option<String> {
        if let Expr::Attribute(attr) = expr
            && let Expr::Name(base) = &*attr.value
            && let Some(prefix) = self.record_vars.get(base.id.as_str())
        {
            return Some(join_path(prefix, attr.attr.id.as_str()));
        }
        None
    }
}

impl<'a> Visitor<'a> for BodyWalker {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::For(for_stmt) = stmt {
            // The relation prefix the loop variable inherits.
            let bind_prefix = if let Some(rel) = self.relation_path(&for_stmt.iter) {
                // `for line in self.line_ids:` — traverse + bind via the relation.
                self.traverses.push(rel.clone());
                Some(rel)
            } else if let Expr::Name(iter) = &*for_stmt.iter {
                // `for r in <record-var>:` — same prefix as the iterated var.
                self.record_vars.get(iter.id.as_str()).cloned()
            } else {
                None
            };
            if let Some(prefix) = bind_prefix
                && let Expr::Name(target) = &*for_stmt.target
            {
                self.record_vars
                    .insert(target.id.as_str().to_string(), prefix);
            }
        } else if let Stmt::Raise(raise) = stmt
            && let Some(exc) = &raise.exc
            && let Expr::Call(call) = &**exc
            && let Some(name) = terminal_name(&call.func)
        {
            self.raises.push(name.to_string());
        }
        walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        // Only *load* reads of `<record-var>.<attr>` count — a store target
        // (`self.total = ...`) is a write, not a read.
        if let Expr::Attribute(attr) = expr
            && attr.ctx.is_load()
            && let Expr::Name(base) = &*attr.value
            && let Some(prefix) = self.record_vars.get(base.id.as_str())
        {
            self.reads.push(join_path(prefix, attr.attr.id.as_str()));
        }
        walk_expr(self, expr);
    }
}

/// Join a relation prefix and a member into a dotted path:
/// `("line_ids", "amount")` → `"line_ids.amount"`; `("", "rounding")` →
/// `"rounding"`.
fn join_path(prefix: &str, member: &str) -> String {
    if prefix.is_empty() {
        member.to_string()
    } else {
        format!("{prefix}.{member}")
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
