use num_traits::ToPrimitive;
use std::fmt;

use crate::autofix::snippet::SourceCodeSnippet;
use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::{Arguments, Constant, Expr};
use ruff_python_codegen::Generator;
use ruff_python_semantic::helpers::is_unused;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

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
#[violation]
pub struct UnnecessaryEnumerate {
    subset: EnumerateSubset,
    iterable_suggestion: Option<SourceCodeSnippet>,
}

impl Violation for UnnecessaryEnumerate {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryEnumerate {
            subset,
            iterable_suggestion,
        } = self;
        if let Some(suggestion) = iterable_suggestion.map(SourceCodeSnippet::full_display)
        {
            format!("Do not iterate over `enumerate` when only using the {subset}, iterate over `{suggestion}` instead")
        } else {
            format!("Do not iterate over `enumerate` when only using the {subset}")
        }
    }

    fn autofix_title(&self) -> Option<String> {
        let UnnecessaryEnumerate {
            subset,
            iterable_suggestion,
        } = self;
        if let Some(suggestion) = iterable_suggestion
            .as_ref()
            .and_then(SourceCodeSnippet::full_display)
        {
            Some(format!("Replace with `{suggestion}`"))
        } else {
            match subset {
                EnumerateSubset::Indices => {
                    Some("Replace with iteration over `range(len(...))`".to_string())
                }
                EnumerateSubset::Values => {
                    Some("Replace with direct iteration over the sequence".to_string())
                }
            }
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

    // Check that the function is the `enumerate` builtin.
    let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() else {
        return;
    };
    if id != "enumerate" {
        return;
    };
    if !checker.semantic().is_builtin("enumerate") {
        return;
    };

    // Get the `start` argument, if it is a constant integer.
    let start = start(arguments);

    // Get the first argument, which is the sequence to iterate over.
    let Some(Expr::Name(ast::ExprName { id: sequence, .. })) = arguments.args.first() else {
        return;
    };

    // Check if the index and value are used.
    match (
        is_unused(index, checker.semantic()),
        is_unused(value, checker.semantic()),
    ) {
        (true, true) => {
            // Both the index and the value are unused.
        }
        (false, false) => {
            // Both the index and the value are used.
        }
        (true, false) => {
            // The index is unused, so replace with `for value in sequence`.

            // Attempt to create a suggested iterator replacement, as it's used
            // in both the message and the autofix.
            let replace_iter = match start {
                Some(start) if start > 0 => {
                    // If the start argument is a positive integer, there isn't a clear fix.
                    None
                }
                _ => {
                    // Otherwise, suggest iterating over the sequence itself.
                    Some(Edit::range_replacement(
                        sequence.into(),
                        stmt_for.iter.range(),
                    ))
                }
            };
            let iterable_suggestion: Option<SourceCodeSnippet> =
                replace_iter.as_ref().and_then(|edit| {
                    edit.content()
                        .map(|content| SourceCodeSnippet::new(content.to_string()))
                });

            let mut diagnostic = Diagnostic::new(
                UnnecessaryEnumerate {
                    subset: EnumerateSubset::Values,
                    iterable_suggestion,
                },
                func.range(),
            );

            if checker.patch(diagnostic.kind.rule()) {
                // If we made a suggested iterator replacement, use it and
                // replace the target to produce a fix.
                if let Some(replace_iter) = replace_iter {
                    let replace_target = Edit::range_replacement(
                        checker.locator().slice(value).to_string(),
                        stmt_for.target.range(),
                    );
                    diagnostic.set_fix(Fix::suggested_edits(replace_iter, [replace_target]));
                }
            }

            checker.diagnostics.push(diagnostic);
        }
        (false, true) => {
            // The value is unused, so replace with `for index in range(len(sequence))`.

            // Create suggested iterator replacement, as it's used in both the
            // message and the autofix.
            let replace_iter = Edit::range_replacement(
                generate_range_len_call(sequence, start, checker.generator()),
                stmt_for.iter.range(),
            );
            let iterable_suggestion = replace_iter
                .content()
                .map(|content| SourceCodeSnippet::new(content.to_string()));

            let mut diagnostic = Diagnostic::new(
                UnnecessaryEnumerate {
                    subset: EnumerateSubset::Indices,
                    iterable_suggestion,
                },
                func.range(),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Replace the target and combine it with the suggested
                // iterator replacement to produce a fix.
                let replace_target = Edit::range_replacement(
                    checker.locator().slice(index).to_string(),
                    stmt_for.target.range(),
                );
                diagnostic.set_fix(Fix::suggested_edits(replace_iter, [replace_target]));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum EnumerateSubset {
    Indices,
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

/// Returns the value of the `start` argument to `enumerate`, if it is a
/// constant integer. Otherwise, returns `None`.
fn start(arguments: &Arguments) -> Option<u32> {
    let step_param = arguments.find_argument("start", 1)?;
    if let Expr::Constant(ast::ExprConstant {
        value: Constant::Int(value),
        ..
    }) = &step_param
    {
        value.to_u32()
    } else {
        None
    }
}

/// Format a code snippet to call `range(len(name))`, where `name` is the given
/// sequence name.
fn generate_range_len_call(name: &str, start: Option<u32>, generator: Generator) -> String {
    // Construct `name`.
    let var = ast::ExprName {
        id: name.to_string(),
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
    };
    // Construct `len(name)`.
    let len = ast::ExprCall {
        func: Box::new(
            ast::ExprName {
                id: "len".to_string(),
                ctx: ast::ExprContext::Load,
                range: TextRange::default(),
            }
            .into(),
        ),
        arguments: Arguments {
            args: vec![var.into()],
            keywords: vec![],
            range: TextRange::default(),
        },
        range: TextRange::default(),
    };
    // Construct `range(len(name))`.
    let range_args: Vec<Expr> = match start {
        Some(start) if start > 0 => {
            let start_expr: Expr = Expr::Constant(ast::ExprConstant {
                range: TextRange::default(),
                value: Constant::Int(start.into()),
            });
            vec![start_expr, len.into()]
        }
        _ => vec![len.into()],
    };
    let range = ast::ExprCall {
        func: Box::new(
            ast::ExprName {
                id: "range".to_string(),
                ctx: ast::ExprContext::Load,
                range: TextRange::default(),
            }
            .into(),
        ),
        arguments: Arguments {
            args: range_args,
            keywords: vec![],
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
