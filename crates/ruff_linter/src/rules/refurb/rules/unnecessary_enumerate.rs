use std::fmt;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_python_ast::{Arguments, Expr, Int};
use ruff_python_codegen::Generator;
use ruff_python_semantic::analyze::typing::{is_dict, is_list, is_set, is_tuple};

use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::edits::pad;

/// ## What it does
/// Checks for uses of `enumerate` that discard either the index or the value
/// when iterating over a sequence.
///
/// ## Why is this bad?
/// The built-in `enumerate` function is useful when you need both the index and
/// value of a sequence.
///
/// If you only need the index or values of a sequence, you should iterate over
/// `range(len(...))` or the sequence itself, respectively, instead. This is
/// more efficient and communicates the intent of the code more clearly.
///
/// ## Known problems
/// This rule is prone to false negatives due to type inference limitations;
/// namely, it will only suggest a fix using the `len` builtin function if the
/// sequence passed to `enumerate` is an instantiated as a list, set, dict, or
/// tuple literal, or annotated as such with a type annotation.
///
/// The `len` builtin function is not defined for all object types (such as
/// generators), and so refactoring to use `len` over `enumerate` is not always
/// safe.
///
/// ## Example
/// ```python
/// for index, _ in enumerate(sequence):
///     print(index)
///
/// for _, value in enumerate(sequence):
///     print(value)
/// ```
///
/// Use instead:
/// ```python
/// for index in range(len(sequence)):
///     print(index)
///
/// for value in sequence:
///     print(value)
/// ```
///
/// ## References
/// - [Python documentation: `enumerate`](https://docs.python.org/3/library/functions.html#enumerate)
/// - [Python documentation: `range`](https://docs.python.org/3/library/stdtypes.html#range)
/// - [Python documentation: `len`](https://docs.python.org/3/library/functions.html#len)
#[violation]
pub struct UnnecessaryEnumerate {
    subset: EnumerateSubset,
}

impl Violation for UnnecessaryEnumerate {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryEnumerate { subset } = self;
        match subset {
            EnumerateSubset::Indices => {
                format!("`enumerate` value is unused, use `for x in range(len(y))` instead")
            }
            EnumerateSubset::Values => {
                format!("`enumerate` index is unused, use `for x in y` instead")
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        let UnnecessaryEnumerate { subset } = self;
        match subset {
            EnumerateSubset::Indices => Some("Replace with `range(len(...))`".to_string()),
            EnumerateSubset::Values => Some("Remove `enumerate`".to_string()),
        }
    }
}

/// FURB148
pub(crate) fn unnecessary_enumerate(checker: &mut Checker, stmt_for: &ast::StmtFor) {
    // Check the for statement is of the form `for x, y in func(...)`.
    let Expr::Tuple(ast::ExprTuple { elts, .. }) = stmt_for.target.as_ref() else {
        return;
    };
    let [index, value] = elts.as_slice() else {
        return;
    };
    let Expr::Call(ast::ExprCall {
        func, arguments, ..
    }) = stmt_for.iter.as_ref()
    else {
        return;
    };

    // Get the first argument, which is the sequence to iterate over.
    let Some(Expr::Name(sequence)) = arguments.args.first() else {
        return;
    };

    // Check that the function is the `enumerate` builtin.
    if !checker.semantic().match_builtin_expr(func, "enumerate") {
        return;
    }

    // Check if the index and value are used.
    match (
        checker.semantic().is_unused(index),
        checker.semantic().is_unused(value),
    ) {
        (true, true) => {
            // Both the index and the value are unused.
        }
        (false, false) => {
            // Both the index and the value are used.
        }
        (true, false) => {
            let mut diagnostic = Diagnostic::new(
                UnnecessaryEnumerate {
                    subset: EnumerateSubset::Values,
                },
                func.range(),
            );

            // The index is unused, so replace with `for value in sequence`.
            let replace_iter =
                Edit::range_replacement(sequence.id.to_string(), stmt_for.iter.range());
            let replace_target = Edit::range_replacement(
                pad(
                    checker.locator().slice(value).to_string(),
                    stmt_for.target.range(),
                    checker.locator(),
                ),
                stmt_for.target.range(),
            );
            diagnostic.set_fix(Fix::unsafe_edits(replace_iter, [replace_target]));

            checker.diagnostics.push(diagnostic);
        }
        (false, true) => {
            // Ensure the sequence object works with `len`. If it doesn't, the
            // fix is unclear.
            let Some(binding) = checker
                .semantic()
                .only_binding(sequence)
                .map(|id| checker.semantic().binding(id))
            else {
                return;
            };
            // This will lead to a lot of false negatives, but it is the best
            // we can do with the current type inference.
            if !is_list(binding, checker.semantic())
                && !is_dict(binding, checker.semantic())
                && !is_set(binding, checker.semantic())
                && !is_tuple(binding, checker.semantic())
            {
                return;
            }

            // The value is unused, so replace with `for index in range(len(sequence))`.
            let mut diagnostic = Diagnostic::new(
                UnnecessaryEnumerate {
                    subset: EnumerateSubset::Indices,
                },
                func.range(),
            );
            if checker.semantic().has_builtin_binding("range")
                && checker.semantic().has_builtin_binding("len")
            {
                // If the `start` argument is set to something other than the `range` default,
                // there's no clear fix.
                let start = arguments.find_argument("start", 1);
                if start.map_or(true, |start| {
                    matches!(
                        start,
                        Expr::NumberLiteral(ast::ExprNumberLiteral {
                            value: ast::Number::Int(Int::ZERO),
                            ..
                        })
                    )
                }) {
                    let replace_iter = Edit::range_replacement(
                        generate_range_len_call(sequence.id.clone(), checker.generator()),
                        stmt_for.iter.range(),
                    );

                    let replace_target = Edit::range_replacement(
                        pad(
                            checker.locator().slice(index).to_string(),
                            stmt_for.target.range(),
                            checker.locator(),
                        ),
                        stmt_for.target.range(),
                    );

                    diagnostic.set_fix(Fix::unsafe_edits(replace_iter, [replace_target]));
                }
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum EnumerateSubset {
    /// E.g., `for _, value in enumerate(sequence):`.
    Indices,
    /// E.g., `for index, _ in enumerate(sequence):`.
    Values,
}

impl fmt::Display for EnumerateSubset {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            EnumerateSubset::Indices => write!(f, "indices"),
            EnumerateSubset::Values => write!(f, "values"),
        }
    }
}

/// Format a code snippet to call `range(len(name))`, where `name` is the given
/// sequence name.
fn generate_range_len_call(name: Name, generator: Generator) -> String {
    // Construct `name`.
    let var = ast::ExprName {
        id: name,
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
    };
    // Construct `len(name)`.
    let len = ast::ExprCall {
        func: Box::new(
            ast::ExprName {
                id: Name::new_static("len"),
                ctx: ast::ExprContext::Load,
                range: TextRange::default(),
            }
            .into(),
        ),
        arguments: Arguments {
            args: Box::from([var.into()]),
            keywords: Box::from([]),
            range: TextRange::default(),
        },
        range: TextRange::default(),
    };
    // Construct `range(len(name))`.
    let range = ast::ExprCall {
        func: Box::new(
            ast::ExprName {
                id: Name::new_static("range"),
                ctx: ast::ExprContext::Load,
                range: TextRange::default(),
            }
            .into(),
        ),
        arguments: Arguments {
            args: Box::from([len.into()]),
            keywords: Box::from([]),
            range: TextRange::default(),
        },
        range: TextRange::default(),
    };
    // And finally, turn it into a statement.
    let stmt = ast::StmtExpr {
        value: Box::new(range.into()),
        range: TextRange::default(),
    };
    generator.stmt(&stmt.into())
}
