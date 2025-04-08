use crate::rules::numpy::helpers::ImportSearcher;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::name::{QualifiedName, QualifiedNameBuilder};
use ruff_python_ast::statement_visitor::StatementVisitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::{Exceptions, Modules, SemanticModel};
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
/// ## Example
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
#[derive(ViolationMetadata)]
pub(crate) struct Numpy2Deprecation {
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
pub(crate) fn numpy_2_0_deprecation(checker: &Checker, expr: &Expr) {
    let semantic = checker.semantic();

    if !semantic.seen_module(Modules::NUMPY) {
        return;
    }

    let Some(qualified_name) = semantic.resolve_qualified_name(expr) else {
        return;
    };

    let replacement = match qualified_name.segments() {
        // NumPy's main namespace np.* members removed in 2.0
        ["numpy", "add_docstring"] => Replacement {
            existing: "add_docstring",
            details: Details::AutoImport {
                path: "numpy.lib",
                name: "add_docstring",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "add_newdoc"] => Replacement {
            existing: "add_newdoc",
            details: Details::AutoImport {
                path: "numpy.lib",
                name: "add_newdoc",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "add_newdoc_ufunc"] => Replacement {
            existing: "add_newdoc_ufunc",
            details: Details::Manual {
                guideline: Some("`add_newdoc_ufunc` is an internal function."),
            },
        },
        ["numpy", "alltrue"] => Replacement {
            existing: "alltrue",
            details: Details::AutoImport {
                path: "numpy",
                name: "all",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "asfarray"] => Replacement {
            existing: "asfarray",
            details: Details::Manual {
                guideline: Some("Use `np.asarray` with a `float` dtype instead."),
            },
        },
        ["numpy", "byte_bounds"] => Replacement {
            existing: "byte_bounds",
            details: Details::AutoImport {
                path: "numpy.lib.array_utils",
                name: "byte_bounds",
                compatibility: Compatibility::Breaking,
            },
        },
        ["numpy", "cast"] => Replacement {
            existing: "cast",
            details: Details::Manual {
                guideline: Some("Use `np.asarray(arr, dtype=dtype)` instead."),
            },
        },
        ["numpy", "cfloat"] => Replacement {
            existing: "cfloat",
            details: Details::AutoImport {
                path: "numpy",
                name: "complex128",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "clongfloat"] => Replacement {
            existing: "clongfloat",
            details: Details::AutoImport {
                path: "numpy",
                name: "clongdouble",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "compat"] => Replacement {
            existing: "compat",
            details: Details::Manual {
                guideline: Some("Python 2 is no longer supported."),
            },
        },
        ["numpy", "complex_"] => Replacement {
            existing: "complex_",
            details: Details::AutoImport {
                path: "numpy",
                name: "complex128",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "cumproduct"] => Replacement {
            existing: "cumproduct",
            details: Details::AutoImport {
                path: "numpy",
                name: "cumprod",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "DataSource"] => Replacement {
            existing: "DataSource",
            details: Details::AutoImport {
                path: "numpy.lib.npyio",
                name: "DataSource",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "deprecate"] => Replacement {
            existing: "deprecate",
            details: Details::Manual {
                guideline: Some("Emit `DeprecationWarning` with `warnings.warn` directly, or use `typing.deprecated`."),
            },
        },
        ["numpy", "deprecate_with_doc"] => Replacement {
            existing: "deprecate_with_doc",
            details: Details::Manual {
                guideline: Some("Emit `DeprecationWarning` with `warnings.warn` directly, or use `typing.deprecated`."),
            },
        },
        ["numpy", "disp"] => Replacement {
            existing: "disp",
            details: Details::Manual {
                guideline: Some("Use a dedicated print function instead."),
            },
        },
        ["numpy", "fastCopyAndTranspose"] => Replacement {
            existing: "fastCopyAndTranspose",
            details: Details::Manual {
                guideline: Some("Use `arr.T.copy()` instead."),
            },
        },
        ["numpy", "find_common_type"] => Replacement {
            existing: "find_common_type",
            details: Details::Manual {
                guideline: Some("Use `numpy.promote_types` or `numpy.result_type` instead. To achieve semantics for the `scalar_types` argument, use `numpy.result_type` and pass the Python values `0`, `0.0`, or `0j`."),
            },
        },
        ["numpy", "get_array_wrap"] => Replacement {
            existing: "get_array_wrap",
            details: Details::Manual {
                guideline: None,
            },
        },
        ["numpy", "float_"] => Replacement {
            existing: "float_",
            details: Details::AutoImport {
                path: "numpy",
                name: "float64",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "geterrobj"] => Replacement {
            existing: "geterrobj",
            details: Details::Manual {
                guideline: Some("Use the `np.errstate` context manager instead."),
            },
        },
        ["numpy", "in1d"] => Replacement {
            existing: "in1d",
            details: Details::AutoImport {
                path: "numpy",
                name: "isin",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "INF"] => Replacement {
            existing: "INF",
            details: Details::AutoImport {
                path: "numpy",
                name: "inf",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "Inf"] => Replacement {
            existing: "Inf",
            details: Details::AutoImport {
                path: "numpy",
                name: "inf",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "Infinity"] => Replacement {
            existing: "Infinity",
            details: Details::AutoImport {
                path: "numpy",
                name: "inf",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "infty"] => Replacement {
            existing: "infty",
            details: Details::AutoImport {
                path: "numpy",
                name: "inf",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "issctype"] => Replacement {
            existing: "issctype",
            details: Details::Manual {
                guideline: None,
            },
        },
        ["numpy", "issubclass_"] => Replacement {
            existing: "issubclass_",
            details: Details::AutoPurePython {
                python_expr: "issubclass",
            },
        },
        ["numpy", "issubsctype"] => Replacement {
            existing: "issubsctype",
            details: Details::AutoImport {
                path: "numpy",
                name: "issubdtype",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "mat"] => Replacement {
            existing: "mat",
            details: Details::AutoImport {
                path: "numpy",
                name: "asmatrix",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "maximum_sctype"] => Replacement {
            existing: "maximum_sctype",
            details: Details::Manual {
                guideline: None,
            },
        },
        ["numpy", existing @ ("NaN" | "NAN")] => Replacement {
            existing,
            details: Details::AutoImport {
                path: "numpy",
                name: "nan",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "nbytes"] => Replacement {
            existing: "nbytes",
            details: Details::Manual {
                guideline: Some("Use `np.dtype(<dtype>).itemsize` instead."),
            },
        },
        ["numpy", "NINF"] => Replacement {
            existing: "NINF",
            details: Details::AutoPurePython {
                python_expr: "-np.inf",
            },
        },
        ["numpy", "NZERO"] => Replacement {
            existing: "NZERO",
            details: Details::AutoPurePython {
                python_expr: "-0.0",
            },
        },
        ["numpy", "longcomplex"] => Replacement {
            existing: "longcomplex",
            details: Details::AutoImport {
                path: "numpy",
                name: "clongdouble",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "longfloat"] => Replacement {
            existing: "longfloat",
            details: Details::AutoImport {
                path: "numpy",
                name: "longdouble",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "lookfor"] => Replacement {
            existing: "lookfor",
            details: Details::Manual {
                guideline: Some("Search NumPy’s documentation directly."),
            },
        },
        ["numpy", "obj2sctype"] => Replacement {
            existing: "obj2sctype",
            details: Details::Manual {
                guideline: None,
            },
        },
        ["numpy", "PINF"] => Replacement {
            existing: "PINF",
            details: Details::AutoImport {
                path: "numpy",
                name: "inf",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "product"] => Replacement {
            existing: "product",
            details: Details::AutoImport {
                path: "numpy",
                name: "prod",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "PZERO"] => Replacement {
            existing: "PZERO",
            details: Details::AutoPurePython { python_expr: "0.0" },
        },
        ["numpy", "recfromcsv"] => Replacement {
            existing: "recfromcsv",
            details: Details::Manual {
                guideline: Some("Use `np.genfromtxt` with comma delimiter instead."),
            },
        },
        ["numpy", "recfromtxt"] => Replacement {
            existing: "recfromtxt",
            details: Details::Manual {
                guideline: Some("Use `np.genfromtxt` instead."),
            },
        },
        ["numpy", "round_"] => Replacement {
            existing: "round_",
            details: Details::AutoImport {
                path: "numpy",
                name: "round",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "safe_eval"] => Replacement {
            existing: "safe_eval",
            details: Details::AutoImport {
                path: "ast",
                name: "literal_eval",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "sctype2char"] => Replacement {
            existing: "sctype2char",
            details: Details::Manual {
                guideline: None,
            },
        },
        ["numpy", "sctypes"] => Replacement {
            existing: "sctypes",
            details: Details::Manual {
                guideline: None,
            },
        },
        ["numpy", "seterrobj"] => Replacement {
            existing: "seterrobj",
            details: Details::Manual {
                guideline: Some("Use the `np.errstate` context manager instead."),
            },
        },
        ["numpy", "set_string_function"] => Replacement {
            existing: "set_string_function",
            details: Details::Manual {
                guideline: Some("Use `np.set_printoptions` for custom printing of NumPy objects."),
            },
        },
        ["numpy", "singlecomplex"] => Replacement {
            existing: "singlecomplex",
            details: Details::AutoImport {
                path: "numpy",
                name: "complex64",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "string_"] => Replacement {
            existing: "string_",
            details: Details::AutoImport {
                path: "numpy",
                name: "bytes_",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "sometrue"] => Replacement {
            existing: "sometrue",
            details: Details::AutoImport {
                path: "numpy",
                name: "any",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "source"] => Replacement {
            existing: "source",
            details: Details::AutoImport {
                path: "inspect",
                name: "getsource",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "tracemalloc_domain"] => Replacement {
            existing: "tracemalloc_domain",
            details: Details::AutoImport {
                path: "numpy.lib",
                name: "tracemalloc_domain",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "trapz"] => Replacement {
            existing: "trapz",
            details: Details::AutoImport {
                path: "numpy",
                name: "trapezoid",
                compatibility: Compatibility::Breaking,
            },
        },
        ["numpy", "unicode_"] => Replacement {
            existing: "unicode_",
            details: Details::AutoImport {
                path: "numpy",
                name: "str_",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "who"] => Replacement {
            existing: "who",
            details: Details::Manual {
                guideline: Some("Use an IDE variable explorer or `locals()` instead."),
            },
        },
        ["numpy", "row_stack"] => Replacement {
            existing: "row_stack",
            details: Details::AutoImport {
                path: "numpy",
                name: "vstack",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "AxisError"] => Replacement {
            existing: "AxisError",
            details: Details::AutoImport {
                path: "numpy.exceptions",
                name: "AxisError",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "ComplexWarning"] => Replacement {
            existing: "ComplexWarning",
            details: Details::AutoImport {
                path: "numpy.exceptions",
                name: "ComplexWarning",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "DTypePromotionError"] => Replacement {
            existing: "DTypePromotionError",
            details: Details::AutoImport {
                path: "numpy.exceptions",
                name: "DTypePromotionError",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "ModuleDeprecationWarning"] => Replacement {
            existing: "ModuleDeprecationWarning",
            details: Details::AutoImport {
                path: "numpy.exceptions",
                name: "ModuleDeprecationWarning",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "RankWarning"] => Replacement {
            existing: "RankWarning",
            details: Details::AutoImport {
                path: "numpy.exceptions",
                name: "RankWarning",
                compatibility: Compatibility::Breaking,
            },
        },
        ["numpy", "TooHardError"] => Replacement {
            existing: "TooHardError",
            details: Details::AutoImport {
                path: "numpy.exceptions",
                name: "TooHardError",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "VisibleDeprecationWarning"] => Replacement {
            existing: "VisibleDeprecationWarning",
            details: Details::AutoImport {
                path: "numpy.exceptions",
                name: "VisibleDeprecationWarning",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "compare_chararrays"] => Replacement {
            existing: "compare_chararrays",
            details: Details::AutoImport {
                path: "numpy.char",
                name: "compare_chararrays",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "chararray"] => Replacement {
            existing: "chararray",
            details: Details::AutoImport {
                path: "numpy.char",
                name: "chararray",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        ["numpy", "format_parser"] => Replacement {
            existing: "format_parser",
            details: Details::AutoImport {
                path: "numpy.rec",
                name: "format_parser",
                compatibility: Compatibility::BackwardsCompatible,
            },
        },
        _ => return,
    };

    if is_guarded_by_try_except(expr, &replacement, semantic) {
        return;
    }

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
                    Compatibility::Breaking => Fix::unsafe_edits(import_edit, [replacement_edit]),
                })
            });
        }
        Details::AutoPurePython { python_expr } => diagnostic.set_fix(Fix::safe_edit(
            Edit::range_replacement(python_expr.to_string(), expr.range()),
        )),
        Details::Manual { guideline: _ } => {}
    }
    checker.report_diagnostic(diagnostic);
}

/// Ignore attempts to access a `numpy` member via its deprecated name
/// if the access takes place in an `except` block that provides compatibility
/// with older numpy versions.
///
/// For attribute accesses (e.g. `np.ComplexWarning`), we only ignore the violation
/// if it's inside an `except AttributeError` block, and the member is accessed
/// through its non-deprecated name in the associated `try` block.
///
/// For uses of the `numpy` member where it's simply an `ExprName` node,
/// we check to see how the `numpy` member was bound. If it was bound via a
/// `from numpy import foo` statement, we check to see if that import statement
/// took place inside an `except ImportError` or `except ModuleNotFoundError` block.
/// If so, and if the `numpy` member was imported through its non-deprecated name
/// in the associated try block, we ignore the violation in the same way.
///
/// Examples:
///
/// ```py
/// import numpy as np
///
/// try:
///     np.all([True, True])
/// except AttributeError:
///     np.alltrue([True, True])  # Okay
///
/// try:
///     from numpy.exceptions import ComplexWarning
/// except ImportError:
///     from numpy import ComplexWarning
///
/// x = ComplexWarning()  # Okay
/// ```
fn is_guarded_by_try_except(
    expr: &Expr,
    replacement: &Replacement,
    semantic: &SemanticModel,
) -> bool {
    match expr {
        Expr::Attribute(_) => {
            if !semantic.in_exception_handler() {
                return false;
            }
            let Some(try_node) = semantic
                .current_statements()
                .find_map(|stmt| stmt.as_try_stmt())
            else {
                return false;
            };
            let suspended_exceptions = Exceptions::from_try_stmt(try_node, semantic);
            if !suspended_exceptions.contains(Exceptions::ATTRIBUTE_ERROR) {
                return false;
            }
            try_block_contains_undeprecated_attribute(try_node, &replacement.details, semantic)
        }
        Expr::Name(ast::ExprName { id, .. }) => {
            let Some(binding_id) = semantic.lookup_symbol(id.as_str()) else {
                return false;
            };
            let binding = semantic.binding(binding_id);
            if !binding.is_external() {
                return false;
            }
            if !binding.in_exception_handler() {
                return false;
            }
            let Some(try_node) = binding.source.and_then(|import_id| {
                semantic
                    .statements(import_id)
                    .find_map(|stmt| stmt.as_try_stmt())
            }) else {
                return false;
            };
            let suspended_exceptions = Exceptions::from_try_stmt(try_node, semantic);
            if !suspended_exceptions
                .intersects(Exceptions::IMPORT_ERROR | Exceptions::MODULE_NOT_FOUND_ERROR)
            {
                return false;
            }
            try_block_contains_undeprecated_import(try_node, &replacement.details)
        }
        _ => false,
    }
}

/// Given an [`ast::StmtTry`] node, does the `try` branch of that node
/// contain any [`ast::ExprAttribute`] nodes that indicate the numpy
/// member is being accessed from the non-deprecated location?
fn try_block_contains_undeprecated_attribute(
    try_node: &ast::StmtTry,
    replacement_details: &Details,
    semantic: &SemanticModel,
) -> bool {
    let Details::AutoImport {
        path,
        name,
        compatibility: _,
    } = replacement_details
    else {
        return false;
    };
    let undeprecated_qualified_name = {
        let mut builder = QualifiedNameBuilder::default();
        for part in path.split('.') {
            builder.push(part);
        }
        builder.push(name);
        builder.build()
    };
    let mut attribute_searcher = AttributeSearcher::new(undeprecated_qualified_name, semantic);
    attribute_searcher.visit_body(&try_node.body);
    attribute_searcher.found_attribute
}

/// AST visitor that searches an AST tree for [`ast::ExprAttribute`] nodes
/// that match a certain [`QualifiedName`].
struct AttributeSearcher<'a> {
    attribute_to_find: QualifiedName<'a>,
    semantic: &'a SemanticModel<'a>,
    found_attribute: bool,
}

impl<'a> AttributeSearcher<'a> {
    fn new(attribute_to_find: QualifiedName<'a>, semantic: &'a SemanticModel<'a>) -> Self {
        Self {
            attribute_to_find,
            semantic,
            found_attribute: false,
        }
    }
}

impl Visitor<'_> for AttributeSearcher<'_> {
    fn visit_expr(&mut self, expr: &'_ Expr) {
        if self.found_attribute {
            return;
        }
        if expr.is_attribute_expr()
            && self
                .semantic
                .resolve_qualified_name(expr)
                .is_some_and(|qualified_name| qualified_name == self.attribute_to_find)
        {
            self.found_attribute = true;
            return;
        }
        ast::visitor::walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &ruff_python_ast::Stmt) {
        if !self.found_attribute {
            ast::visitor::walk_stmt(self, stmt);
        }
    }

    fn visit_body(&mut self, body: &[ruff_python_ast::Stmt]) {
        for stmt in body {
            self.visit_stmt(stmt);
            if self.found_attribute {
                return;
            }
        }
    }
}

/// Given an [`ast::StmtTry`] node, does the `try` branch of that node
/// contain any [`ast::StmtImportFrom`] nodes that indicate the numpy
/// member is being imported from the non-deprecated location?
fn try_block_contains_undeprecated_import(
    try_node: &ast::StmtTry,
    replacement_details: &Details,
) -> bool {
    let Details::AutoImport {
        path,
        name,
        compatibility: _,
    } = replacement_details
    else {
        return false;
    };
    let mut import_searcher = ImportSearcher::new(path, name);
    import_searcher.visit_body(&try_node.body);
    import_searcher.found_import
}
