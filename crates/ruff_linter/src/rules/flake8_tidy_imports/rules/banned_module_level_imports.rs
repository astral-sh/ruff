use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat;
use ruff_python_ast::helpers::resolve_imported_module_path;
use ruff_python_ast::{Alias, Stmt, StmtImport, StmtImportFrom, flat::AnyNodeRef};
use ruff_text_size::Ranged;
use std::borrow::Cow;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::flake8_tidy_imports::matchers::{MatchName, MatchNameOrParent, NameMatchPolicy};

/// ## What it does
/// Checks for module-level imports that should instead be imported lazily
/// (e.g., within a function definition, or an `if TYPE_CHECKING:` block, or
/// some other nested context).
///
/// ## Why is this bad?
/// Some modules are expensive to import. For example, importing `torch` or
/// `tensorflow` can introduce a noticeable delay in the startup time of a
/// Python program.
///
/// In such cases, you may want to enforce that the module is imported lazily
/// as needed, rather than at the top of the file. This could involve inlining
/// the import into the function that uses it, rather than importing it
/// unconditionally, to ensure that the module is only imported when necessary.
///
/// ## Example
/// ```python
/// import tensorflow as tf
///
///
/// def show_version():
///     print(tf.__version__)
/// ```
///
/// Use instead:
/// ```python
/// def show_version():
///     import tensorflow as tf
///
///     print(tf.__version__)
/// ```
///
/// ## Options
/// - `lint.flake8-tidy-imports.banned-module-level-imports`
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.285", category = Category::Restriction)]
pub(crate) struct BannedModuleLevelImports {
    name: String,
}

impl Violation for BannedModuleLevelImports {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BannedModuleLevelImports { name } = self;
        format!("`{name}` is banned at the module level")
    }
}

/// TID253
pub(crate) fn banned_module_level_imports(checker: &Checker, stmt: flat::View<'_, Stmt>) {
    if !checker.semantic().at_top_level() {
        return;
    }

    for (policy, node) in &BannedModuleImportPolicies::new(stmt, checker) {
        if let Some(banned_module) = policy.find(
            checker
                .settings()
                .flake8_tidy_imports
                .banned_module_level_imports(),
        ) {
            checker.report_diagnostic(
                BannedModuleLevelImports {
                    name: banned_module,
                },
                node.range(),
            );
        }
    }
}

pub(crate) enum BannedModuleImportPolicies<'a> {
    Import(flat::View<'a, StmtImport>),
    ImportFrom {
        module: Option<Cow<'a, str>>,
        node: flat::View<'a, StmtImportFrom>,
    },
    NonImport,
}

impl<'a> BannedModuleImportPolicies<'a> {
    pub(crate) fn new(stmt: flat::View<'a, Stmt>, checker: &Checker) -> Self {
        match stmt.classify() {
            flat::StmtKind::Import(import) => Self::Import(import),
            flat::StmtKind::ImportFrom(import) => {
                let module = import.module();
                let level = import.level();

                let module = resolve_imported_module_path(
                    level,
                    module.map(flat::View::<ruff_python_ast::Identifier>::as_str),
                    checker.module.qualified_name(),
                );

                Self::ImportFrom {
                    module,
                    node: import,
                }
            }
            _ => Self::NonImport,
        }
    }
}

impl<'a> IntoIterator for &'a BannedModuleImportPolicies<'a> {
    type Item = <Self::IntoIter as Iterator>::Item;
    type IntoIter = BannedModuleImportPoliciesIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            BannedModuleImportPolicies::Import(import) => {
                BannedModuleImportPoliciesIter::Import(import.names().iter())
            }
            BannedModuleImportPolicies::ImportFrom { module, node } => {
                BannedModuleImportPoliciesIter::ImportFrom {
                    module: module.as_deref(),
                    names: node.names().iter(),
                    import: Some(*node),
                }
            }
            BannedModuleImportPolicies::NonImport => BannedModuleImportPoliciesIter::NonImport,
        }
    }
}

pub(crate) enum BannedModuleImportPoliciesIter<'a> {
    Import(flat::ListIter<'a, flat::Handle<Alias>>),
    ImportFrom {
        module: Option<&'a str>,
        names: flat::ListIter<'a, flat::Handle<Alias>>,
        import: Option<flat::View<'a, StmtImportFrom>>,
    },
    NonImport,
}

impl<'a> Iterator for BannedModuleImportPoliciesIter<'a> {
    type Item = (NameMatchPolicy<'a>, AnyNodeRef<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Import(names) => {
                let name = names.next()?;
                Some((
                    NameMatchPolicy::MatchNameOrParent(MatchNameOrParent {
                        module: name.name().as_str(),
                    }),
                    name.into(),
                ))
            }
            Self::ImportFrom {
                module,
                import,
                names,
            } => {
                let module = module.as_ref()?;

                if let Some(import) = import.take() {
                    return Some((
                        NameMatchPolicy::MatchNameOrParent(MatchNameOrParent { module }),
                        import.into(),
                    ));
                }

                loop {
                    let alias = names.next()?;
                    if alias.name() == "*" {
                        continue;
                    }

                    break Some((
                        NameMatchPolicy::MatchName(MatchName {
                            module,
                            member: alias.name().as_str(),
                        }),
                        alias.into(),
                    ));
                }
            }
            Self::NonImport => None,
        }
    }
}
