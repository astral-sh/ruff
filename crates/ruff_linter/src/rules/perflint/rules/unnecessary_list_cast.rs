use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::statement_visitor::{StatementVisitor, walk_stmt};
use ruff_python_ast::{self as ast, Expr, ExprKind, Stmt, StmtKind, Suite};
use ruff_python_semantic::analyze::typing::find_assigned_value;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::edits;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for explicit casts to `list` on for-loop iterables.
///
/// ## Why is this bad?
/// Using a `list()` call to eagerly iterate over an already-iterable type
/// (like a tuple, list, or set) is inefficient, as it forces Python to create
/// a new list unnecessarily.
///
/// Removing the `list()` call will not change the behavior of the code, but
/// may improve performance.
///
/// Note that, as with all `perflint` rules, this is only intended as a
/// micro-optimization, and will have a negligible impact on performance in
/// most cases.
///
/// ## Example
/// ```python
/// items = (1, 2, 3)
/// for i in list(items):
///     print(i)
/// ```
///
/// Use instead:
/// ```python
/// items = (1, 2, 3)
/// for i in items:
///     print(i)
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe if there's comments in the
/// `list()` call, as comments may be removed.
///
/// For example, the fix would be marked as unsafe in the following case:
/// ```python
/// items = (1, 2, 3)
/// for i in list(  # comment
///     items
/// ):
///     print(i)
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.276", category = Category::Performance)]
pub(crate) struct UnnecessaryListCast;

impl AlwaysFixableViolation for UnnecessaryListCast {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Do not cast an iterable to `list` before iterating over it".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove `list()` cast".to_string()
    }
}

/// PERF101
pub(crate) fn unnecessary_list_cast(checker: &Checker, iter: Expr<'_>, body: Suite<'_>) {
    let ExprKind::Call(call_node) = iter.classify() else {
        return;
    };
    let func = call_node.func();
    let arguments = call_node.arguments();
    let args = arguments.args();

    let Some([arg]) = args.as_array() else {
        return;
    };

    if !checker.semantic().match_builtin_expr(func, "list") {
        return;
    }

    match arg.fields() {
        ast::ExprFields::Tuple(ast::ExprTupleFields {
            range: iterable_range,
            ..
        })
        | ast::ExprFields::List(ast::ExprListFields {
            range: iterable_range,
            ..
        })
        | ast::ExprFields::Set(ast::ExprSetFields {
            range: iterable_range,
            ..
        }) => {
            let mut diagnostic = checker.report_diagnostic(UnnecessaryListCast, iter.range());
            diagnostic.set_fix(remove_cast(checker, iter.range(), iterable_range));
        }
        ast::ExprFields::Name(ast::ExprNameFields {
            id,
            range: iterable_range,
            ..
        }) => {
            let Some(value) = find_assigned_value(id, checker.semantic()) else {
                return;
            };
            if matches!(
                value.classify(),
                ExprKind::Tuple(_) | ExprKind::List(_) | ExprKind::Set(_)
            ) {
                // If the variable is being modified to, don't suggest removing the cast:
                //
                // ```python
                // items = ["foo", "bar"]
                // for item in list(items):
                //    items.append("baz")
                // ```
                //
                // Here, removing the `list()` cast would change the behavior of the code.
                let mut visitor = MutationVisitor::new(id);
                visitor.visit_body(body);
                if visitor.is_mutated {
                    return;
                }

                let mut diagnostic = checker.report_diagnostic(UnnecessaryListCast, iter.range());
                diagnostic.set_fix(remove_cast(checker, iter.range(), iterable_range));
            }
        }
        _ => {}
    }
}

/// Generate a [`Fix`] to remove a `list` cast from an expression.
fn remove_cast(checker: &Checker, list_range: TextRange, iterable_range: TextRange) -> Fix {
    let content = edits::pad(
        checker.locator().slice(iterable_range).to_string(),
        list_range,
        checker.locator(),
    );
    Fix::applicable_edit(
        Edit::range_replacement(content, list_range),
        if checker.comment_ranges().intersects(list_range) {
            Applicability::Unsafe
        } else {
            Applicability::Safe
        },
    )
}

/// A [`StatementVisitor`] that (conservatively) identifies mutations to a variable.
#[derive(Default)]
pub(crate) struct MutationVisitor<'a> {
    target: &'a str,
    is_mutated: bool,
}

impl<'a> MutationVisitor<'a> {
    fn new(target: &'a str) -> Self {
        Self {
            target,
            is_mutated: false,
        }
    }
}

impl<'a> StatementVisitor<'a> for MutationVisitor<'a> {
    fn visit_stmt(&mut self, stmt: Stmt<'a>) {
        if match_mutation(stmt, self.target) {
            self.is_mutated = true;
        } else {
            walk_stmt(self, stmt);
        }
    }
}

/// Check if a statement is (probably) a modification to the list assigned to the given identifier.
///
/// For example, `foo.append(bar)` would return `true` if `id` is `foo`.
fn match_mutation(stmt: Stmt<'_>, id: &str) -> bool {
    match stmt.classify() {
        // Ex) `foo.append(bar)`
        StmtKind::Expr(stmt_expr) => {
            let value = stmt_expr.value();

            let Some(ast::ExprCallFields { func, .. }) =
                value.as_call_expr().map(ruff_python_ast::ExprCall::fields)
            else {
                return false;
            };
            let Some(ast::ExprAttributeFields { value, attr, .. }) = func
                .as_attribute_expr()
                .map(ruff_python_ast::ExprAttribute::fields)
            else {
                return false;
            };
            if !matches!(
                attr.as_str(),
                "append" | "insert" | "extend" | "remove" | "pop" | "clear" | "reverse" | "sort"
            ) {
                return false;
            }
            let Some(ast::ExprNameFields { id: target_id, .. }) =
                value.as_name_expr().map(ruff_python_ast::ExprName::fields)
            else {
                return false;
            };
            target_id == id
        }
        // Ex) `foo[0] = bar`
        StmtKind::Assign(stmt_assign) => {
            let targets = stmt_assign.targets();

            targets.iter().any(|target| {
                if let Some(ast::ExprSubscriptFields { value: target, .. }) = target
                    .as_subscript_expr()
                    .map(ruff_python_ast::ExprSubscript::fields)
                {
                    if let Some(ast::ExprNameFields { id: target_id, .. }) =
                        target.as_name_expr().map(ruff_python_ast::ExprName::fields)
                    {
                        return target_id == id;
                    }
                }
                false
            })
        }
        // Ex) `foo += bar`
        StmtKind::AugAssign(stmt_aug_assign) => {
            let target = stmt_aug_assign.target();

            if let Some(ast::ExprNameFields { id: target_id, .. }) =
                target.as_name_expr().map(ruff_python_ast::ExprName::fields)
            {
                target_id == id
            } else {
                false
            }
        }
        // Ex) `foo[0]: int = bar`
        StmtKind::AnnAssign(stmt_ann_assign) => {
            let target = stmt_ann_assign.target();

            if let Some(ast::ExprSubscriptFields { value: target, .. }) = target
                .as_subscript_expr()
                .map(ruff_python_ast::ExprSubscript::fields)
            {
                if let Some(ast::ExprNameFields { id: target_id, .. }) =
                    target.as_name_expr().map(ruff_python_ast::ExprName::fields)
                {
                    return target_id == id;
                }
            }
            false
        }
        // Ex) `del foo[0]`
        StmtKind::Delete(stmt_delete) => {
            let targets = stmt_delete.targets();

            targets.iter().any(|target| {
                if let Some(ast::ExprSubscriptFields { value: target, .. }) = target
                    .as_subscript_expr()
                    .map(ruff_python_ast::ExprSubscript::fields)
                {
                    if let Some(ast::ExprNameFields { id: target_id, .. }) =
                        target.as_name_expr().map(ruff_python_ast::ExprName::fields)
                    {
                        return target_id == id;
                    }
                }
                false
            })
        }
        _ => false,
    }
}
