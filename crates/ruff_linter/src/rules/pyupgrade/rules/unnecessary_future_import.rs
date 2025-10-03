use itertools::{Itertools, chain};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::{QualifiedName, QualifiedNameBuilder};
use ruff_python_ast::{self as ast, Alias, Stmt, StmtRef};
use ruff_python_semantic::NodeId;
use ruff_python_semantic::{NameImport, Scope};
use ruff_text_size::{Ranged, TextRange};
use std::collections::{BTreeSet, HashMap};

use crate::checkers::ast::Checker;
use crate::fix;
use crate::preview::is_separate_unused_import_diag_enabled;
use crate::{AlwaysFixableViolation, Applicability, Fix};

/// ## What it does
/// Checks for unnecessary `__future__` imports.
///
/// ## Why is this bad?
/// The `__future__` module is used to enable features that are not yet
/// available in the current Python version. If a feature is already
/// available in the minimum supported Python version, importing it
/// from `__future__` is unnecessary and should be removed to avoid
/// confusion.
///
/// ## Example
/// ```python
/// from __future__ import print_function
///
/// print("Hello, world!")
/// ```
///
/// Use instead:
/// ```python
/// print("Hello, world!")
/// ```
///
/// ## Preview
///
/// When [preview] is enabled, this rule underlines each unused import individually
/// instead of grouping them together if there are two or more within an import statement.
///
/// ## Fix safety
/// This fix is marked unsafe if applying it would delete a comment.
///
/// ## Options
/// - `target-version`
///
/// ## References
/// - [Python documentation: `__future__` — Future statement definitions](https://docs.python.org/3/library/__future__.html)
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryFutureImport {
    pub names: Vec<String>,
}

impl AlwaysFixableViolation for UnnecessaryFutureImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryFutureImport { names } = self;
        if names.len() == 1 {
            let import = &names[0];
            format!("Unnecessary `__future__` import `{import}` for target Python version")
        } else {
            let imports = names.iter().map(|name| format!("`{name}`")).join(", ");
            format!("Unnecessary `__future__` imports {imports} for target Python version")
        }
    }

    fn fix_title(&self) -> String {
        "Remove unnecessary `__future__` import".to_string()
    }
}

const PY33_PLUS_REMOVE_FUTURES: &[&str] = &[
    "nested_scopes",
    "generators",
    "with_statement",
    "division",
    "absolute_import",
    "with_statement",
    "print_function",
    "unicode_literals",
];

const PY37_PLUS_REMOVE_FUTURES: &[&str] = &[
    "nested_scopes",
    "generators",
    "with_statement",
    "division",
    "absolute_import",
    "with_statement",
    "print_function",
    "unicode_literals",
    "generator_stop",
];

pub(crate) type RequiredImports = BTreeSet<NameImport>;

pub(crate) fn is_import_required_by_isort(
    required_imports: &RequiredImports,
    stmt: StmtRef,
    alias: &Alias,
) -> bool {
    match stmt {
        StmtRef::ImportFrom(ast::StmtImportFrom {
            module: Some(module),
            ..
        }) => {
            let mut builder = QualifiedNameBuilder::with_capacity(module.split('.').count() + 1);
            builder.extend(module.split('.'));
            builder.push(alias.name.as_str());
            let qualified = builder.build();

            required_imports
                .iter()
                .any(|required_import| required_import.qualified_name() == qualified)
        }
        StmtRef::ImportFrom(ast::StmtImportFrom { module: None, .. })
        | StmtRef::Import(ast::StmtImport { .. }) => {
            let name = alias.name.as_str();
            let qualified = if name.contains('.') {
                QualifiedName::from_dotted_name(name)
            } else {
                QualifiedName::user_defined(name)
            };

            required_imports
                .iter()
                .any(|required_import| required_import.qualified_name() == qualified)
        }
        _ => false,
    }
}

/// UP010
pub(crate) fn unnecessary_future_import(checker: &Checker, scope: &Scope) {
    let mut unused_imports: HashMap<NodeId, Vec<&Alias>> = HashMap::new();
    let mut import_counts: HashMap<NodeId, usize> = HashMap::new();
    for future_name in chain(PY33_PLUS_REMOVE_FUTURES, PY37_PLUS_REMOVE_FUTURES).unique() {
        for binding_id in scope.get_all(future_name) {
            let binding = checker.semantic().binding(binding_id);
            if binding.kind.is_future_import() && binding.is_unused() {
                let Some(node_id) = binding.source else {
                    continue;
                };

                let stmt = checker.semantic().statement(node_id);
                if let Stmt::ImportFrom(ast::StmtImportFrom { names, .. }) = stmt {
                    import_counts
                        .entry(node_id)
                        .and_modify(|c| *c += names.len())
                        .or_insert(names.len());
                    let Some(alias) = names
                        .iter()
                        .find(|alias| alias.name.as_str() == binding.name(checker.source()))
                    else {
                        continue;
                    };

                    if alias.asname.is_some() {
                        continue;
                    }

                    if is_import_required_by_isort(
                        &checker.settings().isort.required_imports,
                        stmt.into(),
                        alias,
                    ) {
                        continue;
                    }
                    unused_imports.entry(node_id).or_default().push(alias);
                }
            }
        }
    }
    for (node_id, unused_aliases) in unused_imports {
        create_diagnostic(
            checker,
            unused_aliases.as_slice(),
            *import_counts.get(&node_id).unwrap_or(&unused_aliases.len()),
            checker.semantic().statement(node_id).range(),
            node_id,
        );
    }
}
fn create_diagnostic(
    checker: &Checker,
    unused_aliases: &[&Alias],
    import_counts: usize,
    range: TextRange,
    node_id: NodeId,
) {
    let mut diagnostic = checker.report_diagnostic(
        UnnecessaryFutureImport {
            names: unused_aliases
                .iter()
                .map(|alias| alias.name.to_string())
                .sorted()
                .collect(),
        },
        range,
    );

    if is_separate_unused_import_diag_enabled(checker.settings()) && import_counts > 1 {
        for unused_alias in unused_aliases {
            diagnostic.secondary_annotation(
                format!("Unused import `{}`", unused_alias.name),
                unused_alias.range(),
            );
        }
    }

    diagnostic.try_set_fix(|| {
        let statement = checker.semantic().statement(node_id);
        let parent = checker.semantic().parent_statement(node_id);
        let edit = fix::edits::remove_unused_imports(
            unused_aliases
                .iter()
                .map(|alias| &alias.name)
                .map(ast::Identifier::as_str),
            statement,
            parent,
            checker.locator(),
            checker.stylist(),
            checker.indexer(),
        )?;

        let range = edit.range();
        let applicability = if checker.comment_ranges().intersects(range) {
            Applicability::Unsafe
        } else {
            Applicability::Safe
        };

        Ok(
            Fix::applicable_edit(edit, applicability).isolate(Checker::isolation(
                checker.semantic().current_statement_parent_id(),
            )),
        )
    });
}
