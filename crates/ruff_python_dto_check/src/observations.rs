//! Per-family comparison computations (set algebra + distributions).
//!
//! # AST hash algorithm
//!
//! Post-order walk of the function body (a `Suite`), emitting one token per
//! node of the form `<NodeKind>(<child-count>)`. SHA-256 of the concatenated
//! token string becomes `ast_hash_self`. This is deterministic, captures
//! structural shape, and does not depend on identifier names or whitespace.
//!
//! # Body line count
//!
//! Defined as `line_end - line_start + 1` (inclusive, 1-based).
//!
//! # Percentile algorithm
//!
//! Linear interpolation between order statistics; nearest-rank if family
//! size < 4.

use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};

use ruff_python_ast::{
    Expr, ExprContext, Stmt, StmtFunctionDef,
};
use ruff_python_parser::parse_module;

use crate::bundle::{ComparisonWithinFamily, Distribution, EmittedBundle};

/// Compute and attach `comparison_within_family` blocks to all bundles
/// in each family. Modifies `family_map` in place.
pub fn attach_observations(
    family_map: &mut BTreeMap<String, Vec<EmittedBundle>>,
    source_map: &BTreeMap<String, String>,
) {
    for (family, bundles) in family_map.iter_mut() {
        let observations = compute_family_observations(family, bundles, source_map);
        for (bundle, obs) in bundles.iter_mut().zip(observations.into_iter()) {
            bundle.comparison_within_family = Some(obs);
        }
    }
}

fn compute_family_observations(
    family: &str,
    bundles: &[EmittedBundle],
    source_map: &BTreeMap<String, String>,
) -> Vec<ComparisonWithinFamily> {
    let family_size = bundles.len();

    // Compute decorator sets and body line counts for each bundle.
    let dec_sets: Vec<BTreeSet<String>> = bundles
        .iter()
        .map(|b| b.all_decorators.iter().cloned().collect())
        .collect();

    let body_lines: Vec<u64> = bundles.iter().map(|b| u64::from(b.body_lines)).collect();

    // Family decorator intersection: decorators present in ALL functions.
    let family_intersection: BTreeSet<String> = if dec_sets.is_empty() {
        BTreeSet::new()
    } else {
        dec_sets[1..].iter().fold(dec_sets[0].clone(), |acc, s| {
            acc.intersection(s).cloned().collect()
        })
    };

    let body_dist = percentile_distribution(&body_lines);

    // Compute param counts.
    let param_counts: Vec<u64> = bundles
        .iter()
        .map(|b| compute_param_count(b, source_map))
        .collect();
    let param_dist = percentile_distribution(&param_counts);

    // Compute AST hashes.
    let ast_hashes: Vec<String> = bundles
        .iter()
        .map(|b| compute_ast_hash(b, source_map))
        .collect();

    // Build a map from hash → function names (for collision detection).
    let mut hash_to_names: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (bundle, hash) in bundles.iter().zip(ast_hashes.iter()) {
        hash_to_names
            .entry(hash.clone())
            .or_default()
            .push(bundle.function_name.clone());
    }

    bundles
        .iter()
        .zip(dec_sets.iter())
        .zip(ast_hashes.iter())
        .enumerate()
        .map(|(i, ((bundle, dec_set), hash))| {
            let self_minus_family: Vec<String> = dec_set
                .difference(&family_intersection)
                .cloned()
                .collect();
            let family_minus_self: Vec<String> = family_intersection
                .difference(dec_set)
                .cloned()
                .collect();

            let collisions: Vec<String> = hash_to_names
                .get(hash.as_str())
                .map(|names| {
                    names
                        .iter()
                        .filter(|n| n.as_str() != bundle.function_name)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();

            ComparisonWithinFamily {
                family: family.to_string(),
                family_size,
                decorators_family_intersection: family_intersection.iter().cloned().collect(),
                self_minus_family_intersection: self_minus_family,
                family_intersection_minus_self: family_minus_self,
                body_lines_self: bundle.body_lines,
                body_lines_family_distribution: body_dist.clone(),
                param_count_self: param_counts[i] as usize,
                param_count_family_distribution: param_dist.clone(),
                ast_hash_self: format!("sha256:{hash}"),
                ast_hash_family_collisions: collisions,
            }
        })
        .collect()
}

/// Compute param count for a bundle by reparsing its source file.
fn compute_param_count(bundle: &EmittedBundle, source_map: &BTreeMap<String, String>) -> u64 {
    let Some(source) = source_map.get(&bundle.file) else {
        return 0;
    };
    let Ok(parsed) = parse_module(source) else {
        return 0;
    };
    for stmt in &parsed.syntax().body {
        if let Stmt::FunctionDef(func) = stmt
            && func.name.id.as_str() == bundle.function_name
        {
            let count = func.parameters.args.len()
                + func.parameters.posonlyargs.len()
                + usize::from(func.parameters.vararg.is_some())
                + usize::from(func.parameters.kwarg.is_some())
                + func.parameters.kwonlyargs.len();
            return count as u64;
        }
    }
    0
}

/// Post-order walk of a function's body statements, emitting one token per
/// node: `<NodeKind>(<child-count>)`. SHA-256 of the concatenated tokens.
fn compute_ast_hash(bundle: &EmittedBundle, source_map: &BTreeMap<String, String>) -> String {
    let Some(source) = source_map.get(&bundle.file) else {
        return hex_sha256(b"");
    };
    let Ok(parsed) = parse_module(source) else {
        return hex_sha256(b"");
    };

    let mut tokens = String::new();
    for stmt in &parsed.syntax().body {
        if let Stmt::FunctionDef(func) = stmt
            && func.name.id.as_str() == bundle.function_name
        {
            for s in &func.body {
                walk_stmt(s, &mut tokens);
            }
            break;
        }
    }
    hex_sha256(tokens.as_bytes())
}

fn hex_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

// ---------------------------------------------------------------------------
// AST post-order walker — emits structural tokens
// ---------------------------------------------------------------------------

fn walk_stmt(stmt: &Stmt, out: &mut String) {
    match stmt {
        Stmt::FunctionDef(f) => {
            for s in &f.body {
                walk_stmt(s, out);
            }
            out.push_str(&format!("FunctionDef({})", f.body.len()));
        }
        Stmt::Return(r) => {
            let child = usize::from(r.value.is_some());
            if let Some(v) = &r.value {
                walk_expr(v, out);
            }
            out.push_str(&format!("Return({child})"));
        }
        Stmt::Assign(a) => {
            for t in &a.targets {
                walk_expr(t, out);
            }
            walk_expr(&a.value, out);
            out.push_str(&format!("Assign({})", a.targets.len() + 1));
        }
        Stmt::AugAssign(a) => {
            walk_expr(&a.target, out);
            walk_expr(&a.value, out);
            out.push_str("AugAssign(2)");
        }
        Stmt::AnnAssign(a) => {
            walk_expr(&a.target, out);
            walk_expr(&a.annotation, out);
            let v = usize::from(a.value.is_some());
            if let Some(val) = &a.value {
                walk_expr(val, out);
            }
            out.push_str(&format!("AnnAssign({})", 2 + v));
        }
        Stmt::Expr(e) => {
            walk_expr(&e.value, out);
            out.push_str("Expr(1)");
        }
        Stmt::If(i) => {
            walk_expr(&i.test, out);
            for s in &i.body {
                walk_stmt(s, out);
            }
            for clause in &i.elif_else_clauses {
                if let Some(test) = &clause.test {
                    walk_expr(test, out);
                }
                for s in &clause.body {
                    walk_stmt(s, out);
                }
            }
            out.push_str(&format!("If({})", 1 + i.body.len() + i.elif_else_clauses.len()));
        }
        Stmt::For(f) => {
            walk_expr(&f.target, out);
            walk_expr(&f.iter, out);
            for s in &f.body {
                walk_stmt(s, out);
            }
            out.push_str(&format!("For({})", 2 + f.body.len()));
        }
        Stmt::While(w) => {
            walk_expr(&w.test, out);
            for s in &w.body {
                walk_stmt(s, out);
            }
            out.push_str(&format!("While({})", 1 + w.body.len()));
        }
        Stmt::With(w) => {
            for s in &w.body {
                walk_stmt(s, out);
            }
            out.push_str(&format!("With({})", w.body.len()));
        }
        Stmt::Try(t) => {
            for s in &t.body {
                walk_stmt(s, out);
            }
            out.push_str(&format!("Try({})", t.body.len()));
        }
        Stmt::Raise(r) => {
            let child = usize::from(r.exc.is_some());
            if let Some(exc) = &r.exc {
                walk_expr(exc, out);
            }
            out.push_str(&format!("Raise({child})"));
        }
        Stmt::Delete(d) => {
            for t in &d.targets {
                walk_expr(t, out);
            }
            out.push_str(&format!("Delete({})", d.targets.len()));
        }
        Stmt::Pass(_) => out.push_str("Pass(0)"),
        Stmt::Break(_) => out.push_str("Break(0)"),
        Stmt::Continue(_) => out.push_str("Continue(0)"),
        Stmt::Import(i) => {
            out.push_str(&format!("Import({})", i.names.len()));
        }
        Stmt::ImportFrom(i) => {
            out.push_str(&format!("ImportFrom({})", i.names.len()));
        }
        Stmt::Global(g) => {
            out.push_str(&format!("Global({})", g.names.len()));
        }
        Stmt::Nonlocal(n) => {
            out.push_str(&format!("Nonlocal({})", n.names.len()));
        }
        Stmt::ClassDef(c) => {
            for s in &c.body {
                walk_stmt(s, out);
            }
            out.push_str(&format!("ClassDef({})", c.body.len()));
        }
        Stmt::TypeAlias(_) => out.push_str("TypeAlias(0)"),
        Stmt::Match(m) => {
            walk_expr(&m.subject, out);
            out.push_str(&format!("Match({})", 1 + m.cases.len()));
        }
        Stmt::IpyEscapeCommand(_) => out.push_str("IpyEscapeCommand(0)"),
        Stmt::Assert(a) => {
            walk_expr(&a.test, out);
            if let Some(msg) = &a.msg {
                walk_expr(msg, out);
            }
            let n = 1 + usize::from(a.msg.is_some());
            out.push_str(&format!("Assert({n})"));
        }
    }
}

fn walk_expr(expr: &Expr, out: &mut String) {
    match expr {
        Expr::Name(n) => {
            // Include load/store/del context to distinguish assignment targets.
            let ctx = match n.ctx {
                ExprContext::Load => "Load",
                ExprContext::Store => "Store",
                ExprContext::Del => "Del",
                ExprContext::Invalid => "Invalid",
            };
            out.push_str(&format!("Name({ctx})"));
        }
        Expr::Attribute(a) => {
            walk_expr(&a.value, out);
            out.push_str("Attribute(1)");
        }
        Expr::Call(c) => {
            walk_expr(&c.func, out);
            let n = c.arguments.args.len() + c.arguments.keywords.len();
            out.push_str(&format!("Call({n})"));
        }
        Expr::BinOp(b) => {
            walk_expr(&b.left, out);
            walk_expr(&b.right, out);
            out.push_str("BinOp(2)");
        }
        Expr::UnaryOp(u) => {
            walk_expr(&u.operand, out);
            out.push_str("UnaryOp(1)");
        }
        Expr::BoolOp(b) => {
            for v in &b.values {
                walk_expr(v, out);
            }
            out.push_str(&format!("BoolOp({})", b.values.len()));
        }
        Expr::Compare(c) => {
            walk_expr(&c.left, out);
            for comp in &c.comparators {
                walk_expr(comp, out);
            }
            out.push_str(&format!("Compare({})", 1 + c.comparators.len()));
        }
        Expr::If(i) => {
            walk_expr(&i.test, out);
            walk_expr(&i.body, out);
            walk_expr(&i.orelse, out);
            out.push_str("IfExp(3)");
        }
        Expr::Subscript(s) => {
            walk_expr(&s.value, out);
            walk_expr(&s.slice, out);
            out.push_str("Subscript(2)");
        }
        Expr::Tuple(t) => {
            for e in &t.elts {
                walk_expr(e, out);
            }
            out.push_str(&format!("Tuple({})", t.elts.len()));
        }
        Expr::List(l) => {
            for e in &l.elts {
                walk_expr(e, out);
            }
            out.push_str(&format!("List({})", l.elts.len()));
        }
        Expr::Dict(d) => {
            out.push_str(&format!("Dict({})", d.items.len()));
        }
        Expr::Set(s) => {
            out.push_str(&format!("Set({})", s.elts.len()));
        }
        Expr::StringLiteral(_) => out.push_str("Str(0)"),
        Expr::BytesLiteral(_) => out.push_str("Bytes(0)"),
        Expr::NumberLiteral(_) => out.push_str("Num(0)"),
        Expr::BooleanLiteral(_) => out.push_str("Bool(0)"),
        Expr::NoneLiteral(_) => out.push_str("None(0)"),
        Expr::EllipsisLiteral(_) => out.push_str("Ellipsis(0)"),
        Expr::FString(_) => out.push_str("FString(0)"),
        Expr::Lambda(l) => {
            walk_expr(&l.body, out);
            out.push_str("Lambda(1)");
        }
        Expr::ListComp(_) => out.push_str("ListComp(0)"),
        Expr::SetComp(_) => out.push_str("SetComp(0)"),
        Expr::DictComp(_) => out.push_str("DictComp(0)"),
        Expr::Generator(_) => out.push_str("Generator(0)"),
        Expr::Await(a) => {
            walk_expr(&a.value, out);
            out.push_str("Await(1)");
        }
        Expr::Yield(y) => {
            if let Some(v) = &y.value {
                walk_expr(v, out);
                out.push_str("Yield(1)");
            } else {
                out.push_str("Yield(0)");
            }
        }
        Expr::YieldFrom(y) => {
            walk_expr(&y.value, out);
            out.push_str("YieldFrom(1)");
        }
        Expr::Starred(s) => {
            walk_expr(&s.value, out);
            out.push_str("Starred(1)");
        }
        Expr::Named(n) => {
            walk_expr(&n.target, out);
            walk_expr(&n.value, out);
            out.push_str("Named(2)");
        }
        Expr::IpyEscapeCommand(_) => out.push_str("IpyEscape(0)"),
        Expr::TString(t) => {
            // Template strings (PEP 750) — emit token + part count.
            // Detailed traversal would require ruff_python_ast::TStringPart;
            // structural token is enough for the AST-hash heuristic.
            let n = t.value.iter().count();
            out.push_str(&format!("TString({n})"));
        }
        Expr::Slice(s) => {
            let mut n = 0usize;
            if let Some(lower) = &s.lower {
                walk_expr(lower, out);
                n += 1;
            }
            if let Some(upper) = &s.upper {
                walk_expr(upper, out);
                n += 1;
            }
            if let Some(step) = &s.step {
                walk_expr(step, out);
                n += 1;
            }
            out.push_str(&format!("Slice({n})"));
        }
    }
}

/// Compute percentile distribution using linear interpolation between
/// order statistics; nearest-rank if `values.len() < 4`.
pub fn percentile_distribution(values: &[u64]) -> Distribution {
    if values.is_empty() {
        return Distribution { p25: 0, p50: 0, p75: 0, p95: 0, p99: 0, max: 0 };
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let max = *sorted.last().unwrap_or(&0);
    Distribution {
        p25: percentile(&sorted, 25),
        p50: percentile(&sorted, 50),
        p75: percentile(&sorted, 75),
        p95: percentile(&sorted, 95),
        p99: percentile(&sorted, 99),
        max,
    }
}

fn percentile(sorted: &[u64], p: u8) -> u64 {
    let n = sorted.len();
    if n == 0 {
        return 0;
    }
    if n < 4 {
        // Nearest-rank.
        let idx = ((p as f64 / 100.0) * n as f64).ceil() as usize;
        return sorted[idx.saturating_sub(1).min(n - 1)];
    }
    // Linear interpolation.
    let pos = (p as f64 / 100.0) * (n as f64 - 1.0);
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        return sorted[lo];
    }
    let frac = pos - pos.floor();
    let lo_val = sorted[lo] as f64;
    let hi_val = sorted[hi] as f64;
    (lo_val + frac * (hi_val - lo_val)).round() as u64
}
