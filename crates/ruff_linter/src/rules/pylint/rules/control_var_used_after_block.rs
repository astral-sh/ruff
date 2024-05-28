use std::{fmt, iter};

use regex::Regex;
use ruff_python_ast::{self as ast, Arguments, Expr, ExprContext, Stmt, WithItem};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::statement_visitor::{walk_stmt, StatementVisitor};
use ruff_python_semantic::{Binding, Scope, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for variables defined in `for` loops and `with` statements that
/// get overwritten within the body, for example by another `for` loop or
/// `with` statement or by direct assignment.
///
/// ## Why is this bad?
/// Redefinition of a loop variable inside the loop's body causes its value
/// to differ from the original loop iteration for the remainder of the
/// block, in a way that will likely cause bugs.
///
/// In Python, unlike many other languages, `for` loops and `with`
/// statements don't define their own scopes. Therefore, a nested loop that
/// uses the same target variable name as an outer loop will reuse the same
/// actual variable, and the value from the last iteration will "leak out"
/// into the remainder of the enclosing loop.
///
/// While this mistake is easy to spot in small examples, it can be hidden
/// in larger blocks of code, where the definition and redefinition of the
/// variable may not be visible at the same time.
///
/// ## Example
/// ```python
/// for i in range(10):
///     i = 9
///     print(i)  # prints 9 every iteration
///
/// for i in range(10):
///     for i in range(10):  # original value overwritten
///         pass
///     print(i)  # also prints 9 every iteration
///
/// with path1.open() as f:
///     with path2.open() as f:
///         f = path2.open()
///     print(f.readline())  # prints a line from path2
/// ```
#[violation]
pub struct ControlVarUsedAfterBlock {
    control_var_name: String,
    block_kind: BlockKind,
}

impl Violation for ControlVarUsedAfterBlock {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ControlVarUsedAfterBlock {
            control_var_name,
            block_kind,
        } = self;
        // Prefix the nouns describing the outer and inner kinds with "outer" and "inner"
        // to better distinguish them, but to avoid confusion, only do so if the outer and inner
        // kinds are equal. For example, instead of:
        //
        //    "Outer `for` loop variable `i` overwritten by inner assignment target."
        //
        // We have:
        //
        //    "`for` loop variable `i` overwritten by assignment target."
        //
        // While at the same time, we have:
        //
        //    "Outer `for` loop variable `i` overwritten by inner `for` loop target."
        //    "Outer `with` statement variable `f` overwritten by inner `with` statement target."

        format!(
            "{block_kind} variable {control_var_name} from {block_kind} is used outside of block"
        )
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum BlockKind {
    For,
    With,
}

impl fmt::Display for BlockKind {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BlockKind::For => fmt.write_str("`for` loop"),
            BlockKind::With => fmt.write_str("`with` statement"),
        }
    }
}

/// WPS441: Forbid control variables after the block body.
pub(crate) fn control_var_used_after_block(checker: &Checker, scope: &Scope) {
    println!("qwerqwere control_var_used_after_block");
    // if scope.uses_locals() && scope.kind.is_function() {
    //     return;
    // }

    for (name, binding) in scope
        .bindings()
        .map(|(name, binding_id)| (name, checker.semantic().binding(binding_id)))
    // .filter_map(|(name, binding)| {
    //     if (binding.kind.is_assignment()
    //         || binding.kind.is_named_expr_assignment()
    //         || binding.kind.is_with_item_var())
    //         && (!binding.is_unpacked_assignment() || checker.settings.preview.is_enabled())
    //         && !binding.is_nonlocal()
    //         && !binding.is_global()
    //         && !binding.is_used()
    //         && !checker.settings.dummy_variable_rgx.is_match(name)
    //         && !matches!(
    //             name,
    //             "__tracebackhide__"
    //                 | "__traceback_info__"
    //                 | "__traceback_supplement__"
    //                 | "__debuggerskip__"
    //         )
    //     {
    //         return Some((name, binding));
    //     }

    //     None
    // })
    {
        println!("asdf {:?} {:?}", name, binding);
        println!("asdfasdf {:?}", binding.references);
        for reference in binding.references() {
            println!("asdfasdfasdf {:?}", checker.semantic().reference(reference));

            // TODO: Look if the reference is under the same block as the binding
            // (see `too_many_nested_blocks` for an example of how to work with blocks)
        }
    }
}
