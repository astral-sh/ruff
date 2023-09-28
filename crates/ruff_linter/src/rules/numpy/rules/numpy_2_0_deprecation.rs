use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

/// ## What it does
/// Checks for uses of NumPy's main namespace members removed in 2.0 release.
///
/// ## Why is this bad?
/// NumPy 2.0 release includes an overhaul of Python API. It's meant to remove redundant aliases
/// and routines, and establish unambiguous ways for accessing constants, dtypes and functions.
///
/// This rule is meant to provide automatic fixes for NumPy's main namespace changes, that are
/// expected to be the most disruptive ones.
///
/// ## Examples
/// ```python
/// import numpy as np
///
/// arr1 = [np.Infinity, np.NaN, np.nan, np.PINF, np.inf]
/// arr2 = [np.float_(1.5), np.float64(5.1)]
/// np.round_(arr2)
/// ```
///
/// Use instead:
/// ```python
/// import numpy as np
///
/// arr1 = [np.inf, np.nan, np.nan, np.inf, np.inf]
/// arr2 = [np.float64(1.5), np.float64(5.1)]
/// np.round(arr2)
/// ```
#[violation]
pub struct Numpy2Deprecation {
    existing: String,
    migration_guide: String,
}

impl Violation for Numpy2Deprecation {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Numpy2Deprecation {
            existing,
            migration_guide,
        } = self;
        format!("`np.{existing}` will be removed in the NumPy 2.0. {migration_guide}")
    }

    fn fix_title(&self) -> Option<String> {
        let Numpy2Deprecation {
            migration_guide, ..
        } = self;
        Some(format!("{migration_guide}"))
    }
}

#[derive(Debug)]
struct Replacement<'a> {
    existing: &'a str,
    details: Details<'a>,
}

#[derive(Debug)]
enum Details<'a> {
    AutoImport { path: &'a str, name: &'a str },
    AutoPurePython { python_expr: &'a str },
    Manual { guideline: &'a str },
}

impl Details<'_> {
    fn get_guideline(&self) -> String {
        match self {
            Details::AutoImport { path, name } => {
                format!("Use `{path}.{name}` instead.")
            }
            Details::AutoPurePython { python_expr } => {
                format!("Use `{python_expr}` instead.")
            }
            Details::Manual { guideline } => (*guideline).to_string(),
        }
    }
}

/// NPY201
pub(crate) fn numpy_2_0_deprecation(checker: &mut Checker, expr: &Expr) {
    let maybe_replacement = checker
        .semantic()
        .resolve_call_path(expr)
        .and_then(|call_path| match call_path.as_slice() {
            // NumPy's main namespace np.* members removed in 2.0
            ["numpy", "add_docstring"] => Some(Replacement {
                existing: "add_docstring",
                details: Details::AutoImport {
                    path: "numpy.lib",
                    name: "add_docstring",
                },
            }),
            ["numpy", "add_newdoc"] => Some(Replacement {
                existing: "add_newdoc",
                details: Details::AutoImport {
                    path: "numpy.lib",
                    name: "add_newdoc",
                },
            }),
            ["numpy", "add_newdoc_ufunc"] => Some(Replacement {
                existing: "add_newdoc_ufunc",
                details: Details::Manual {
                    guideline: "It’s an internal function and doesn't have a replacement.",
                },
            }),
            ["numpy", "asfarray"] => Some(Replacement {
                existing: "asfarray",
                details: Details::Manual {
                    guideline: "Use np.asarray with a float dtype instead.",
                },
            }),
            ["numpy", "byte_bounds"] => Some(Replacement {
                existing: "byte_bounds",
                details: Details::AutoImport {
                    path: "numpy.lib.array_utils",
                    name: "byte_bounds",
                },
            }),
            ["numpy", "cast"] => Some(Replacement {
                existing: "cast",
                details: Details::Manual {
                    guideline: "Use np.asarray(arr, dtype=dtype) instead.",
                },
            }),
            ["numpy", "cfloat"] => Some(Replacement {
                existing: "cfloat",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "complex128",
                },
            }),
            ["numpy", "clongfloat"] => Some(Replacement {
                existing: "clongfloat",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "clongdouble",
                },
            }),
            ["numpy", "compat"] => Some(Replacement {
                existing: "compat",
                details: Details::Manual {
                    guideline: "There's no replacement, as Python 2 is no longer supported.",
                },
            }),
            ["numpy", "complex_"] => Some(Replacement {
                existing: "complex_",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "complex128",
                },
            }),
            ["numpy", "DataSource"] => Some(Replacement {
                existing: "DataSource",
                details: Details::AutoImport {
                    path: "numpy.lib.npyio",
                    name: "DataSource",
                },
            }),
            ["numpy", "deprecate"] => Some(Replacement {
                existing: "deprecate",
                details: Details::Manual {
                    guideline: "Emit DeprecationWarning with warnings.warn directly, or use \
                                typing.deprecated.",
                },
            }),
            ["numpy", "deprecate_with_doc"] => Some(Replacement {
                existing: "deprecate_with_doc",
                details: Details::Manual {
                    guideline: "Emit DeprecationWarning with warnings.warn directly, or use \
                    typing.deprecated.",
                },
            }),
            ["numpy", "disp"] => Some(Replacement {
                existing: "disp",
                details: Details::Manual {
                    guideline: "Use your own printing function instead.",
                },
            }),
            ["numpy", "fastCopyAndTranspose"] => Some(Replacement {
                existing: "fastCopyAndTranspose",
                details: Details::Manual {
                    guideline: "Use arr.T.copy() instead.",
                },
            }),
            ["numpy", "find_common_type"] => Some(Replacement {
                existing: "find_common_type",
                details: Details::Manual {
                    guideline: "Use numpy.promote_types or numpy.result_type instead. \
                    To achieve semantics for the scalar_types argument, use \
                    numpy.result_type and pass the Python values 0, 0.0, or 0j.",
                },
            }),
            ["numpy", "get_array_wrap"] => Some(Replacement {
                existing: "get_array_wrap",
                details: Details::Manual {
                    guideline: "It's a niche function and there's no replacement.",
                },
            }),
            ["numpy", "float_"] => Some(Replacement {
                existing: "float_",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "float64",
                },
            }),
            ["numpy", "geterrobj"] => Some(Replacement {
                existing: "geterrobj",
                details: Details::Manual {
                    guideline: "Use the np.errstate context manager instead.",
                },
            }),
            ["numpy", "INF"] => Some(Replacement {
                existing: "INF",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "inf",
                },
            }),
            ["numpy", "Inf"] => Some(Replacement {
                existing: "Inf",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "inf",
                },
            }),
            ["numpy", "Infinity"] => Some(Replacement {
                existing: "Infinity",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "inf",
                },
            }),
            ["numpy", "infty"] => Some(Replacement {
                existing: "infty",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "inf",
                },
            }),
            ["numpy", "issctype"] => Some(Replacement {
                existing: "issctype",
                details: Details::Manual {
                    guideline: "It's a niche function and there's no replacement.",
                },
            }),
            ["numpy", "issubclass_"] => Some(Replacement {
                existing: "issubclass_",
                details: Details::AutoPurePython {
                    python_expr: "issubclass",
                },
            }),
            ["numpy", "issubsctype"] => Some(Replacement {
                existing: "issubsctype",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "issubdtype",
                },
            }),
            ["numpy", "mat"] => Some(Replacement {
                existing: "mat",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "asmatrix",
                },
            }),
            ["numpy", "maximum_sctype"] => Some(Replacement {
                existing: "maximum_sctype",
                details: Details::Manual {
                    guideline: "It's a niche function and there's no replacement.",
                },
            }),
            ["numpy", "NaN"] => Some(Replacement {
                existing: "NaN",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "nan",
                },
            }),
            ["numpy", "nbytes"] => Some(Replacement {
                existing: "nbytes",
                details: Details::Manual {
                    guideline: "Use np.dtype(<dtype>).itemsize instead.",
                },
            }),
            ["numpy", "NINF"] => Some(Replacement {
                existing: "NINF",
                details: Details::AutoPurePython {
                    python_expr: "-np.inf",
                },
            }),
            ["numpy", "NZERO"] => Some(Replacement {
                existing: "NZERO",
                details: Details::AutoPurePython {
                    python_expr: "-0.0",
                },
            }),
            ["numpy", "longcomplex"] => Some(Replacement {
                existing: "longcomplex",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "clongdouble",
                },
            }),
            ["numpy", "longfloat"] => Some(Replacement {
                existing: "longfloat",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "longdouble",
                },
            }),
            ["numpy", "lookfor"] => Some(Replacement {
                existing: "lookfor",
                details: Details::Manual {
                    guideline: "Search NumPy’s documentation directly.",
                },
            }),
            ["numpy", "obj2sctype"] => Some(Replacement {
                existing: "obj2sctype",
                details: Details::Manual {
                    guideline: "It's a niche function and there's no replacement.",
                },
            }),
            ["numpy", "PINF"] => Some(Replacement {
                existing: "PINF",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "inf",
                },
            }),
            ["numpy", "PZERO"] => Some(Replacement {
                existing: "PZERO",
                details: Details::AutoPurePython { python_expr: "0.0" },
            }),
            ["numpy", "recfromcsv"] => Some(Replacement {
                existing: "recfromcsv",
                details: Details::Manual {
                    guideline: "Use np.genfromtxt with comma delimiter instead.",
                },
            }),
            ["numpy", "recfromtxt"] => Some(Replacement {
                existing: "recfromtxt",
                details: Details::Manual {
                    guideline: "Use np.genfromtxt instead.",
                },
            }),
            ["numpy", "round_"] => Some(Replacement {
                existing: "round_",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "round",
                },
            }),
            ["numpy", "safe_eval"] => Some(Replacement {
                existing: "safe_eval",
                details: Details::AutoImport {
                    path: "ast",
                    name: "literal_eval",
                },
            }),
            ["numpy", "sctype2char"] => Some(Replacement {
                existing: "sctype2char",
                details: Details::Manual {
                    guideline: "It's a niche function and there's no replacement.",
                },
            }),
            ["numpy", "sctypes"] => Some(Replacement {
                existing: "sctypes",
                details: Details::Manual {
                    guideline: "It's a niche function and there's no replacement.",
                },
            }),
            ["numpy", "seterrobj"] => Some(Replacement {
                existing: "seterrobj",
                details: Details::Manual {
                    guideline: "Use the np.errstate context manager instead.",
                },
            }),
            ["numpy", "set_string_function"] => Some(Replacement {
                existing: "set_string_function",
                details: Details::Manual {
                    guideline: "Use np.set_printoptions instead with a formatter for \
                    custom printing of NumPy objects.",
                },
            }),
            ["numpy", "singlecomplex"] => Some(Replacement {
                existing: "singlecomplex",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "complex64",
                },
            }),
            ["numpy", "string_"] => Some(Replacement {
                existing: "string_",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "bytes_",
                },
            }),
            ["numpy", "source"] => Some(Replacement {
                existing: "source",
                details: Details::AutoImport {
                    path: "inspect",
                    name: "getsource",
                },
            }),
            ["numpy", "tracemalloc_domain"] => Some(Replacement {
                existing: "tracemalloc_domain",
                details: Details::AutoImport {
                    path: "numpy.lib",
                    name: "tracemalloc_domain",
                },
            }),
            ["numpy", "unicode_"] => Some(Replacement {
                existing: "unicode_",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "str_",
                },
            }),
            ["numpy", "who"] => Some(Replacement {
                existing: "who",
                details: Details::Manual {
                    guideline: "Use an IDE variable explorer or `locals()` instead.",
                },
            }),
            _ => None,
        });

    if let Some(replacement) = maybe_replacement {
        let mut diagnostic = Diagnostic::new(
            Numpy2Deprecation {
                existing: replacement.existing.to_string(),
                migration_guide: replacement.details.get_guideline(),
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
                    Ok(Fix::safe_edits(import_edit, [replacement_edit]))
                });
            }
            Details::AutoPurePython { python_expr } => diagnostic.set_fix(Fix::safe_edit(
                Edit::range_replacement(python_expr.to_string(), expr.range()),
            )),
            Details::Manual { guideline: _ } => {}
        };
        checker.diagnostics.push(diagnostic);
    }
}
