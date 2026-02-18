use ruff_diagnostics::Applicability;
use ruff_python_ast::{self as ast, ExceptHandler, Expr, ExprContext};
use ruff_text_size::{Ranged, TextRange};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::{Name, UnqualifiedName};
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;
use crate::fix::edits::pad;
use crate::importer::ImportRequest;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for uses of deprecated `zipfile.BadZipfile` that is aliased as `zipfile.BadZipFile`.
///
/// ## Why is this bad?
/// `zipfile.BadZipfile` is deprecated since version 3.2 and may be removed in future versions
///
/// ## Example
/// ```python
/// raise zipfile.BadZipfile
/// ```
///
/// Use instead:
/// ```python
/// raise zipfile.BadZipFile
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe if it would delete any comments
/// within the exception expression range.
///
/// ## References
/// - [Python documentation: `BadZipfile`](https://docs.python.org/3/library/zipfile.html#zipfile.BadZipfile)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct BadZipFileAlias {
    name: Option<String>,
}

impl AlwaysFixableViolation for BadZipFileAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Replace aliased error with `BadZipFile`".to_string()
    }

    fn fix_title(&self) -> String {
        let BadZipFileAlias { name } = self;
        match name {
            None => "Replace with `zipfile.BadZipFile`".to_string(),
            Some(name) => format!("Replace `{name}` with `zipfile.BadZipFile`"),
        }
    }
}

/// Return `true` if an [`Expr`] is an alias of `BadZipFile`.
fn is_alias(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(expr)
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["zipfile", "BadZipfile"])
        })
}

/// Create a [`Diagnostic`] for a single target, like an [`Expr::Name`].
fn atom_diagnostic(checker: &Checker, target: &Expr) {
    let mut diagnostic = checker.report_diagnostic(
        BadZipFileAlias {
            name: UnqualifiedName::from_expr(target).map(|name| name.to_string()),
        },
        target.range(),
    );
    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import_from("zipfile", "BadZipFile"),
            target.start(),
            checker.semantic(),
        )?;

        let applicability = if checker.comment_ranges().intersects(target.range()) {
            Applicability::Unsafe
        } else {
            Applicability::Safe
        };

        Ok(Fix::applicable_edits(
            Edit::range_replacement(binding, target.range()),
            [import_edit],
            applicability,
        ))
    });
}

/// Create a [`Diagnostic`] for a tuple of expressions.
fn tuple_diagnostic(checker: &Checker, tuple: &ast::ExprTuple, aliases: &[&Expr]) {
    let mut diagnostic = checker.report_diagnostic(BadZipFileAlias { name: None }, tuple.range());
    let semantic = checker.semantic();

    let applicability = if checker.comment_ranges().intersects(tuple.range()) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import_from("zipfile", "BadZipFile"),
            tuple.start(),
            checker.semantic(),
        )?;

        // Filter out any `BadZipFile` aliases.
        let mut remaining: Vec<Expr> = tuple
            .iter()
            .filter_map(|element| {
                if aliases.contains(&element) {
                    None
                } else {
                    Some(element.clone())
                }
            })
            .collect();

        // If `BadZipFile` itself isn't already in the tuple, add it.
        // Use the binding name from get_or_import_symbol, which handles existing imports correctly.
        if tuple.iter().all(|element| {
            semantic
                .resolve_qualified_name(element)
                .map(|qn| qn.segments() != ["zipfile", "BadZipFile"])
                .unwrap_or(true)
        }) {
            let node = ast::ExprName {
                id: Name::new(&binding),
                ctx: ExprContext::Load,
                range: TextRange::default(),
                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
            };
            remaining.insert(0, node.into());
        }

        let content = if remaining.len() == 1 {
            binding.clone()
        } else {
            let node = ast::ExprTuple {
                elts: remaining,
                ctx: ExprContext::Load,
                range: TextRange::default(),
                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
                parenthesized: true,
            };
            format!("({})", checker.generator().expr(&node.into()))
        };

        Ok(Fix::applicable_edits(
            Edit::range_replacement(
                pad(content, tuple.range(), checker.locator()),
                tuple.range(),
            ),
            [import_edit],
            applicability,
        ))
    });
}

/// UP051
pub(crate) fn badzipfile_alias_handlers(checker: &Checker, handlers: &[ExceptHandler]) {
    for handler in handlers {
        let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { type_, .. }) = handler;
        let Some(expr) = type_.as_ref() else {
            continue;
        };
        match expr.as_ref() {
            Expr::Name(_) | Expr::Attribute(_) => {
                if is_alias(expr, checker.semantic()) {
                    atom_diagnostic(checker, expr);
                }
            }
            Expr::Tuple(tuple) => {
                // List of aliases to replace with `BadZipFile`.
                let mut aliases: Vec<&Expr> = vec![];
                for element in tuple {
                    if is_alias(element, checker.semantic()) {
                        aliases.push(element);
                    }
                }
                if !aliases.is_empty() {
                    tuple_diagnostic(checker, tuple, &aliases);
                }
            }
            _ => {}
        }
    }
}

/// UP051
pub(crate) fn badzipfile_alias_call(checker: &Checker, func: &Expr) {
    if is_alias(func, checker.semantic()) {
        atom_diagnostic(checker, func);
    }
}

/// UP051
pub(crate) fn badzipfile_alias_raise(checker: &Checker, expr: &Expr) {
    if matches!(expr, Expr::Name(_) | Expr::Attribute(_)) {
        if is_alias(expr, checker.semantic()) {
            atom_diagnostic(checker, expr);
        }
    }
}
