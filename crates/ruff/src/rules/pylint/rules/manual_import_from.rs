use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Alias, Int, Ranged, Stmt};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::unparse_stmt;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct ManualFromImport {
    module: String,
    name: String,
}

impl Violation for ManualFromImport {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ManualFromImport { module, name } = self;
        format!("Use `from {module} import {name}` in lieu of alias")
    }

    fn autofix_title(&self) -> Option<String> {
        let ManualFromImport { module, name } = self;
        Some(format!("Replace with `from {module} import {name}`"))
    }
}

/// PLR0402
pub(crate) fn manual_from_import(
    checker: &mut Checker,
    stmt: &Stmt,
    alias: &Alias,
    names: &[Alias],
) {
    let Some(asname) = &alias.asname else {
        return;
    };
    let Some((module, name)) = alias.name.rsplit_once('.') else {
        return;
    };
    if asname != name {
        return;
    }

    let fixable = names.len() == 1;
    let mut diagnostic = Diagnostic::new(
        ManualFromImport {
            module: module.to_string(),
            name: name.to_string(),
        },
        alias.range(),
    );
    if fixable && checker.patch(diagnostic.kind.rule()) {
        let node = ast::StmtImportFrom {
            module: Some(module.into()),
            names: vec![Alias {
                name: asname.clone(),
                asname: None,
                range: TextRange::default(),
            }],
            level: Some(Int::new(0)),
            range: TextRange::default(),
        };
        #[allow(deprecated)]
        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
            unparse_stmt(&node.into(), checker.stylist),
            stmt.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}
