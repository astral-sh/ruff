use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, ExprList, ExprName, ExprTuple, Stmt, StmtFor};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::{Binding, ScopeId, SemanticModel, TypingOnlyBindingsStatus};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::rules::refurb::helpers::IterLocation;
use crate::{AlwaysFixableViolation, Applicability, Edit, Fix};

use crate::rules::refurb::helpers::parenthesize_loop_iter_if_necessary;

/// ## What it does
/// Checks for the use of `IOBase.write` in a for loop.
///
/// ## Why is this bad?
/// When writing a batch of elements, it's more idiomatic to use a single method call to
/// `IOBase.writelines`, rather than write elements one by one.
///
/// ## Example
/// ```python
/// from pathlib import Path
///
/// with Path("file").open("w") as f:
///     for line in lines:
///         f.write(line)
///
/// with Path("file").open("wb") as f_b:
///     for line_b in lines_b:
///         f_b.write(line_b.encode())
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// with Path("file").open("w") as f:
///     f.writelines(lines)
///
/// with Path("file").open("wb") as f_b:
///     f_b.writelines(line_b.encode() for line_b in lines_b)
/// ```
///
/// ## Fix safety
/// This fix is marked as unsafe if it would cause comments to be deleted.
///
/// ## References
/// - [Python documentation: `io.IOBase.writelines`](https://docs.python.org/3/library/io.html#io.IOBase.writelines)
#[derive(ViolationMetadata)]
pub(crate) struct ForLoopWrites {
    name: String,
}

impl AlwaysFixableViolation for ForLoopWrites {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `{}.write` in a for loop", self.name)
    }

    fn fix_title(&self) -> String {
        format!("Replace with `{}.writelines`", self.name)
    }
}

/// FURB122
pub(crate) fn for_loop_writes_binding(checker: &Checker, binding: &Binding) {
    if !binding.kind.is_loop_var() {
        return;
    }

    let semantic = checker.semantic();

    let Some(for_stmt) = binding
        .statement(semantic)
        .and_then(|stmt| stmt.as_for_stmt())
    else {
        return;
    };

    if for_stmt.is_async {
        return;
    }

    let binding_names = binding_names(&for_stmt.target);

    if !binding_names
        .first()
        .is_some_and(|name| name.range().contains_range(binding.range))
    {
        return;
    }

    for_loop_writes(checker, for_stmt, binding.scope, &binding_names);
}

/// FURB122
pub(crate) fn for_loop_writes_stmt(checker: &Checker, for_stmt: &StmtFor) {
    // Loops with bindings are handled later.
    if !binding_names(&for_stmt.target).is_empty() {
        return;
    }

    let scope_id = checker.semantic().scope_id;

    for_loop_writes(checker, for_stmt, scope_id, &[]);
}

/// Find the names in a `for` loop target
/// that are assigned to during iteration.
///
/// ```python
/// for ((), [(a,), [[b]]], c.d, e[f], *[*g]) in h:
///     #      ^      ^                   ^
///     ...
/// ```
fn binding_names(for_target: &Expr) -> Vec<&ExprName> {
    fn collect_names<'a>(expr: &'a Expr, names: &mut Vec<&'a ExprName>) {
        match expr {
            Expr::Name(name) => names.push(name),

            Expr::Starred(starred) => collect_names(&starred.value, names),

            Expr::List(ExprList { elts, .. }) | Expr::Tuple(ExprTuple { elts, .. }) => elts
                .iter()
                .for_each(|element| collect_names(element, names)),

            _ => {}
        }
    }

    let mut names = vec![];
    collect_names(for_target, &mut names);
    names
}

/// FURB122
fn for_loop_writes(
    checker: &Checker,
    for_stmt: &StmtFor,
    scope_id: ScopeId,
    binding_names: &[&ExprName],
) {
    if !for_stmt.orelse.is_empty() {
        return;
    }
    let [Stmt::Expr(stmt_expr)] = for_stmt.body.as_slice() else {
        return;
    };

    let Some(call_expr) = stmt_expr.value.as_call_expr() else {
        return;
    };
    let Some(expr_attr) = call_expr.func.as_attribute_expr() else {
        return;
    };

    if &expr_attr.attr != "write" {
        return;
    }

    if !call_expr.arguments.keywords.is_empty() {
        return;
    }
    let [write_arg] = call_expr.arguments.args.as_ref() else {
        return;
    };

    let Some(io_object_name) = expr_attr.value.as_name_expr() else {
        return;
    };

    let semantic = checker.semantic();

    // Determine whether `f` in `f.write()` was bound to a file object.
    let Some(name) = semantic.resolve_name(io_object_name) else {
        return;
    };
    let binding = semantic.binding(name);
    if !typing::is_io_base(binding, semantic) {
        return;
    }

    if loop_variables_are_used_outside_loop(binding_names, for_stmt.range, semantic, scope_id) {
        return;
    }

    let locator = checker.locator();
    let content = match (for_stmt.target.as_ref(), write_arg) {
        (Expr::Name(for_target), Expr::Name(write_arg)) if for_target.id == write_arg.id => {
            format!(
                "{}.writelines({})",
                locator.slice(io_object_name),
                parenthesize_loop_iter_if_necessary(for_stmt, checker, IterLocation::Call),
            )
        }
        (for_target, write_arg) => {
            format!(
                "{}.writelines({} for {} in {})",
                locator.slice(io_object_name),
                locator.slice(write_arg),
                locator.slice(for_target),
                parenthesize_loop_iter_if_necessary(for_stmt, checker, IterLocation::Comprehension),
            )
        }
    };

    let applicability = if checker.comment_ranges().intersects(for_stmt.range) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };
    let fix = Fix::applicable_edit(
        Edit::range_replacement(content, for_stmt.range),
        applicability,
    );

    checker
        .report_diagnostic(
            ForLoopWrites {
                name: io_object_name.id.to_string(),
            },
            for_stmt.range,
        )
        .set_fix(fix);
}

fn loop_variables_are_used_outside_loop(
    binding_names: &[&ExprName],
    loop_range: TextRange,
    semantic: &SemanticModel,
    scope_id: ScopeId,
) -> bool {
    let find_binding_id = |name: &ExprName, offset: TextSize| {
        semantic.simulate_runtime_load_at_location_in_scope(
            name.id.as_str(),
            TextRange::at(offset, 0.into()),
            scope_id,
            TypingOnlyBindingsStatus::Disallowed,
        )
    };

    // If the load simulation succeeds at the position right before the loop,
    // that binding is shadowed.
    // ```python
    //   a = 1
    //   for a in b: ...
    // # ^ Load here
    // ```
    let name_overwrites_outer =
        |name: &ExprName| find_binding_id(name, loop_range.start()).is_some();

    let name_is_used_later = |name: &ExprName| {
        let Some(binding_id) = find_binding_id(name, loop_range.end()) else {
            return false;
        };

        for reference_id in semantic.binding(binding_id).references() {
            let reference = semantic.reference(reference_id);

            if !loop_range.contains_range(reference.range()) {
                return true;
            }
        }

        false
    };

    binding_names
        .iter()
        .any(|name| name_overwrites_outer(name) || name_is_used_later(name))
}
