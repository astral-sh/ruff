use std::cmp::Ordering;

use rustpython_parser::ast::{Arguments, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::identifier_range;
use ruff_python_ast::source_code::Locator;
use ruff_python_semantic::analyze::visibility::is_staticmethod;

use crate::checkers::ast::Checker;

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ExpectedParams {
    Fixed(usize),
    Range(usize, usize),
}

impl ExpectedParams {
    fn from_method(name: &str, is_staticmethod: bool) -> Option<ExpectedParams> {
        let expected_params = match name {
            "__del__" | "__repr__" | "__str__" | "__bytes__" | "__hash__" | "__bool__"
            | "__dir__" | "__len__" | "__length_hint__" | "__iter__" | "__reversed__"
            | "__neg__" | "__pos__" | "__abs__" | "__invert__" | "__complex__" | "__int__"
            | "__float__" | "__index__" | "__trunc__" | "__floor__" | "__ceil__" | "__enter__"
            | "__aenter__" | "__getnewargs_ex__" | "__getnewargs__" | "__getstate__"
            | "__reduce__" | "__copy__" | "__unicode__" | "__nonzero__" | "__await__"
            | "__aiter__" | "__anext__" | "__fspath__" | "__subclasses__" => {
                Some(ExpectedParams::Fixed(0))
            }
            "__format__" | "__lt__" | "__le__" | "__eq__" | "__ne__" | "__gt__" | "__ge__"
            | "__getattr__" | "__getattribute__" | "__delattr__" | "__delete__"
            | "__instancecheck__" | "__subclasscheck__" | "__getitem__" | "__missing__"
            | "__delitem__" | "__contains__" | "__add__" | "__sub__" | "__mul__"
            | "__truediv__" | "__floordiv__" | "__rfloordiv__" | "__mod__" | "__divmod__"
            | "__lshift__" | "__rshift__" | "__and__" | "__xor__" | "__or__" | "__radd__"
            | "__rsub__" | "__rmul__" | "__rtruediv__" | "__rmod__" | "__rdivmod__"
            | "__rpow__" | "__rlshift__" | "__rrshift__" | "__rand__" | "__rxor__" | "__ror__"
            | "__iadd__" | "__isub__" | "__imul__" | "__itruediv__" | "__ifloordiv__"
            | "__imod__" | "__ilshift__" | "__irshift__" | "__iand__" | "__ixor__" | "__ior__"
            | "__ipow__" | "__setstate__" | "__reduce_ex__" | "__deepcopy__" | "__cmp__"
            | "__matmul__" | "__rmatmul__" | "__imatmul__" | "__div__" => {
                Some(ExpectedParams::Fixed(1))
            }
            "__setattr__" | "__get__" | "__set__" | "__setitem__" | "__set_name__" => {
                Some(ExpectedParams::Fixed(2))
            }
            "__exit__" | "__aexit__" => Some(ExpectedParams::Fixed(3)),
            "__round__" => Some(ExpectedParams::Range(0, 1)),
            "__pow__" => Some(ExpectedParams::Range(1, 2)),
            _ => None,
        }?;

        Some(if is_staticmethod {
            expected_params
        } else {
            match expected_params {
                ExpectedParams::Fixed(n) => ExpectedParams::Fixed(n + 1),
                ExpectedParams::Range(min, max) => ExpectedParams::Range(min + 1, max + 1),
            }
        })
    }

    fn message(&self) -> String {
        match self {
            ExpectedParams::Fixed(n) if *n == 1 => "1 parameter".to_string(),
            ExpectedParams::Fixed(n) => {
                format!("{} parameters", *n)
            }
            ExpectedParams::Range(min, max) => {
                format!("between {} and {} parameters", *min, *max)
            }
        }
    }
}

/// ## What it does
/// Checks for "special" methods that have an unexpected method signature.
///
/// ## Why is this bad?
/// "Special" methods, like `__len__`, are expected to adhere to a specific,
/// standard function signature. Implementing a "special" method using a
/// non-standard function signature can lead to unexpected and surprising
/// behavior for users of a given class.
///
/// ## Example
/// ```python
/// class Bookshelf:
///     def __init__(self):
///         self._books = ["Foo", "Bar", "Baz"]
///
///     def __len__(self, index):  # __len__ does not except an index parameter
///         return len(self._books)
///
///     def __getitem__(self, index):
///         return self._books[index]
/// ```
///
/// Use instead:
/// ```python
/// class Bookshelf:
///     def __init__(self):
///         self._books = ["Foo", "Bar", "Baz"]
///
///     def __len__(self):
///         return len(self._books)
///
///     def __getitem__(self, index):
///         return self._books[index]
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/reference/datamodel.html)
#[violation]
pub struct UnexpectedSpecialMethodSignature {
    method_name: String,
    expected_params: ExpectedParams,
    actual_params: usize,
}

impl Violation for UnexpectedSpecialMethodSignature {
    #[derive_message_formats]
    fn message(&self) -> String {
        let verb = if self.actual_params > 1 {
            "were"
        } else {
            "was"
        };
        format!(
            "The special method `{}` expects {}, {} {} given",
            self.method_name,
            self.expected_params.message(),
            self.actual_params,
            verb
        )
    }
}

/// PLE0302
pub(crate) fn unexpected_special_method_signature(
    checker: &mut Checker,
    stmt: &Stmt,
    name: &str,
    decorator_list: &[Expr],
    args: &Arguments,
    locator: &Locator,
) {
    if !checker.semantic_model().scope().kind.is_class() {
        return;
    }

    // Ignore methods with positional-only or keyword-only parameters, or variadic parameters.
    if !args.posonlyargs.is_empty() || !args.kwonlyargs.is_empty() || args.kwarg.is_some() {
        return;
    }

    // Method has no parameter, will be caught by no-method-argument (E0211/N805).
    if args.args.is_empty() && args.vararg.is_none() {
        return;
    }

    let actual_params = args.args.len();
    let optional_params = args.defaults.len();
    let mandatory_params = actual_params - optional_params;

    let Some(expected_params) = ExpectedParams::from_method(name, is_staticmethod(checker.semantic_model(), decorator_list)) else {
        return;
    };

    let valid_signature = match expected_params {
        ExpectedParams::Range(min, max) => {
            if mandatory_params >= min {
                mandatory_params <= max
            } else {
                args.vararg.is_some() || actual_params <= max
            }
        }
        ExpectedParams::Fixed(expected) => match expected.cmp(&mandatory_params) {
            Ordering::Less => false,
            Ordering::Greater => args.vararg.is_some() || actual_params >= expected,
            Ordering::Equal => true,
        },
    };

    if !valid_signature {
        checker.diagnostics.push(Diagnostic::new(
            UnexpectedSpecialMethodSignature {
                method_name: name.to_owned(),
                expected_params,
                actual_params,
            },
            identifier_range(stmt, locator),
        ));
    }
}
