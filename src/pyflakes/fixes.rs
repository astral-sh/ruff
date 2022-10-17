use libcst_native::ImportNames::Aliases;
use libcst_native::NameOrAttribute::N;
use libcst_native::{Codegen, SmallStatement, Statement};
use rustpython_ast::Stmt;

use crate::ast::operations::SourceCodeLocator;
use crate::ast::types::Range;
use crate::autofix::{helpers, Fix};

/// Generate a Fix to remove any unused imports from an `import` statement.
pub fn remove_unused_imports(
    locator: &mut SourceCodeLocator,
    full_names: &[&str],
    stmt: &Stmt,
    parent: Option<&Stmt>,
    deleted: &[&Stmt],
) -> anyhow::Result<Fix> {
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

    // Identify unused imports from within the `import from`.
    let mut removable = vec![];
    for (index, alias) in aliases.iter().enumerate() {
        if let N(import_name) = &alias.name {
            if full_names.contains(&import_name.value) {
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

/// Generate a Fix to remove any unused imports from an `import from` statement.
pub fn remove_unused_import_froms(
    locator: &mut SourceCodeLocator,
    full_names: &[&str],
    stmt: &Stmt,
    parent: Option<&Stmt>,
    deleted: &[&Stmt],
) -> anyhow::Result<Fix> {
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
    let aliases = if let Aliases(aliases) = &mut body.names {
        aliases
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Aliases."));
    };

    // Preserve the trailing comma (or not) from the last entry.
    let trailing_comma = aliases.last().and_then(|alias| alias.comma.clone());

    // Identify unused imports from within the `import from`.
    let mut removable = vec![];
    for (index, alias) in aliases.iter().enumerate() {
        if let N(name) = &alias.name {
            let import_name = if let Some(N(module_name)) = &body.module {
                format!("{}.{}", module_name.value, name.value)
            } else {
                name.value.to_string()
            };
            if full_names.contains(&import_name.as_str()) {
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
