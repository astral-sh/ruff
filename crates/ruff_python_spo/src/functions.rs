//! Method analysis: decorators → `@api.depends` args; body → reads / raises /
//! traverses / `writes` / `guarded_writes` / `calls` (the DTO-arm quartet,
//! see `.claude/knowledge/fuzzy-recipe-codebook.md` §2).
//!
//! The body walk uses ruff's own [`Visitor`] (the existing mechanism) rather
//! than hand-rolling recursion. `Visitor` walks in evaluation order, so a
//! `for line in self.line_ids:` loop binds `line` (carrying its `line_ids`
//! relation prefix) *before* its body is visited — that's what lets
//! `line.amount` register as a read of `line_ids.amount`, preserving the
//! relation hop instead of collapsing it to a bare `amount`.
//!
//! # The DTO-arm quartet (populated here; mirrors `ruff_ruby_spo`)
//!
//! - **`writes`** (Authoritative) — `self.<f> = …` / `<record-var>.<f> = …`
//!   store targets, the field rendered through the SAME relation-prefix
//!   machinery `reads` uses (`move.amount_total` inside `for move in self:`
//!   writes `amount_total`, not `move.amount_total` — `move` IS `self`, prefix
//!   `""`). A plain local assignment (`total = 0`, target is a bare `Name`
//!   with no record-var binding) is deliberately NOT a write.
//! - **`guarded_writes`** (Authoritative, always ⊆ `writes`) — the J1 fact.
//!   Three Python spellings are covered (mirrors ruby's blank-guard +
//!   `OrAsgn` arms):
//!   1. `if not self.x: self.x = v` — `Expr::UnaryOp(Not)` guard, the write
//!      is a DIRECT statement in the `if`-body (no dominator analysis — a
//!      write nested under a further conditional is NOT claimed, same as
//!      ruby's `claim_if_writes`).
//!   2. `if self.x is None: self.x = v` — `Expr::Compare([Is], [None])` guard.
//!   3. `self.x = self.x or v` — an `Expr::BoolOp(Or)` RHS whose first
//!      operand reads the SAME field the assignment writes (the Python
//!      spelling of ruby's `self.x ||= v` `OrAsgn` idiom).
//!
//!   NOT covered (honest gaps, not silently dropped): `unless`-style
//!   `if self.x: pass else: self.x = v`, `elif` chains, `x == False` guards,
//!   `getattr`/`setattr` forms.
//! - **`calls`** (Inferred) — a dispatch of one of the closed
//!   [`ORM_MUTATORS`] verbs, recorded as `"<receiver>.<method>"`. The
//!   receiver is rendered by [`BodyWalker::receiver_label`]: a bound
//!   record-var (including bare `self`) renders as its relation path: e.g.
//!   `self.line_ids.unlink()` → `"line_ids.unlink"`.
//! - **helpers** — N/A for this frontend. Odoo/Python has no method
//!   visibility keyword the way Ruby has `private`/`protected`; every
//!   `Stmt::FunctionDef` in a model class body already lands in
//!   `RawClass::methods` → `Model::functions` unconditionally
//!   (`walk.rs::walk_class`, the `Stmt::FunctionDef(func) => …` arm has no
//!   filter). There is no non-routable subset to split off, so there is no
//!   `helpers` vec to thread through here — see
//!   `.claude/knowledge/fuzzy-recipe-codebook.md` §2's coverage table.

use std::collections::{HashMap, HashSet};

use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::{BoolOp, CmpOp, Expr, Stmt, StmtFunctionDef, UnaryOp};

use crate::{RawMethod, expr_str};

/// The closed set of Odoo ORM lifecycle mutators. A dispatch of one of these
/// marks a method as a *command* (mutates persistent state) rather than a
/// *query* — the command-shape counterpart of `reads`/`traverses`. Mirrors
/// ruby's `AR_MUTATORS`: deliberately narrow (not every call, just the ORM's
/// own mutator vocabulary) per `.claude/knowledge/fuzzy-recipe-codebook.md`
/// §2.
const ORM_MUTATORS: &[&str] = &["create", "write", "unlink", "update", "copy", "flush_recordset"];

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
        writes: dedup(walker.writes),
        guarded_writes: dedup(walker.guarded_writes),
        calls: dedup(walker.calls),
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
    /// Own-field store targets (`self.<f> = …` / `<record-var>.<f> = …`).
    /// Authoritative; command-shape counterpart of `reads`.
    writes: Vec<String>,
    /// The J1 subset of `writes` guarded by a blank/`None` test (or an
    /// `x or default` RHS) on the same field. Always ⊆ `writes`.
    guarded_writes: Vec<String>,
    /// Closed-set ORM-mutator dispatches, as `"<receiver>.<method>"`.
    calls: Vec<String>,
}

impl BodyWalker {
    fn new() -> Self {
        Self {
            record_vars: HashMap::from([("self".to_string(), String::new())]),
            reads: Vec::new(),
            raises: Vec::new(),
            traverses: Vec::new(),
            writes: Vec::new(),
            guarded_writes: Vec::new(),
            calls: Vec::new(),
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

    /// Best-effort label for a call receiver, mirroring ruby's
    /// `receiver_label`: a bound record-var (including bare `self`) renders
    /// as its relation path (`""` prefix → `"self"`); an unresolved local
    /// renders as its own identifier; anything else is `"<expr>"`.
    fn receiver_label(&self, expr: &Expr) -> String {
        if let Expr::Name(name) = expr
            && let Some(prefix) = self.record_vars.get(name.id.as_str())
        {
            return if prefix.is_empty() {
                "self".to_string()
            } else {
                prefix.clone()
            };
        }
        if let Some(path) = self.relation_path(expr) {
            return path;
        }
        if let Expr::Name(name) = expr {
            return name.id.to_string();
        }
        "<expr>".to_string()
    }

    /// `self.x = self.x or v` (or a bound record-var's field) — the Python
    /// spelling of ruby's `self.x ||= v` `OrAsgn` default idiom: an
    /// `Expr::BoolOp(Or)` RHS whose first operand reads the same field the
    /// assignment writes.
    fn is_or_guarded_default(&self, field: &str, value: &Expr) -> bool {
        if let Expr::BoolOp(b) = value
            && b.op == BoolOp::Or
            && let Some(first) = b.values.first()
        {
            return self.relation_path(first).as_deref() == Some(field);
        }
        false
    }

    /// `not self.x` / `self.x is None` (or a bound record-var's field) → the
    /// guarded field path. Mirrors ruby's `blank_guarded_field` /
    /// `present_guarded_field`; see the module doc for the spellings NOT
    /// covered.
    fn blank_guarded_field(&self, test: &Expr) -> Option<String> {
        match test {
            Expr::UnaryOp(u) if u.op == UnaryOp::Not => self.relation_path(&u.operand),
            Expr::Compare(c)
                if matches!(&*c.ops, [CmpOp::Is])
                    && matches!(c.comparators.first(), Some(Expr::NoneLiteral(_))) =>
            {
                self.relation_path(&c.left)
            }
            _ => None,
        }
    }

    /// If any TOP-LEVEL statement in `body` directly assigns `field` (no
    /// recursion into nested compound statements — mirrors ruby's
    /// local-only `claim_if_writes`), record it as a guarded (default)
    /// write. The write itself is captured separately by the generic
    /// `Stmt::Assign` handling in [`Visitor::visit_stmt`] when the nested
    /// statement is walked.
    fn claim_guarded_write(&mut self, body: &[Stmt], field: &str) {
        let guarded = body.iter().any(|stmt| {
            if let Stmt::Assign(assign) = stmt {
                assign
                    .targets
                    .iter()
                    .any(|t| self.relation_path(t).as_deref() == Some(field))
            } else {
                false
            }
        });
        if guarded {
            self.guarded_writes.push(field.to_string());
        }
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
        } else if let Stmt::Assign(assign) = stmt {
            // `<record-var>.<f> = …` — a write; local assignment (target is
            // a bare `Name`, no record-var binding) is NOT a write.
            for target in &assign.targets {
                if let Some(field) = self.relation_path(target) {
                    self.writes.push(field.clone());
                    if self.is_or_guarded_default(&field, &assign.value) {
                        self.guarded_writes.push(field);
                    }
                }
            }
        } else if let Stmt::If(if_stmt) = stmt
            && let Some(field) = self.blank_guarded_field(&if_stmt.test)
        {
            // J1: `if not self.x: self.x = v` / `if self.x is None: self.x = v`.
            self.claim_guarded_write(&if_stmt.body, &field);
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
        } else if let Expr::Call(call) = expr
            && let Expr::Attribute(attr) = &*call.func
            && ORM_MUTATORS.contains(&attr.attr.id.as_str())
        {
            // Closed-set ORM-mutator dispatch: `self.line_ids.unlink()` →
            // `"line_ids.unlink"`.
            let receiver = self.receiver_label(&attr.value);
            self.calls
                .push(format!("{receiver}.{}", attr.attr.id.as_str()));
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
