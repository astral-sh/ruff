use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::{self as ast, ExprContext, Stmt};
use ruff_python_semantic::BindingKind;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for accesses of module-level (global) variable names inside
/// `for` and `while` loop bodies within functions.
///
/// ## Why is this bad?
/// In CPython, looking up a global variable uses the `LOAD_GLOBAL` bytecode
/// instruction, which is slower than `LOAD_FAST` for local variables.
/// In performance-sensitive loops, caching a global as a local variable
/// before the loop can provide a measurable speedup.
///
/// As with all `perflint` rules, this is only intended as a
/// micro-optimization. In many cases, it will have a negligible impact on
/// performance.
///
/// ## Example
/// ```python
/// x = 10
///
///
/// def foo(seq):
///     for item in seq:
///         print(x)
/// ```
///
/// Use instead:
/// ```python
/// x = 10
///
///
/// def foo(seq):
///     local_x = x
///     for item in seq:
///         print(local_x)
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct LoopGlobalUsage {
    name: String,
}

impl Violation for LoopGlobalUsage {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LoopGlobalUsage { name } = self;
        format!("Global variable `{name}` accessed in loop body")
    }
}

/// PERF202
pub(crate) fn loop_global_usage(checker: &Checker, body: &[Stmt]) {
    let semantic = checker.semantic();

    // Only relevant inside functions; global-to-global has no perf difference
    if semantic.current_scope().kind.is_module() {
        return;
    }

    let mut visitor = GlobalNameCollector::default();
    visitor.visit_body(body);
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
            checker.report_diagnostic(
                LoopGlobalUsage {
                    name: name_expr.id.to_string(),
                },
                name_expr.range(),
            );
        }
    }
}

#[derive(Debug, Default)]
struct GlobalNameCollector<'a> {
    names: Vec<&'a ast::ExprName>,
}

impl<'a> Visitor<'a> for GlobalNameCollector<'a> {
    fn visit_expr(&mut self, expr: &'a ast::Expr) {
        match expr {
            ast::Expr::Name(name) if name.ctx == ExprContext::Load => {
                self.names.push(name);
            }
            // Don't recurse into lambda bodies
            //
            // they are deferred by the checker and their names won't be resolved yet
            ast::Expr::Lambda(_) => {}
            _ => walk_expr(self, expr),
        }
    }

    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            // Don't recurse into nested scopes
            //
            // function/class bodies are deferred by the checker too
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            // Don't recurse into nested loops
            //
            // they get their own dispatch
            Stmt::For(_) | Stmt::While(_) => {}
            // Augmented assignment targets are also loaded (LOAD_GLOBAL + STORE_GLOBAL),
            // but the AST marks them as Store context
            //
            // Collect the target name explicitly
            Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => {
                if let ast::Expr::Name(name) = target.as_ref() {
                    self.names.push(name);
                }
                walk_stmt(self, stmt);
            }
            // `walk_stmt` for `Stmt::If` visits elif test expressions and then
            // calls `walk_elif_else_clause` which visits them again. Handle
            // `Stmt::If` manually to avoid collecting duplicate names
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
