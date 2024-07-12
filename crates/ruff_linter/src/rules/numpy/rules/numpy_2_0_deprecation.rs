use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

/// ## What it does
/// Checks for uses of NumPy functions and constants that were removed from
/// the main namespace in NumPy 2.0.
///
/// ## Why is this bad?
/// NumPy 2.0 includes an overhaul of NumPy's Python API, intended to remove
/// redundant aliases and routines, and establish unambiguous mechanisms for
/// accessing constants, dtypes, and functions.
///
/// As part of this overhaul, a variety of deprecated NumPy functions and
/// constants were removed from the main namespace.
///
/// The majority of these functions and constants can be automatically replaced
/// by other members of the NumPy API or by equivalents from the Python
/// standard library. With the exception of renaming `numpy.byte_bounds` to
/// `numpy.lib.array_utils.byte_bounds`, all such replacements are backwards
/// compatible with earlier versions of NumPy.
///
/// This rule flags all uses of removed members, along with automatic fixes for
/// any backwards-compatible replacements.
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
    migration_guide: Option<String>,
    code_action: Option<String>,
}

impl Violation for Numpy2Deprecation {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Numpy2Deprecation {
            existing,
            migration_guide,
            code_action: _,
        } = self;
        match migration_guide {
            Some(migration_guide) => {
                format!("`np.{existing}` will be removed in NumPy 2.0. {migration_guide}",)
            }
            None => format!("`np.{existing}` will be removed without replacement in NumPy 2.0"),
        }
    }

    fn fix_title(&self) -> Option<String> {
        let Numpy2Deprecation {
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
    /// The deprecated member can be replaced by another member in the NumPy API.
    AutoImport {
        path: &'a str,
        name: &'a str,
        compatibility: Compatibility,
    },
    /// The deprecated member can be replaced by a member of the Python standard library.
    AutoPurePython { python_expr: &'a str },
    /// The deprecated member can be replaced by a manual migration.
    Manual { guideline: Option<&'a str> },
}

impl Details<'_> {
    fn guideline(&self) -> Option<String> {
        match self {
            Details::AutoImport {
                path,
                name,
                compatibility: Compatibility::BackwardsCompatible,
            } => Some(format!("Use `{path}.{name}` instead.")),
            Details::AutoImport {
                path,
                name,
                compatibility: Compatibility::Breaking,
            } => Some(format!(
                "Use `{path}.{name}` on NumPy 2.0, or ignore this warning on earlier versions."
            )),
            Details::AutoPurePython { python_expr } => {
                Some(format!("Use `{python_expr}` instead."))
            }
            Details::Manual { guideline } => guideline.map(ToString::to_string),
        }
    }

    fn code_action(&self) -> Option<String> {
        match self {
            Details::AutoImport {
                path,
                name,
                compatibility: Compatibility::BackwardsCompatible,
            } => Some(format!("Replace with `{path}.{name}`")),
            Details::AutoImport {
                path,
                name,
                compatibility: Compatibility::Breaking,
            } => Some(format!(
                "Replace with `{path}.{name}` (requires NumPy 2.0 or greater)"
            )),
            Details::AutoPurePython { python_expr } => {
                Some(format!("Replace with `{python_expr}`"))
            }
            Details::Manual { guideline: _ } => None,
        }
    }
}

#[derive(Debug)]
enum Compatibility {
    /// The changes is backwards compatible with earlier versions of NumPy.
    BackwardsCompatible,
    /// The change is breaking in NumPy 2.0.
    Breaking,
}
/// NPY201
pub(crate) fn numpy_2_0_deprecation(checker: &mut Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::NUMPY) {
        return;
    }

    let maybe_replacement = checker
        .semantic()
        .resolve_qualified_name(expr)
        .and_then(|qualified_name| match qualified_name.segments() {
            // NumPy's main namespace np.* members removed in 2.0
            ["numpy", "add_docstring"] => Some(Replacement {
                existing: "add_docstring",
                details: Details::AutoImport {
                    path: "numpy.lib",
                    name: "add_docstring",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "add_newdoc"] => Some(Replacement {
                existing: "add_newdoc",
                details: Details::AutoImport {
                    path: "numpy.lib",
                    name: "add_newdoc",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "add_newdoc_ufunc"] => Some(Replacement {
                existing: "add_newdoc_ufunc",
                details: Details::Manual {
                    guideline: Some("`add_newdoc_ufunc` is an internal function."),
                },
            }),
            ["numpy", "alltrue"] => Some(Replacement {
                existing: "alltrue",
                details: Details::AutoPurePython {
                    python_expr: "all",
                },
            }),
            ["numpy", "asfarray"] => Some(Replacement {
                existing: "asfarray",
                details: Details::Manual {
                    guideline: Some("Use `np.asarray` with a `float` dtype instead."),
                },
            }),
            ["numpy", "byte_bounds"] => Some(Replacement {
                existing: "byte_bounds",
                details: Details::AutoImport {
                    path: "numpy.lib.array_utils",
                    name: "byte_bounds",
                    compatibility: Compatibility::Breaking,
                },
            }),
            ["numpy", "cast"] => Some(Replacement {
                existing: "cast",
                details: Details::Manual {
                    guideline: Some("Use `np.asarray(arr, dtype=dtype)` instead."),
                },
            }),
            ["numpy", "cfloat"] => Some(Replacement {
                existing: "cfloat",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "complex128",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "clongfloat"] => Some(Replacement {
                existing: "clongfloat",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "clongdouble",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "compat"] => Some(Replacement {
                existing: "compat",
                details: Details::Manual {
                    guideline: Some("Python 2 is no longer supported."),
                },
            }),
            ["numpy", "complex_"] => Some(Replacement {
                existing: "complex_",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "complex128",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "cumproduct"] => Some(Replacement {
                existing: "cumproduct",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "cumprod",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "DataSource"] => Some(Replacement {
                existing: "DataSource",
                details: Details::AutoImport {
                    path: "numpy.lib.npyio",
                    name: "DataSource",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "deprecate"] => Some(Replacement {
                existing: "deprecate",
                details: Details::Manual {
                    guideline: Some("Emit `DeprecationWarning` with `warnings.warn` directly, or use `typing.deprecated`."),
                },
            }),
            ["numpy", "deprecate_with_doc"] => Some(Replacement {
                existing: "deprecate_with_doc",
                details: Details::Manual {
                    guideline: Some("Emit `DeprecationWarning` with `warnings.warn` directly, or use `typing.deprecated`."),
                },
            }),
            ["numpy", "disp"] => Some(Replacement {
                existing: "disp",
                details: Details::Manual {
                    guideline: Some("Use a dedicated print function instead."),
                },
            }),
            ["numpy", "fastCopyAndTranspose"] => Some(Replacement {
                existing: "fastCopyAndTranspose",
                details: Details::Manual {
                    guideline: Some("Use `arr.T.copy()` instead."),
                },
            }),
            ["numpy", "find_common_type"] => Some(Replacement {
                existing: "find_common_type",
                details: Details::Manual {
                    guideline: Some("Use `numpy.promote_types` or `numpy.result_type` instead. To achieve semantics for the `scalar_types` argument, use `numpy.result_type` and pass the Python values `0`, `0.0`, or `0j`."),
                },
            }),
            ["numpy", "get_array_wrap"] => Some(Replacement {
                existing: "get_array_wrap",
                details: Details::Manual {
                    guideline: None,
                },
            }),
            ["numpy", "float_"] => Some(Replacement {
                existing: "float_",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "float64",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "geterrobj"] => Some(Replacement {
                existing: "geterrobj",
                details: Details::Manual {
                    guideline: Some("Use the `np.errstate` context manager instead."),
                },
            }),
            ["numpy", "in1d"] => Some(Replacement {
                existing: "in1d",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "isin",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "INF"] => Some(Replacement {
                existing: "INF",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "inf",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "Inf"] => Some(Replacement {
                existing: "Inf",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "inf",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "Infinity"] => Some(Replacement {
                existing: "Infinity",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "inf",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "infty"] => Some(Replacement {
                existing: "infty",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "inf",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "issctype"] => Some(Replacement {
                existing: "issctype",
                details: Details::Manual {
                    guideline: None,
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
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "mat"] => Some(Replacement {
                existing: "mat",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "asmatrix",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "maximum_sctype"] => Some(Replacement {
                existing: "maximum_sctype",
                details: Details::Manual {
                    guideline: None,
                },
            }),
            ["numpy", existing @ ("NaN" | "NAN")] => Some(Replacement {
                existing,
                details: Details::AutoImport {
                    path: "numpy",
                    name: "nan",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "nbytes"] => Some(Replacement {
                existing: "nbytes",
                details: Details::Manual {
                    guideline: Some("Use `np.dtype(<dtype>).itemsize` instead."),
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
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "longfloat"] => Some(Replacement {
                existing: "longfloat",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "longdouble",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "lookfor"] => Some(Replacement {
                existing: "lookfor",
                details: Details::Manual {
                    guideline: Some("Search NumPy’s documentation directly."),
                },
            }),
            ["numpy", "obj2sctype"] => Some(Replacement {
                existing: "obj2sctype",
                details: Details::Manual {
                    guideline: None,
                },
            }),
            ["numpy", "PINF"] => Some(Replacement {
                existing: "PINF",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "inf",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "product"] => Some(Replacement {
                existing: "product",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "prod",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "PZERO"] => Some(Replacement {
                existing: "PZERO",
                details: Details::AutoPurePython { python_expr: "0.0" },
            }),
            ["numpy", "recfromcsv"] => Some(Replacement {
                existing: "recfromcsv",
                details: Details::Manual {
                    guideline: Some("Use `np.genfromtxt` with comma delimiter instead."),
                },
            }),
            ["numpy", "recfromtxt"] => Some(Replacement {
                existing: "recfromtxt",
                details: Details::Manual {
                    guideline: Some("Use `np.genfromtxt` instead."),
                },
            }),
            ["numpy", "round_"] => Some(Replacement {
                existing: "round_",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "round",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "safe_eval"] => Some(Replacement {
                existing: "safe_eval",
                details: Details::AutoImport {
                    path: "ast",
                    name: "literal_eval",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "sctype2char"] => Some(Replacement {
                existing: "sctype2char",
                details: Details::Manual {
                    guideline: None,
                },
            }),
            ["numpy", "sctypes"] => Some(Replacement {
                existing: "sctypes",
                details: Details::Manual {
                    guideline: None,
                },
            }),
            ["numpy", "seterrobj"] => Some(Replacement {
                existing: "seterrobj",
                details: Details::Manual {
                    guideline: Some("Use the `np.errstate` context manager instead."),
                },
            }),
            ["numpy", "set_string_function"] => Some(Replacement {
                existing: "set_string_function",
                details: Details::Manual {
                    guideline: Some("Use `np.set_printoptions` for custom printing of NumPy objects."),
                },
            }),
            ["numpy", "singlecomplex"] => Some(Replacement {
                existing: "singlecomplex",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "complex64",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "string_"] => Some(Replacement {
                existing: "string_",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "bytes_",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "sometrue"] => Some(Replacement {
                existing: "sometrue",
                details: Details::AutoPurePython {
                    python_expr: "any",
                },
            }),
            ["numpy", "source"] => Some(Replacement {
                existing: "source",
                details: Details::AutoImport {
                    path: "inspect",
                    name: "getsource",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "tracemalloc_domain"] => Some(Replacement {
                existing: "tracemalloc_domain",
                details: Details::AutoImport {
                    path: "numpy.lib",
                    name: "tracemalloc_domain",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "trapz"] => Some(Replacement {
                existing: "trapz",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "trapezoid",
                    compatibility: Compatibility::Breaking,
                },
            }),
            ["numpy", "unicode_"] => Some(Replacement {
                existing: "unicode_",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "str_",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "who"] => Some(Replacement {
                existing: "who",
                details: Details::Manual {
                    guideline: Some("Use an IDE variable explorer or `locals()` instead."),
                },
            }),
            ["numpy", "row_stack"] => Some(Replacement {
                existing: "row_stack",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "vstack",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "AxisError"] => Some(Replacement {
                existing: "AxisError",
                details: Details::AutoImport {
                    path: "numpy.exceptions",
                    name: "AxisError",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "ComplexWarning"] => Some(Replacement {
                existing: "ComplexWarning",
                details: Details::AutoImport {
                    path: "numpy.exceptions",
                    name: "ComplexWarning",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "DTypePromotionError"] => Some(Replacement {
                existing: "DTypePromotionError",
                details: Details::AutoImport {
                    path: "numpy.exceptions",
                    name: "DTypePromotionError",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "ModuleDeprecationWarning"] => Some(Replacement {
                existing: "ModuleDeprecationWarning",
                details: Details::AutoImport {
                    path: "numpy.exceptions",
                    name: "ModuleDeprecationWarning",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "RankWarning"] => Some(Replacement {
                existing: "RankWarning",
                details: Details::AutoImport {
                    path: "numpy.exceptions",
                    name: "RankWarning",
                    compatibility: Compatibility::Breaking,
                },
            }),
            ["numpy", "TooHardError"] => Some(Replacement {
                existing: "TooHardError",
                details: Details::AutoImport {
                    path: "numpy.exceptions",
                    name: "TooHardError",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "VisibleDeprecationWarning"] => Some(Replacement {
                existing: "VisibleDeprecationWarning",
                details: Details::AutoImport {
                    path: "numpy.exceptions",
                    name: "VisibleDeprecationWarning",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "compare_chararrays"] => Some(Replacement {
                existing: "compare_chararrays",
                details: Details::AutoImport {
                    path: "numpy.char",
                    name: "compare_chararrays",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "chararray"] => Some(Replacement {
                existing: "chararray",
                details: Details::AutoImport {
                    path: "numpy.char",
                    name: "chararray",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            ["numpy", "format_parser"] => Some(Replacement {
                existing: "format_parser",
                details: Details::AutoImport {
                    path: "numpy.rec",
                    name: "format_parser",
                    compatibility: Compatibility::BackwardsCompatible,
                },
            }),
            _ => None,
        });

    if let Some(replacement) = maybe_replacement {
        let mut diagnostic = Diagnostic::new(
            Numpy2Deprecation {
                existing: replacement.existing.to_string(),
                migration_guide: replacement.details.guideline(),
                code_action: replacement.details.code_action(),
            },
            expr.range(),
        );
        match replacement.details {
            Details::AutoImport {
                path,
                name,
                compatibility,
            } => {
                diagnostic.try_set_fix(|| {
                    let (import_edit, binding) = checker.importer().get_or_import_symbol(
                        &ImportRequest::import_from(path, name),
                        expr.start(),
                        checker.semantic(),
                    )?;
                    let replacement_edit = Edit::range_replacement(binding, expr.range());
                    Ok(match compatibility {
                        Compatibility::BackwardsCompatible => {
                            Fix::safe_edits(import_edit, [replacement_edit])
                        }
                        Compatibility::Breaking => {
                            Fix::unsafe_edits(import_edit, [replacement_edit])
                        }
                    })
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
