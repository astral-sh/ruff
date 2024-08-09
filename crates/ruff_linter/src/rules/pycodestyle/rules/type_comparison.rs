use itertools::Itertools;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use ruff_python_ast::{self as ast, CmpOp, Expr};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for object type comparisons using `==` and other comparison
/// operators.
///
/// ## Why is this bad?
/// Unlike a direct type comparison, `isinstance` will also check if an object
/// is an instance of a class or a subclass thereof.
///
/// If you want to check for an exact type match, use `is` or `is not`.
///
/// ## Known problems
/// When using libraries that override the `==` (`__eq__`) operator (such as NumPy,
/// Pandas, and SQLAlchemy), this rule may produce false positives, as converting
/// from `==` to `is` or `is not` will change the behavior of the code.
///
/// For example, the following operations are _not_ equivalent:
/// ```python
/// import numpy as np
///
/// np.array([True, False]) == False
/// # array([False,  True])
///
/// np.array([True, False]) is False
/// # False
/// ```
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
pub struct TypeComparison;

impl Violation for TypeComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Use `is` and `is not` for type comparisons, or `isinstance()` for isinstance checks"
        )
    }
}

/// E721
pub(crate) fn type_comparison(checker: &mut Checker, compare: &ast::ExprCompare) {
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
            checker
                .diagnostics
                .push(Diagnostic::new(TypeComparison, compare.range()));
        }
    }
}

/// Returns `true` if the [`Expr`] is known to evaluate to a type (e.g., `int`, or `type(1)`).
fn is_type(expr: &Expr, semantic: &SemanticModel) -> bool {
    match expr {
        Expr::Call(ast::ExprCall { func, .. }) => {
            // Ex) `type(obj) == type(1)`
            semantic.match_builtin_expr(func, "type")
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
            ) && semantic.has_builtin_binding(id)
        }
        _ => false,
    }
}

/// Returns `true` if the [`Expr`] appears to be a reference to a NumPy dtype, since:
/// > `dtype` are a bit of a strange beast, but definitely best thought of as instances, not
/// > classes, and they are meant to be comparable not just to their own class, but also to the
/// > corresponding scalar types (e.g., `x.dtype == np.float32`) and strings (e.g.,
/// > `x.dtype == ['i1,i4']`; basically, __eq__ always tries to do `dtype(other)`).
fn is_dtype(expr: &Expr, semantic: &SemanticModel) -> bool {
    match expr {
        // Ex) `np.dtype(obj)`
        Expr::Call(ast::ExprCall { func, .. }) => semantic
            .resolve_qualified_name(func)
            .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["numpy", "dtype"])),
        // Ex) `obj.dtype`
        Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
            // Ex) `obj.dtype`
            attr.as_str() == "dtype"
        }
        _ => false,
    }
}
