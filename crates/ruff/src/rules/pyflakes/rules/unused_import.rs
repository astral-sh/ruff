use itertools::Itertools;
use ruff_text_size::TextRange;
use rustc_hash::FxHashMap;
use rustpython_parser::ast::Ranged;

use ruff_diagnostics::{AutofixKind, Diagnostic, Fix, IsolationLevel, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::binding::Exceptions;
use ruff_python_semantic::node::NodeId;
use ruff_python_semantic::scope::Scope;

use crate::autofix;
use crate::checkers::ast::Checker;
use crate::registry::Rule;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum UnusedImportContext {
    ExceptHandler,
    Init,
}

/// ## What it does
/// Checks for unused imports.
///
/// ## Why is this bad?
/// Unused imports add a performance overhead at runtime, and risk creating
/// import cycles. They also increase the cognitive load of reading the code.
///
/// If an import statement is used to check for the availability or existence
/// of a module, consider using `importlib.util.find_spec` instead.
///
/// ## Options
///
/// - `pyflakes.extend-generics`
///
/// ## Example
/// ```python
/// import numpy as np  # unused import
///
///
/// def area(radius):
///     return 3.14 * radius**2
/// ```
///
/// Use instead:
/// ```python
/// def area(radius):
///     return 3.14 * radius**2
/// ```
///
/// To check the availability of a module, use `importlib.util.find_spec`:
/// ```python
/// from importlib.util import find_spec
///
/// if find_spec("numpy") is not None:
///     print("numpy is installed")
/// else:
///     print("numpy is not installed")
/// ```
///
/// ## References
/// - [Python documentation: `import`](https://docs.python.org/3/reference/simple_stmts.html#the-import-statement)
/// - [Python documentation: `importlib.util.find_spec`](https://docs.python.org/3/library/importlib.html#importlib.util.find_spec)
#[violation]
pub struct UnusedImport {
    name: String,
    context: Option<UnusedImportContext>,
    multiple: bool,
}

impl Violation for UnusedImport {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedImport { name, context, .. } = self;
        match context {
            Some(UnusedImportContext::ExceptHandler) => {
                format!(
                    "`{name}` imported but unused; consider using `importlib.util.find_spec` to test for availability"
                )
            }
            Some(UnusedImportContext::Init) => {
                format!(
                    "`{name}` imported but unused; consider adding to `__all__` or using a redundant \
                     alias"
                )
            }
            None => format!("`{name}` imported but unused"),
        }
    }

    fn autofix_title(&self) -> Option<String> {
        let UnusedImport { name, multiple, .. } = self;
        Some(if *multiple {
            "Remove unused import".to_string()
        } else {
            format!("Remove unused import: `{name}`")
        })
    }
}

type SpannedName<'a> = (&'a str, &'a TextRange);
type BindingContext<'a> = (NodeId, Option<NodeId>, Exceptions);

pub(crate) fn unused_import(checker: &Checker, scope: &Scope, diagnostics: &mut Vec<Diagnostic>) {
    // Collect all unused imports by statement.
    let mut unused: FxHashMap<BindingContext, Vec<SpannedName>> = FxHashMap::default();
    let mut ignored: FxHashMap<BindingContext, Vec<SpannedName>> = FxHashMap::default();

    for binding_id in scope.binding_ids() {
        let binding = &checker.semantic_model().bindings[binding_id];

        if binding.is_used() || binding.is_explicit_export() {
            continue;
        }

        let Some(qualified_name) = binding.qualified_name() else {
            continue;
        };

        let stmt_id = binding.source.unwrap();
        let parent_id = checker.semantic_model().stmts.parent_id(stmt_id);

        let exceptions = binding.exceptions;
        let diagnostic_offset = binding.range.start();
        let stmt = &checker.semantic_model().stmts[stmt_id];
        let parent_offset = if stmt.is_import_from_stmt() {
            Some(stmt.start())
        } else {
            None
        };

        if checker.rule_is_ignored(Rule::UnusedImport, diagnostic_offset)
            || parent_offset.map_or(false, |parent_offset| {
                checker.rule_is_ignored(Rule::UnusedImport, parent_offset)
            })
        {
            ignored
                .entry((stmt_id, parent_id, exceptions))
                .or_default()
                .push((qualified_name, &binding.range));
        } else {
            unused
                .entry((stmt_id, parent_id, exceptions))
                .or_default()
                .push((qualified_name, &binding.range));
        }
    }

    let in_init =
        checker.settings.ignore_init_module_imports && checker.path().ends_with("__init__.py");

    // Generate a diagnostic for every unused import, but share a fix across all unused imports
    // within the same statement (excluding those that are ignored).
    for ((stmt_id, parent_id, exceptions), unused_imports) in unused
        .into_iter()
        .sorted_by_key(|((defined_by, ..), ..)| *defined_by)
    {
        let stmt = checker.semantic_model().stmts[stmt_id];
        let parent = parent_id.map(|parent_id| checker.semantic_model().stmts[parent_id]);
        let multiple = unused_imports.len() > 1;
        let in_except_handler =
            exceptions.intersects(Exceptions::MODULE_NOT_FOUND_ERROR | Exceptions::IMPORT_ERROR);

        let fix = if !in_init && !in_except_handler && checker.patch(Rule::UnusedImport) {
            autofix::edits::remove_unused_imports(
                unused_imports
                    .iter()
                    .map(|(qualified_name, _)| *qualified_name),
                stmt,
                parent,
                checker.locator,
                checker.indexer,
                checker.stylist,
            )
            .ok()
        } else {
            None
        };

        for (qualified_name, range) in unused_imports {
            let mut diagnostic = Diagnostic::new(
                UnusedImport {
                    name: qualified_name.to_string(),
                    context: if in_except_handler {
                        Some(UnusedImportContext::ExceptHandler)
                    } else if in_init {
                        Some(UnusedImportContext::Init)
                    } else {
                        None
                    },
                    multiple,
                },
                *range,
            );
            if stmt.is_import_from_stmt() {
                diagnostic.set_parent(stmt.start());
            }
            if let Some(edit) = fix.as_ref() {
                diagnostic.set_fix(Fix::automatic(edit.clone()).isolate(
                    parent_id.map_or(IsolationLevel::default(), |node_id| {
                        IsolationLevel::Group(node_id.into())
                    }),
                ));
            }
            diagnostics.push(diagnostic);
        }
    }

    // Separately, generate a diagnostic for every _ignored_ unused import, but don't bother
    // creating a fix. We have to generate these diagnostics, even though they'll be ignored later
    // on, so that the suppression comments themselves aren't marked as unnecessary.
    for ((stmt_id, .., exceptions), unused_imports) in ignored
        .into_iter()
        .sorted_by_key(|((stmt_id, ..), ..)| *stmt_id)
    {
        let stmt = checker.semantic_model().stmts[stmt_id];
        let multiple = unused_imports.len() > 1;
        let in_except_handler =
            exceptions.intersects(Exceptions::MODULE_NOT_FOUND_ERROR | Exceptions::IMPORT_ERROR);
        for (qualified_name, range) in unused_imports {
            let mut diagnostic = Diagnostic::new(
                UnusedImport {
                    name: qualified_name.to_string(),
                    context: if in_except_handler {
                        Some(UnusedImportContext::ExceptHandler)
                    } else if in_init {
                        Some(UnusedImportContext::Init)
                    } else {
                        None
                    },
                    multiple,
                },
                *range,
            );
            if stmt.is_import_from_stmt() {
                diagnostic.set_parent(stmt.start());
            }
            diagnostics.push(diagnostic);
        }
    }
}
