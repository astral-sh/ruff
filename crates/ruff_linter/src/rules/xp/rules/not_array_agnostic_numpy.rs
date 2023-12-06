/// use ...
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

/// ## What it does
///

#[violation]
pub struct NotArrayAgnosticNumPy {
    existing: String,
    migration_guide: Option<String>,
    code_action: Option<String>,
}

impl Violation for NotArrayAgnosticNumPy {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NotArrayAgnosticNumPy {
            existing,
            migration_guide,
            code_action: _,
        } = self;
        match migration_guide {
            Some(migration_guide) => {
                format!("`{existing}` is not in the array API standard. {migration_guide}",)
            }
            None => format!("`{existing}` is not in the array API standard."),
        }
    }

    fn fix_title(&self) -> Option<String> {
        let NotArrayAgnosticNumPy {
            existing: _,
            migration_guide: _,
            code_action,
        } = self;
        code_action.clone()
    }
}

#[derive(Debug)]
struct Replacement<'a> {
    existing: &'a str,
    details: Details<'a>,
}

#[derive(Debug)]
enum Details<'a> {
    /// There is a direct replacement in the array API standard.
    AutoImport { path: &'a str, name: &'a str },
    /// There is no direct replacement in the standard.
    Manual { guideline: Option<&'a str> },
}

impl Details<'_> {
    fn guideline(&self) -> Option<String> {
        match self {
            Details::AutoImport { path, name } => Some(format!("Use `{path}.{name}` instead.")),
            Details::Manual { guideline } => guideline.map(ToString::to_string),
        }
    }

    fn code_action(&self) -> Option<String> {
        match self {
            Details::AutoImport { path, name } => Some(format!("Replace with `{path}.{name}`")),
            Details::Manual { guideline: _ } => None,
        }
    }
}

const array_api_functions: [&str; ...] = [
    // methods
    "__abs__",
    "__add__",
    "__and__",
    "__array_namespace__",
    "__bool__",
    "__complex__",
    "__dlpack__",
    "__dlpack_device__",
    "__eq__",
    "__float__",
    "__floordiv__",
    "__ge__",
    "__getitem__",
    "__gt__",
    "__index__",
    "__int__",
    "__invert__",
    "__le__",
    "__lshift__",
    "__lt__",
    "__matmul__",
    "__mod__",
    "__mul__",
    "__ne__",
    "__neg__",
    "__or__",
    "__pos__",
    "__pow__",
    "__rshift__",
    "__setitem__",
    "__sub__",
    "__truediv__",
    "__xor__",
    "to_device",
    // constants
    "e",
    "inf",
    "nan",
    "newaxis",
    "pi",
    // creation functions
    "arange",
    "asarray",
    "empty",
    "empty_like",
    "eye",
    "from_dlpack",
    "full",
    "full_like",
    "linspace",
    "meshgrid",
    "ones",
    "ones_like",
    "tril",
    "triu",
    "zeros",
    "zeros_like",
    // data type functions
    "astype",
    "can_cast",
    "finfo",
    "iinfo",
    "isdtype",
    "result_type",
    // data types
    "bool",
    "int8",
    "int16",
    "int32",
    "int64",
    "uint8",
    "uint16",
    "uint32",
    "uint64",
    "float32",
    "float64",
    "complex64",
    "complex128",
    // element-wise functions
    "abs",
    "acos",
    "acosh",
    "add",
    "asin",
    "asinh",
    "atan",
    "atan2",
    "atanh",
    "bitwise_and",
    "bitwise_left_shift",
    "bitwise_invert",
    "bitwise_or",
    "bitwise_right_shift",
    "bitwise_xor",
    "ceil",
    "conj",
    "cos",
    "cosh",
    "divide",
    "equal",
    "exp",
    "expm1",
    "floor",
    "floor_divide",
    "greater",
    "greater_equal",
    "imag",
    "isfinite",
    "isinf",
    "isnan",
    "less",
    "less_equal",
    "log",
    "log1p",
    "log2",
    "log10",
    "logaddexp",
    "logical_and",
    "logical_not",
    "logical_or",
    "logical_xor",
    "multiply",
    "negative",
    "not_equal",
    "positive",
    "pow",
    "real",
    "remainder",
    "round",
    "sign",
    "sin",
    "sinh",
    "square",
    "sqrt",
    "subtract",
    "tan",
    "tanh",
    "trunc",
    // indexing functions
    "take",
    // linear algebra functions
    "matmul",
    "matrix_transpose",
    "tensordot",
    "vecdot",
    // manipulation functions
    "broadcast_arrays",
    "broadcast_to",
    "concat",
    "expand_dims",
    "flip",
    "permute_dims",
    "reshape",
    "roll",
    "squeeze",
    "stack",
    // searching functions
    "argmax",
    "argmin",
    "nonzero",
    "where",
    // set functions
    "unique_all",
    "unique_counts",
    "unique_inverse",
    "unique_values",
    // sorting functions
    "argsort",
    "sort",
    // statistical functions
    "max",
    "mean",
    "min",
    "prod",
    "std",
    "sum",
    "var",
    "all",
    "any",
    // version
    "__array_api_version__"
]

///XP001
pub(crate) fn not_array_agnostic_numpy(checker: &mut Checker, expr: &Expr) {
    let maybe_replacement = checker
        .semantic()
        .resolve_call_path(expr)
        .and_then(|call_path| match call_path.as_slice() {
            ["numpy", "arccos"] => Some(Replacement {
                existing: "arccos",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "acos",
                },
            }),
            ["numpy", "arccosh"] => Some(Replacement {
                existing: "arccosh",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "acosh",
                },
            }),
            ["numpy", "arcsin"] => Some(Replacement {
                existing: "arcsin",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "asin",
                },
            }),
            ["numpy", "arcsinh"] => Some(Replacement {
                existing: "arcsinh",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "asinh",
                },
            }),
            ["numpy", "arctan"] => Some(Replacement {
                existing: "arctan",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "atan",
                },
            }),
            ["numpy", "arctan2"] => Some(Replacement {
                existing: "arctan2",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "atan2",
                },
            }),
            ["numpy", "arctanh"] => Some(Replacement {
                existing: "arctanh",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "atanh",
                },
            }),
            ["numpy", "left_shift"] => Some(Replacement {
                existing: "left_shift",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "bitwise_left_shift",
                },
            }),
            ["numpy", "arccos"] => Some(Replacement {
                existing: "arccos",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "acos",
                },
            }),
            ["numpy", "invert"] => Some(Replacement {
                existing: "invert",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "bitwise_invert",
                },
            }),
            ["numpy", "right_shift"] => Some(Replacement {
                existing: "right_shift",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "bitwise_right_shift",
                },
            }),
            ["numpy", "bool_"] => Some(Replacement {
                existing: "bool_",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "bool",
                },
            }),
            ["numpy", "concatenate"] => Some(Replacement {
                existing: "concatenate",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "concat",
                },
            }),
            ["numpy", "power"] => Some(Replacement {
                existing: "power",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "pow",
                },
            }),
            ["numpy", func] if &array_api_functions.contains(func) => None,
            ["numpy", func] => Some(Replacement {
                existing: func,
                details: Details::Manual {
                    guideline: Some(
                        format!("xp.{} is not in the array API standard", func)
                    ),
                },
            }),
            _ => None,
        });

    if let Some(replacement) = maybe_replacement {
        let mut diagnostic = Diagnostic::new(
            NotArrayAgnosticNumPy {
                existing: replacement.existing.to_string(),
                migration_guide: replacement.details.guideline(),
                code_action: replacement.details.code_action(),
            },
            expr.range(),
        );
        match replacement.details {
            Details::AutoImport { path, name } => {
                diagnostic.try_set_fix(|| {
                    let (import_edit, binding) = checker.importer().get_or_import_symbol(
                        &ImportRequest::import_from(path, name),
                        expr.start(),
                        checker.semantic(),
                    )?;
                    let replacement_edit = Edit::range_replacement(binding, expr.range());
                    Ok(Fix::unsafe_edits(import_edit, [replacement_edit]))
                });
            }
            Details::Manual { guideline: _ } => {}
        };
        checker.diagnostics.push(diagnostic);
    }
}
