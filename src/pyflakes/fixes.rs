use anyhow::Result;
use libcst_native::{Codegen, ImportNames, NameOrAttribute, SmallStatement, Statement};
use rustpython_ast::Stmt;

use crate::ast::types::Range;
use crate::autofix::{helpers, Fix};
use crate::cst::helpers::compose_module_path;
use crate::source_code_locator::SourceCodeLocator;

/// Generate a Fix to remove any unused imports from an `import` statement.
pub fn remove_unused_imports(
    locator: &SourceCodeLocator,
    full_names: &[&str],
    stmt: &Stmt,
    parent: Option<&Stmt>,
    deleted: &[&Stmt],
) -> Result<Fix> {
    let mut tree = match libcst_native::parse_module(
        locator.slice_source_code_range(&Range::from_located(stmt)),
        None,
    ) {
        Ok(m) => m,
        Err(_) => return Err(anyhow::anyhow!("Failed to extract CST from source.")),
    };

    let body = if let Some(Statement::Simple(body)) = tree.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Statement::Simple."));
    };
    let body = if let Some(SmallStatement::Import(body)) = body.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!(
            "Expected node to be: SmallStatement::ImportFrom."
        ));
    };
    let aliases = &mut body.names;

    // Preserve the trailing comma (or not) from the last entry.
    let trailing_comma = aliases.last().and_then(|alias| alias.comma.clone());

    // Identify unused imports from within the `import`.
    let mut removable = vec![];
    for (index, alias) in aliases.iter().enumerate() {
        if full_names.contains(&compose_module_path(&alias.name).as_str()) {
            removable.push(index);
        }
    }
    // TODO(charlie): This is quadratic.
    for index in removable.iter().rev() {
        aliases.remove(*index);
    }

    if let Some(alias) = aliases.last_mut() {
        alias.comma = trailing_comma;
    }

    if aliases.is_empty() {
        helpers::remove_stmt(stmt, parent, deleted)
    } else {
        let mut state = Default::default();
        tree.codegen(&mut state);

        Ok(Fix::replacement(
            state.to_string(),
            stmt.location,
            stmt.end_location.unwrap(),
        ))
    }
}

/// Generate a Fix to remove any unused imports from an `import from` statement.
pub fn remove_unused_import_froms(
    locator: &SourceCodeLocator,
    full_names: &[&str],
    stmt: &Stmt,
    parent: Option<&Stmt>,
    deleted: &[&Stmt],
) -> Result<Fix> {
    let mut tree = match libcst_native::parse_module(
        locator.slice_source_code_range(&Range::from_located(stmt)),
        None,
    ) {
        Ok(m) => m,
        Err(_) => return Err(anyhow::anyhow!("Failed to extract CST from source.")),
    };

    let body = if let Some(Statement::Simple(body)) = tree.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Statement::Simple."));
    };
    let body = if let Some(SmallStatement::ImportFrom(body)) = body.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!(
            "Expected node to be: SmallStatement::ImportFrom."
        ));
    };

    let aliases = if let ImportNames::Aliases(aliases) = &mut body.names {
        aliases
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Aliases."));
    };

    // Preserve the trailing comma (or not) from the last entry.
    let trailing_comma = aliases.last().and_then(|alias| alias.comma.clone());

    // Identify unused imports from within the `import from`.
    let mut removable = vec![];
    for (index, alias) in aliases.iter().enumerate() {
        if let NameOrAttribute::N(name) = &alias.name {
            let import_name = name.value.to_string();
            let full_name = body
                .module
                .as_ref()
                .map(compose_module_path)
                .map(|module_name| format!("{module_name}.{import_name}"))
                .unwrap_or(import_name);

            if full_names.contains(&full_name.as_str()) {
                removable.push(index);
            }
        }
    }
    // TODO(charlie): This is quadratic.
    for index in removable.iter().rev() {
        aliases.remove(*index);
    }

    if let Some(alias) = aliases.last_mut() {
        alias.comma = trailing_comma;
    }

    if aliases.is_empty() {
        helpers::remove_stmt(stmt, parent, deleted)
    } else {
        let mut state = Default::default();
        tree.codegen(&mut state);

        Ok(Fix::replacement(
            state.to_string(),
            stmt.location,
            stmt.end_location.unwrap(),
        ))
    }
}
