use ruff_python_ast::{self as ast, Identifier, Stmt};
use ruff_text_size::{Ranged, TextRange};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::resolve_imported_module_path;
use ruff_python_codegen::Generator;
use ruff_python_stdlib::identifiers::is_identifier;

use crate::checkers::ast::Checker;
use crate::preview::is_full_path_match_source_strategy_enabled;
use crate::{Edit, Fix, FixAvailability, Violation};

use crate::rules::flake8_tidy_imports::settings::ImportStyle;
use crate::rules::isort;

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
/// ## Options
/// - `lint.flake8-tidy-imports.relative-import-style`
///
/// [PEP 8]: https://peps.python.org/pep-0008/#imports
#[derive(ViolationMetadata)]
pub(crate) struct RelativeImports {
    strictness: ImportStyle,
}

impl Violation for RelativeImports {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        match self.strictness {
            ImportStyle::AlwaysRelative => {
                "Prefer relative imports over absolute imports".to_string()
            }
            ImportStyle::ParentsAbsolute => {
                "Prefer absolute imports over relative imports from parent modules".to_string()
            }
            ImportStyle::AlwaysAbsolute => {
                "Prefer absolute imports over relative imports".to_string()
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        let RelativeImports { strictness } = self;
        Some(match strictness {
            ImportStyle::AlwaysRelative => {
                "Replace absolute imports with relative imports".to_string()
            }
            ImportStyle::ParentsAbsolute => {
                "Replace relative imports from parent modules with absolute imports".to_string()
            }
            ImportStyle::AlwaysAbsolute => {
                "Replace relative imports with absolute imports".to_string()
            }
        })
    }
}

fn fix_banned_relative_import(
    stmt: &Stmt,
    level: u32,
    module: Option<&str>,
    module_path: Option<&[String]>,
    generator: Generator,
) -> Option<Fix> {
    // Only fix is the module path is known.
    let module_path = resolve_imported_module_path(level, module, module_path)?;

    // Require import to be a valid module:
    // https://python.org/dev/peps/pep-0008/#package-and-module-names
    if !module_path.split('.').all(is_identifier) {
        return None;
    }

    let Stmt::ImportFrom(ast::StmtImportFrom { names, .. }) = stmt else {
        panic!("Expected Stmt::ImportFrom");
    };
    let node = ast::StmtImportFrom {
        module: Some(Identifier::new(
            module_path.to_string(),
            TextRange::default(),
        )),
        names: names.clone(),
        level: 0,
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
    };
    let content = generator.stmt(&node.into());
    Some(Fix::unsafe_edit(Edit::range_replacement(
        content,
        stmt.range(),
    )))
}

/// TID252
pub(crate) fn enforce_import_strictness(
    checker: &Checker,
    stmt: &Stmt,
    level: u32,
    module: Option<&str>,
    module_path: Option<&[String]>,
    strictness: ImportStyle,
) {
    match strictness {
        ImportStyle::AlwaysAbsolute | ImportStyle::ParentsAbsolute => {
            let strictness_level = if matches!(strictness, ImportStyle::AlwaysAbsolute) {
                0
            } else {
                1
            };
            if level > strictness_level {
                let mut diagnostic =
                    checker.report_diagnostic(RelativeImports { strictness }, stmt.range());
                if let Some(fix) = fix_banned_relative_import(
                    stmt,
                    level,
                    module,
                    module_path,
                    checker.generator(),
                ) {
                    diagnostic.set_fix(fix);
                }
            }
        }
        ImportStyle::AlwaysRelative => {
            if level == 0 {
                if let Some(module_name) = module {
                    let match_source_strategy =
                        if is_full_path_match_source_strategy_enabled(checker.settings()) {
                            isort::categorize::MatchSourceStrategy::FullPath
                        } else {
                            isort::categorize::MatchSourceStrategy::Root
                        };
                    let import_type = isort::categorize::categorize(
                        module_name,
                        false,
                        &checker.settings().src,
                        checker.package(),
                        checker.settings().isort.detect_same_package,
                        &checker.settings().isort.known_modules,
                        checker.target_version(),
                        checker.settings().isort.no_sections,
                        &checker.settings().isort.section_order,
                        &checker.settings().isort.default_section,
                        match_source_strategy,
                    );

                    if matches!(
                        import_type,
                        isort::ImportSection::Known(
                            isort::ImportType::FirstParty | isort::ImportType::LocalFolder
                        )
                    ) {
                        checker.report_diagnostic(RelativeImports { strictness }, stmt.range());
                    }
                }
            }
        }
    }
}
