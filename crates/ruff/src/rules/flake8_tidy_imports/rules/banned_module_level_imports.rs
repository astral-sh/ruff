use ruff_python_ast::{Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_source_file::Locator;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for banned imports at module level. The banned imports are allowed inline, such as
/// within a function definition or an `if TYPE_CHECKING:` block.
///
/// ## Why is this bad?
/// Some modules take a long time to import. Library authors might want to ensure that you only pay
/// the import cost for these modules if you directly use them, rather than if you import a module
/// that happens to use an expensive module in one of its functions.
///
/// ## Options
/// - `flake8-tidy-imports.banned-module-level-imports`
#[violation]
pub struct BannedModuleLevelImports {
    name: String,
}

impl Violation for BannedModuleLevelImports {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BannedModuleLevelImports { name } = self;
        format!("`{name}` is banned at module level. Please move the import inline.")
    }
}

enum NameMatchPolicy {
    ExactOnly,
    ExactOrParents,
}

fn banned_at_module_level_with_policy<T>(
    checker: &mut Checker,
    name: &str,
    stmt: &Stmt,
    located: &T,
    locator: &Locator,
    policy: &NameMatchPolicy,
) where
    T: Ranged,
{
    if !locator.is_at_start_of_line(stmt.start()) {
        return;
    }
    let banned_module_level_imports = &checker
        .settings
        .flake8_tidy_imports
        .banned_module_level_imports;
    for banned_module_name in banned_module_level_imports {
        let name_is_banned = match policy {
            NameMatchPolicy::ExactOnly => name == banned_module_name,
            NameMatchPolicy::ExactOrParents => {
                name == banned_module_name || name.starts_with(&format!("{banned_module_name}."))
            }
        };
        if name_is_banned {
            checker.diagnostics.push(Diagnostic::new(
                BannedModuleLevelImports {
                    name: banned_module_name.to_string(),
                },
                located.range(),
            ));
            return;
        }
    }
}

/// TID253
pub(crate) fn name_is_banned_at_module_level<T>(
    checker: &mut Checker,
    name: &str,
    stmt: &Stmt,
    located: &T,
    locator: &Locator,
) where
    T: Ranged,
{
    banned_at_module_level_with_policy(
        checker,
        name,
        stmt,
        located,
        locator,
        &NameMatchPolicy::ExactOnly,
    );
}

/// TID253
pub(crate) fn name_or_parent_is_banned_at_module_level<T>(
    checker: &mut Checker,
    name: &str,
    stmt: &Stmt,
    located: &T,
    locator: &Locator,
) where
    T: Ranged,
{
    banned_at_module_level_with_policy(
        checker,
        name,
        stmt,
        located,
        locator,
        &NameMatchPolicy::ExactOrParents,
    );
}
