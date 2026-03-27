use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::{self as ast, ExprContext, Stmt};
use ruff_python_semantic::BindingKind;
use ruff_text_size::{Ranged, TextRange};

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for frequent accesses of module-level (global) variable names
/// inside `for` and `while` loop bodies within functions.
///
/// ## Why is this bad?
/// In CPython, looking up a global variable uses the `LOAD_GLOBAL` bytecode
/// instruction, which is slower than `LOAD_FAST` for local variables.
/// In performance-sensitive loops, caching a global as a local variable
/// before the loop can provide a measurable speedup.
///
/// This rule is inspired by perflint's `W8202` but has a narrower scope
/// to reduce noise. Unlike the original, it only fires when global
/// variables are used as direct operands in expressions (e.g., arithmetic,
/// comparisons, boolean operations) at least 5 times within a single loop
/// body, indicating a likely performance-sensitive hot path. Globals used
/// in method calls, attribute access, or subscripts are not counted.
///
/// As with all `perflint` rules, this is only intended as a
/// micro-optimization. In many cases, it will have a negligible impact on
/// performance.
///
/// ## Example
/// ```python
/// THRESHOLD = 100
///
///
/// def foo(items):
///     for item in items:
///         if item.a > THRESHOLD and item.b > THRESHOLD:
///             item.c = item.a * THRESHOLD + item.b * THRESHOLD
///             item.d = (item.a + item.b) / THRESHOLD
/// ```
///
/// Use instead:
/// ```python
/// THRESHOLD = 100
///
///
/// def foo(items):
///     threshold = THRESHOLD
///     for item in items:
///         if item.a > threshold and item.b > threshold:
///             item.c = item.a * threshold + item.b * threshold
///             item.d = (item.a + item.b) / threshold
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct LoopGlobalUsage {
    names: Vec<String>,
    count: usize,
}

impl Violation for LoopGlobalUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LoopGlobalUsage { names, count } = self;
        let formatted = names
            .iter()
            .map(|n| format!("`{n}`"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("Global variables accessed in loop body {count} times: {formatted}")
    }
}

/// Minimum number of qualifying global references in a loop body before
/// the rule fires
const MIN_GLOBAL_REFS: usize = 5;

/// PERF202
pub(crate) fn loop_global_usage(checker: &Checker, stmt: &Stmt, body: &[Stmt]) {
    let semantic = checker.semantic();

    // Only relevant inside functions; global-to-global has no perf difference
    if semantic.current_scope().kind.is_module() {
        return;
    }

    let mut visitor = GlobalNameCollector::default();

    // The `while` test expression is evaluated every iteration, so
    // globals there count as direct reads. Visit it without forcing
    // `in_direct_read` - the BinOp/Compare/BoolOp/UnaryOp handlers
    // will set the flag for their operands
    if let Stmt::While(ast::StmtWhile { test, .. }) = stmt {
        visitor.visit_expr(test);
    }

    visitor.visit_body(body);

    // Filter to only qualifying global refs, deferring string allocation
    let mut ref_count: usize = 0;
    let mut global_names: Vec<&ast::ExprName> = Vec::new();

    for name_expr in visitor.names {
        let Some(binding_id) = semantic.resolve_name(name_expr) else {
            continue;
        };
        let binding = semantic.binding(binding_id);
        if binding.kind.is_builtin() {
            continue;
        }

        // Follow `global` declarations through to the actual module-level binding
        let binding = if let BindingKind::Global(Some(target_id)) = binding.kind {
            semantic.binding(target_id)
        } else {
            binding
        };

        // Only flag variable assignments, not imports, function/class definitions, etc
        if !matches!(
            binding.kind,
            BindingKind::Assignment
                | BindingKind::NamedExprAssignment
                | BindingKind::LoopVar
                | BindingKind::WithItemVar
        ) {
            continue;
        }
        if semantic.scopes[binding.scope].kind.is_module() {
            ref_count += 1;
            global_names.push(name_expr);
        }
    }

    if ref_count < MIN_GLOBAL_REFS {
        return;
    }

    // Report on the loop header only (e.g., `for x in y:`), not the
    // entire loop body which can be very large
    let header_end = match stmt {
        Stmt::For(ast::StmtFor { iter, .. }) => iter.end(),
        Stmt::While(ast::StmtWhile { test, .. }) => test.end(),
        _ => stmt.end(),
    };
    let header_range = TextRange::new(stmt.start(), header_end);

    let mut unique_names: Vec<String> = Vec::new();
    for name_expr in global_names {
        let name = name_expr.id.to_string();
        if !unique_names.contains(&name) {
            unique_names.push(name);
        }
    }

    checker.report_diagnostic(
        LoopGlobalUsage {
            names: unique_names,
            count: ref_count,
        },
        header_range,
    );
}

#[derive(Debug, Default)]
struct GlobalNameCollector<'a> {
    /// Names found in direct-read positions (binary/comparison/boolean
    /// expression operands and augmented assignment targets)
    names: Vec<&'a ast::ExprName>,
    /// Whether we're inside an expression context where a name is a
    /// direct operand (not the object of an attribute access or call)
    in_direct_read: bool,
}

impl<'a> Visitor<'a> for GlobalNameCollector<'a> {
    fn visit_expr(&mut self, expr: &'a ast::Expr) {
        match expr {
            ast::Expr::Name(name) if name.ctx == ExprContext::Load && self.in_direct_read => {
                self.names.push(name);
            }

            // Direct-read contexts: operands of binary, comparison,
            // boolean, and unary expressions
            ast::Expr::BinOp(ast::ExprBinOp {
                left, right, op: _, ..
            }) => {
                let prev = self.in_direct_read;
                self.in_direct_read = true;
                self.visit_expr(left);
                self.visit_expr(right);
                self.in_direct_read = prev;
            }
            ast::Expr::Compare(ast::ExprCompare {
                left, comparators, ..
            }) => {
                let prev = self.in_direct_read;
                self.in_direct_read = true;
                self.visit_expr(left);
                for comparator in comparators {
                    self.visit_expr(comparator);
                }
                self.in_direct_read = prev;
            }
            ast::Expr::BoolOp(ast::ExprBoolOp { values, .. }) => {
                let prev = self.in_direct_read;
                self.in_direct_read = true;
                for value in values {
                    self.visit_expr(value);
                }
                self.in_direct_read = prev;
            }
            ast::Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) => {
                let prev = self.in_direct_read;
                self.in_direct_read = true;
                self.visit_expr(operand);
                self.in_direct_read = prev;
            }

            // Function calls, attribute access, and subscripts break the
            // direct-read chain - a global inside these is not a bare
            // operand
            ast::Expr::Call(_) | ast::Expr::Attribute(_) | ast::Expr::Subscript(_) => {
                let prev = self.in_direct_read;
                self.in_direct_read = false;
                walk_expr(self, expr);
                self.in_direct_read = prev;
            }

            // Don't recurse into lambda bodies; they are deferred by the
            // checker and their names won't be resolved yet
            ast::Expr::Lambda(_) => {}

            _ => walk_expr(self, expr),
        }
    }

    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            // Don't recurse into nested scopes; function/class bodies are
            // deferred by the checker and their names won't be resolved yet
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            // Don't recurse into nested loops; they get their own dispatch
            Stmt::For(_) | Stmt::While(_) => {}
            // Augmented assignment is an implicit binary operation
            // (LOAD_GLOBAL + op + STORE_GLOBAL). Both the target and
            // value are direct reads
            Stmt::AugAssign(ast::StmtAugAssign { target, value, .. }) => {
                if let ast::Expr::Name(name) = target.as_ref() {
                    self.names.push(name);
                }
                let prev = self.in_direct_read;
                self.in_direct_read = true;
                self.visit_expr(value);
                self.in_direct_read = prev;
            }
            // `walk_stmt` for `Stmt::If` visits elif test expressions and
            // then calls `walk_elif_else_clause` which visits them again.
            // Handle `Stmt::If` manually to avoid collecting duplicate
            // names
            Stmt::If(ast::StmtIf {
                test,
                body,
                elif_else_clauses,
                ..
            }) => {
                self.visit_expr(test);
                self.visit_body(body);
                for clause in elif_else_clauses {
                    if let Some(test) = &clause.test {
                        self.visit_expr(test);
                    }
                    self.visit_body(&clause.body);
                }
            }
            _ => walk_stmt(self, stmt),
        }
    }
}
