use itertools::Itertools;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use ruff_python_ast::{self as ast, CmpOp, Expr};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::settings::types::PreviewMode;

/// ## What it does
/// Checks for object type comparisons using `==` and other comparison
/// operators.
///
/// ## Why is this bad?
/// Unlike a direct type comparison, `isinstance` will also check if an object
/// is an instance of a class or a subclass thereof.
///
/// Under [preview mode](https://docs.astral.sh/ruff/preview), this rule also
/// allows for direct type comparisons using `is` and `is not`, to check for
/// exact type equality (while still forbidding comparisons using `==` and
/// `!=`).
///
/// ## Example
/// ```python
/// if type(obj) == type(1):
///     pass
///
/// if type(obj) == int:
///     pass
/// ```
///
/// Use instead:
/// ```python
/// if isinstance(obj, int):
///     pass
/// ```
#[violation]
pub struct TypeComparison {
    preview: PreviewMode,
}

impl Violation for TypeComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        match self.preview {
            PreviewMode::Disabled => format!("Do not compare types, use `isinstance()`"),
            PreviewMode::Enabled => format!(
                "Use `is` and `is not` for type comparisons, or `isinstance()` for isinstance checks"
            ),
        }
    }
}

/// E721
pub(crate) fn type_comparison(checker: &mut Checker, compare: &ast::ExprCompare) {
    match checker.settings.preview {
        PreviewMode::Disabled => deprecated_type_comparison(checker, compare),
        PreviewMode::Enabled => preview_type_comparison(checker, compare),
    }
}

fn deprecated_type_comparison(checker: &mut Checker, compare: &ast::ExprCompare) {
    for ((left, right), op) in std::iter::once(compare.left.as_ref())
        .chain(compare.comparators.iter())
        .tuple_windows()
        .zip(compare.ops.iter())
    {
        if !matches!(op, CmpOp::Is | CmpOp::IsNot | CmpOp::Eq | CmpOp::NotEq) {
            continue;
        }

        // Left-hand side must be, e.g., `type(obj)`.
        let Expr::Call(ast::ExprCall { func, .. }) = left else {
            continue;
        };

        let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() else {
            continue;
        };

        if !(id == "type" && checker.semantic().is_builtin("type")) {
            continue;
        }

        // Right-hand side must be, e.g., `type(1)` or `int`.
        match right {
            Expr::Call(ast::ExprCall {
                func, arguments, ..
            }) => {
                // Ex) `type(obj) is type(1)`
                let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() else {
                    continue;
                };

                if id == "type" && checker.semantic().is_builtin("type") {
                    // Allow comparison for types which are not obvious.
                    if arguments
                        .args
                        .first()
                        .is_some_and(|arg| !arg.is_name_expr() && !arg.is_none_literal_expr())
                    {
                        checker.diagnostics.push(Diagnostic::new(
                            TypeComparison {
                                preview: PreviewMode::Disabled,
                            },
                            compare.range(),
                        ));
                    }
                }
            }
            Expr::Attribute(ast::ExprAttribute { value, .. }) => {
                // Ex) `type(obj) is types.NoneType`
                if checker
                    .semantic()
                    .resolve_call_path(value.as_ref())
                    .is_some_and(|call_path| matches!(call_path.as_slice(), ["types", ..]))
                {
                    checker.diagnostics.push(Diagnostic::new(
                        TypeComparison {
                            preview: PreviewMode::Disabled,
                        },
                        compare.range(),
                    ));
                }
            }
            Expr::Name(ast::ExprName { id, .. }) => {
                // Ex) `type(obj) is int`
                if matches!(
                    id.as_str(),
                    "int"
                        | "str"
                        | "float"
                        | "bool"
                        | "complex"
                        | "bytes"
                        | "list"
                        | "dict"
                        | "set"
                        | "memoryview"
                ) && checker.semantic().is_builtin(id)
                {
                    checker.diagnostics.push(Diagnostic::new(
                        TypeComparison {
                            preview: PreviewMode::Disabled,
                        },
                        compare.range(),
                    ));
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn preview_type_comparison(checker: &mut Checker, compare: &ast::ExprCompare) {
    for (left, right) in std::iter::once(compare.left.as_ref())
        .chain(compare.comparators.iter())
        .tuple_windows()
        .zip(compare.ops.iter())
        .filter(|(_, op)| matches!(op, CmpOp::Eq | CmpOp::NotEq))
        .map(|((left, right), _)| (left, right))
    {
        // If either expression is a type...
        if is_type(left, checker.semantic()) || is_type(right, checker.semantic()) {
            // And neither is a `dtype`...
            if is_dtype(left, checker.semantic()) || is_dtype(right, checker.semantic()) {
                continue;
            }

            // Disallow the comparison.
            checker.diagnostics.push(Diagnostic::new(
                TypeComparison {
                    preview: PreviewMode::Enabled,
                },
                compare.range(),
            ));
        }
    }
}

/// Returns `true` if the [`Expr`] is known to evaluate to a type (e.g., `int`, or `type(1)`).
fn is_type(expr: &Expr, semantic: &SemanticModel) -> bool {
    match expr {
        Expr::Call(ast::ExprCall {
            func, arguments, ..
        }) => {
            // Ex) `type(obj) == type(1)`
            let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() else {
                return false;
            };

            if !(id == "type" && semantic.is_builtin("type")) {
                return false;
            };

            // Allow comparison for types which are not obvious.
            arguments
                .args
                .first()
                .is_some_and(|arg| !arg.is_name_expr() && !arg.is_none_literal_expr())
        }
        Expr::Name(ast::ExprName { id, .. }) => {
            // Ex) `type(obj) == int`
            matches!(
                id.as_str(),
                "bool"
                    | "bytearray"
                    | "bytes"
                    | "classmethod"
                    | "complex"
                    | "dict"
                    | "enumerate"
                    | "filter"
                    | "float"
                    | "frozenset"
                    | "int"
                    | "list"
                    | "map"
                    | "memoryview"
                    | "object"
                    | "property"
                    | "range"
                    | "reversed"
                    | "set"
                    | "slice"
                    | "staticmethod"
                    | "str"
                    | "super"
                    | "tuple"
                    | "type"
                    | "zip"
                    | "ArithmeticError"
                    | "AssertionError"
                    | "AttributeError"
                    | "BaseException"
                    | "BlockingIOError"
                    | "BrokenPipeError"
                    | "BufferError"
                    | "BytesWarning"
                    | "ChildProcessError"
                    | "ConnectionAbortedError"
                    | "ConnectionError"
                    | "ConnectionRefusedError"
                    | "ConnectionResetError"
                    | "DeprecationWarning"
                    | "EnvironmentError"
                    | "EOFError"
                    | "Exception"
                    | "FileExistsError"
                    | "FileNotFoundError"
                    | "FloatingPointError"
                    | "FutureWarning"
                    | "GeneratorExit"
                    | "ImportError"
                    | "ImportWarning"
                    | "IndentationError"
                    | "IndexError"
                    | "InterruptedError"
                    | "IOError"
                    | "IsADirectoryError"
                    | "KeyboardInterrupt"
                    | "KeyError"
                    | "LookupError"
                    | "MemoryError"
                    | "ModuleNotFoundError"
                    | "NameError"
                    | "NotADirectoryError"
                    | "NotImplementedError"
                    | "OSError"
                    | "OverflowError"
                    | "PendingDeprecationWarning"
                    | "PermissionError"
                    | "ProcessLookupError"
                    | "RecursionError"
                    | "ReferenceError"
                    | "ResourceWarning"
                    | "RuntimeError"
                    | "RuntimeWarning"
                    | "StopAsyncIteration"
                    | "StopIteration"
                    | "SyntaxError"
                    | "SyntaxWarning"
                    | "SystemError"
                    | "SystemExit"
                    | "TabError"
                    | "TimeoutError"
                    | "TypeError"
                    | "UnboundLocalError"
                    | "UnicodeDecodeError"
                    | "UnicodeEncodeError"
                    | "UnicodeError"
                    | "UnicodeTranslateError"
                    | "UnicodeWarning"
                    | "UserWarning"
                    | "ValueError"
                    | "Warning"
                    | "ZeroDivisionError"
            ) && semantic.is_builtin(id)
        }
        _ => false,
    }
}

/// Returns `true` if the [`Expr`] appears to be a reference to a NumPy dtype, since:
/// > `dtype` are a bit of a strange beast, but definitely best thought of as instances, not
/// > classes, and they are meant to be comparable not just to their own class, but also to the
/// corresponding scalar types (e.g., `x.dtype == np.float32`) and strings (e.g.,
/// `x.dtype == ['i1,i4']`; basically, __eq__ always tries to do `dtype(other)`).
fn is_dtype(expr: &Expr, semantic: &SemanticModel) -> bool {
    match expr {
        // Ex) `np.dtype(obj)`
        Expr::Call(ast::ExprCall { func, .. }) => semantic
            .resolve_call_path(func)
            .is_some_and(|call_path| matches!(call_path.as_slice(), ["numpy", "dtype"])),
        // Ex) `obj.dtype`
        Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
            // Ex) `obj.dtype`
            attr.as_str() == "dtype"
        }
        _ => false,
    }
}
