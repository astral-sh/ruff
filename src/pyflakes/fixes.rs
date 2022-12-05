use anyhow::{bail, Result};
use libcst_native::{Codegen, CodegenState, ImportNames, SmallStatement, Statement};
use rustpython_ast::Stmt;

use crate::ast::types::Range;
use crate::autofix::{helpers, Fix};
use crate::cst::helpers::compose_module_path;
use crate::cst::matchers::match_module;
use crate::source_code_locator::SourceCodeLocator;

/// Generate a Fix to remove any unused imports from an `import` statement.
pub fn remove_unused_imports(
    locator: &SourceCodeLocator,
    unused_imports: &Vec<(&String, &Range)>,
    stmt: &Stmt,
    parent: Option<&Stmt>,
    deleted: &[&Stmt],
) -> Result<Fix> {
    let module_text = locator.slice_source_code_range(&Range::from_located(stmt));
    let mut tree = match_module(&module_text)?;

    let Some(Statement::Simple(body)) = tree.body.first_mut() else {
        bail!("Expected Statement::Simple");
    };

    let (aliases, import_module) = match body.body.first_mut() {
        Some(SmallStatement::Import(import_body)) => (&mut import_body.names, None),
        Some(SmallStatement::ImportFrom(import_body)) => {
            if let ImportNames::Aliases(names) = &mut import_body.names {
                (names, import_body.module.as_ref())
            } else {
                bail!("Expected ImportNames::Aliases")
            }
        }
        _ => bail!("Expected SmallStatement::ImportFrom or SmallStatement::Import"),
    };

    // Preserve the trailing comma (or not) from the last entry.
    let trailing_comma = aliases.last().and_then(|alias| alias.comma.clone());

    for (name_to_remove, _) in unused_imports {
        let alias_index = aliases.iter().position(|alias| {
            let full_name = match import_module {
                Some(module_name) => format!(
                    "{}.{}",
                    compose_module_path(module_name),
                    compose_module_path(&alias.name)
                ),
                None => compose_module_path(&alias.name),
            };
            &full_name.as_str() == name_to_remove
        });

        if let Some(index) = alias_index {
            aliases.remove(index);
        }
    }

    if let Some(alias) = aliases.last_mut() {
        alias.comma = trailing_comma;
    }

    if aliases.is_empty() {
        helpers::remove_stmt(stmt, parent, deleted)
    } else {
        let mut state = CodegenState::default();
        tree.codegen(&mut state);

        Ok(Fix::replacement(
            state.to_string(),
            stmt.location,
            stmt.end_location.unwrap(),
        ))
    }
}
