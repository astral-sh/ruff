use rustpython_parser::ast::{Stmt, StmtKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Violation};
use ruff_macros::{derive_message_formats, violation, CacheKey};
use ruff_python_ast::helpers::{create_stmt, from_relative_import, unparse_stmt};
use ruff_python_ast::source_code::Stylist;
use ruff_python_ast::types::Range;
use ruff_python_stdlib::identifiers::is_identifier;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

pub type Settings = Strictness;

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, CacheKey, JsonSchema, Default,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub enum Strictness {
    /// Ban imports that extend into the parent module or beyond.
    #[default]
    Parents,
    /// Ban all relative imports.
    All,
    /// Ban parent imports but force relative imports for siblings.
    ForceSiblings,
}

/// ## What it does
/// Checks for relative imports.
///
/// ## Why is this bad?
/// Absolute imports, or relative imports from siblings, are recommended by [PEP 8]:
///
/// > Absolute imports are recommended, as they are usually more readable and tend to be better behaved...
/// > ```python
/// > import mypkg.sibling
/// > from mypkg import sibling
/// > from mypkg.sibling import example
/// > ```
/// > However, explicit relative imports are an acceptable alternative to absolute imports,
/// > especially when dealing with complex package layouts where using absolute imports would be
/// > unnecessarily verbose:
/// > ```python
/// > from . import sibling
/// > from .sibling import example
/// > ```
///
/// ## Options
/// - `flake8-tidy-imports.ban-relative-imports`
///
/// ## Example
/// ```python
/// from .. import foo
/// ```
///
/// Use instead:
/// ```python
/// from mypkg import foo
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#imports
#[violation]
pub struct RelativeImports {
    pub strictness: Strictness,
}

impl Violation for RelativeImports {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        match self.strictness {
            Strictness::Parents | Strictness::ForceSiblings => {
                format!("Relative imports from parent modules are banned")
            }
            Strictness::All => format!("Relative imports are banned"),
        }
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        Some(|RelativeImports { strictness }| match strictness {
            Strictness::Parents | Strictness::ForceSiblings => {
                format!("Replace relative imports from parent modules with absolute imports")
            }
            Strictness::All => format!("Replace relative imports with absolute imports"),
        })
    }
}

fn fix_banned_relative_import(
    stmt: &Stmt,
    level: Option<usize>,
    module: Option<&str>,
    module_path: Option<&Vec<String>>,
    stylist: &Stylist,
) -> Option<Edit> {
    // Only fix is the module path is known.
    if let Some(mut parts) = module_path.cloned() {
        if level? >= parts.len() {
            return None;
        }

        // Remove relative level from module path.
        for _ in 0..level? {
            parts.pop();
        }

        let module_name = if let Some(module) = module {
            let call_path = from_relative_import(&parts, module);
            // Require import to be a valid module:
            // https://python.org/dev/peps/pep-0008/#package-and-module-names
            if !call_path.iter().all(|part| is_identifier(part)) {
                return None;
            }
            call_path.as_slice().join(".")
        } else if parts.len() > 1 {
            let module = parts.pop().unwrap();
            let call_path = from_relative_import(&parts, &module);
            // Require import to be a valid module:
            // https://python.org/dev/peps/pep-0008/#package-and-module-names
            if !call_path.iter().all(|part| is_identifier(part)) {
                return None;
            }
            call_path.as_slice().join(".")
        } else {
            // Require import to be a valid module:
            // https://python.org/dev/peps/pep-0008/#package-and-module-names
            if !parts.iter().all(|part| is_identifier(part)) {
                return None;
            }
            parts.join(".")
        };

        let StmtKind::ImportFrom { names, .. } = &stmt.node else {
            panic!("Expected StmtKind::ImportFrom");
        };
        let content = unparse_stmt(
            &create_stmt(StmtKind::ImportFrom {
                module: Some(module_name),
                names: names.clone(),
                level: Some(0),
            }),
            stylist,
        );

        Some(Edit::replacement(
            content,
            stmt.location,
            stmt.end_location.unwrap(),
        ))
    } else {
        None
    }
}

/// TID252
pub fn banned_relative_import(
    checker: &Checker,
    stmt: &Stmt,
    level: Option<usize>,
    module: Option<&str>,
    module_path: Option<&Vec<String>>,
    strictness: &Strictness,
) -> Option<Diagnostic> {
    let strictness_level = match strictness {
        Strictness::All => 0,
        Strictness::Parents | Strictness::ForceSiblings => 1,
    };
    if level? > strictness_level {
        let mut diagnostic = Diagnostic::new(
            RelativeImports {
                strictness: *strictness,
            },
            Range::from(stmt),
        );
        if checker.patch(diagnostic.kind.rule()) {
            if let Some(fix) =
                fix_banned_relative_import(stmt, level, module, module_path, checker.stylist)
            {
                diagnostic.set_fix(fix);
            };
        }
        Some(diagnostic)
    } else {
        None
    }
}

/// ## What it does
/// Requires relative imports for siblings
///
/// ## Why is this bad?
/// Absolute imports, or relative imports from siblings, are recommended by [PEP 8]:
///
/// > Absolute imports are recommended, as they are usually more readable and tend to be better behaved...
/// > ```python
/// > import mypkg.sibling
/// > from mypkg import sibling
/// > from mypkg.sibling import example
/// > ```
/// > However, explicit relative imports are an acceptable alternative to absolute imports,
/// > especially when dealing with complex package layouts where using absolute imports would be
/// > unnecessarily verbose:
/// > ```python
/// > from . import sibling
/// > from .sibling import example
/// > ```
///
///
/// ## Options
/// - `flake8-tidy-imports.ban-relative-imports`
///
/// ## Example
/// ```python
/// from mypkg import foo
/// ```
///
/// Use instead:
/// ```python
/// from . import foo
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#imports
#[violation]
pub struct RelativeSiblings;

impl Violation for RelativeSiblings {
    const AUTOFIX: AutofixKind = AutofixKind::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Relative imports for sibling modules are required")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        Some(|_| format!("Replace absolute sibling imports with relative imports"))
    }
}

fn fix_relative_sibling_import(
    stmt: &Stmt,
    module: Option<&str>,
    pkg: &str,
    stylist: &Stylist,
) -> Edit {
    let StmtKind::ImportFrom { names, .. } = &stmt.node else {
        panic!("Expected StmtKind::ImportFrom");
    };
    let prefix = format!("{pkg}.");
    let new_module = module
        .unwrap()
        .strip_prefix(prefix.as_str())
        .map(String::from);
    let content = unparse_stmt(
        &create_stmt(StmtKind::ImportFrom {
            module: new_module,
            names: names.clone(),
            level: Some(1),
        }),
        stylist,
    );

    Edit::replacement(content, stmt.location, stmt.end_location.unwrap())
}

/// TID253
pub fn force_siblings(
    checker: &Checker,
    stmt: &Stmt,
    level: Option<usize>,
    module: Option<&str>,
    module_path: Option<&Vec<String>>,
    strictness: &Strictness,
) -> Option<Diagnostic> {
    if let Strictness::ForceSiblings = strictness {
        if let Some(mods) = &module_path {
            let pkg = mods[..mods.len() - 1].join(".");
            if level == Some(0) && module.unwrap().starts_with(pkg.as_str()) {
                let mut diagnostic = Diagnostic::new(RelativeSiblings {}, Range::from(stmt));
                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.set_fix(fix_relative_sibling_import(
                        stmt,
                        module,
                        pkg.as_str(),
                        checker.stylist,
                    ));
                }
                return Some(diagnostic);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::assert_messages;
    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::settings::Settings;
    use crate::test::test_path;

    use super::Strictness;

    #[test_case(Strictness::Parents; "parents")]
    #[test_case(Strictness::ForceSiblings; "force-siblings")]
    fn ban_parent_imports(strictness: Strictness) -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID252.py"),
            &Settings {
                flake8_tidy_imports: super::super::Settings {
                    ban_relative_imports: strictness,
                    ..Default::default()
                },
                ..Settings::for_rules(vec![Rule::RelativeImports])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn ban_all_imports() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID252.py"),
            &Settings {
                flake8_tidy_imports: super::super::Settings {
                    ban_relative_imports: Strictness::All,
                    ..Default::default()
                },
                ..Settings::for_rules(vec![Rule::RelativeImports])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test_case(Strictness::Parents; "parents")]
    #[test_case(Strictness::ForceSiblings; "force-siblings")]
    fn ban_parent_imports_package(strictness: Strictness) -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID252/my_package/sublib/api/application.py"),
            &Settings {
                flake8_tidy_imports: super::super::Settings {
                    ban_relative_imports: strictness,
                    ..Default::default()
                },
                namespace_packages: vec![Path::new("my_package").to_path_buf()],
                ..Settings::for_rules(vec![Rule::RelativeImports])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test_case(Path::new("TID253.py"); "root module not in package")]
    #[test_case(Path::new("TID253/__init__.py"); "root package init")]
    #[test_case(Path::new("TID253/module.py"); "root package module")]
    #[test_case(Path::new("TID253/nested/__init__.py"); "nested package init")]
    #[test_case(Path::new("TID253/nested/module.py"); "nested package module")]
    #[test_case(Path::new("TID253/not_a_pkg/module.py"); "nested module not in package")]
    fn ban_parent_imports_force_siblings(path: &Path) -> Result<()> {
        let file = path.to_string_lossy();
        let diagnostics = test_path(
            Path::new(&format!("flake8_tidy_imports/{file}")),
            &Settings {
                flake8_tidy_imports: super::super::Settings {
                    ban_relative_imports: Strictness::ForceSiblings,
                    ..Default::default()
                },
                ..Settings::for_rules(vec![Rule::RelativeSiblings])
            },
        )?;
        let snapshot = file.replace("__", "").replace('/', "__").replace(".py", "");
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
