use ruff_python_ast::{self as ast, Identifier, Stmt};
use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::resolve_imported_module_path;
use ruff_python_codegen::Generator;
use ruff_python_stdlib::identifiers::is_identifier;

use crate::checkers::ast::Checker;

use crate::rules::flake8_tidy_imports::settings::Strictness;

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
/// - `lint.flake8-tidy-imports.ban-relative-imports`
///
/// [PEP 8]: https://peps.python.org/pep-0008/#imports
#[derive(ViolationMetadata)]
pub(crate) struct RelativeImports {
    strictness: Strictness,
}

impl Violation for RelativeImports {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        match self.strictness {
            Strictness::Parents => {
                "Prefer absolute imports over relative imports from parent modules".to_string()
            }
            Strictness::All => "Prefer absolute imports over relative imports".to_string(),
        }
    }

    fn fix_title(&self) -> Option<String> {
        let RelativeImports { strictness } = self;
        Some(match strictness {
            Strictness::Parents => {
                "Replace relative imports from parent modules with absolute imports".to_string()
            }
            Strictness::All => "Replace relative imports with absolute imports".to_string(),
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
    };
    let content = generator.stmt(&node.into());
    Some(Fix::unsafe_edit(Edit::range_replacement(
        content,
        stmt.range(),
    )))
}

/// TID252
pub(crate) fn banned_relative_import(
    checker: &Checker,
    stmt: &Stmt,
    level: u32,
    module: Option<&str>,
    module_path: Option<&[String]>,
    strictness: Strictness,
) -> Option<Diagnostic> {
    let strictness_level = match strictness {
        Strictness::All => 0,
        Strictness::Parents => 1,
    };
    if level > strictness_level {
        let mut diagnostic = Diagnostic::new(RelativeImports { strictness }, stmt.range());
        if let Some(fix) =
            fix_banned_relative_import(stmt, level, module, module_path, checker.generator())
        {
            diagnostic.set_fix(fix);
        }
        Some(diagnostic)
    } else {
        None
    }
}
