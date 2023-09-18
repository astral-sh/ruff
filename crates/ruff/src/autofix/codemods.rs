//! Interface for editing code snippets. These functions take statements or expressions as input,
//! and return the modified code snippet as output.
use anyhow::{bail, Result};
use libcst_native::{
    Codegen, CodegenState, ImportNames, ParenthesizableWhitespace, SmallStatement, Statement,
};

use ruff_python_ast::Stmt;
use ruff_python_codegen::Stylist;
use ruff_source_file::Locator;

use crate::cst::helpers::compose_module_path;
use crate::cst::matchers::match_statement;

/// Glue code to make libcst codegen work with ruff's Stylist
pub(crate) trait CodegenStylist<'a>: Codegen<'a> {
    fn codegen_stylist(&self, stylist: &'a Stylist) -> String;
}

impl<'a, T: Codegen<'a>> CodegenStylist<'a> for T {
    fn codegen_stylist(&self, stylist: &'a Stylist) -> String {
        let mut state = CodegenState {
            default_newline: stylist.line_ending().as_str(),
            default_indent: stylist.indentation(),
            ..Default::default()
        };
        self.codegen(&mut state);
        state.to_string()
    }
}

/// Given an import statement, remove any imports that are specified in the `imports` iterator.
///
/// Returns `Ok(None)` if the statement is empty after removing the imports.
pub(crate) fn remove_imports<'a>(
    member_names: impl Iterator<Item = &'a str>,
    stmt: &Stmt,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Option<String>> {
    let module_text = locator.slice(stmt);
    let mut tree = match_statement(module_text)?;

    let Statement::Simple(body) = &mut tree else {
        bail!("Expected Statement::Simple");
    };

    let aliases = match body.body.first_mut() {
        Some(SmallStatement::Import(import_body)) => &mut import_body.names,
        Some(SmallStatement::ImportFrom(import_body)) => {
            if let ImportNames::Aliases(names) = &mut import_body.names {
                names
            } else if let ImportNames::Star(..) = &import_body.names {
                // Special-case: if the import is a `from ... import *`, then we delete the
                // entire statement.
                let mut found_star = false;
                for member in member_names {
                    if member == "*" {
                        found_star = true;
                    } else {
                        bail!("Expected \"*\" for unused import (got: \"{}\")", member);
                    }
                }
                if !found_star {
                    bail!("Expected \'*\' for unused import");
                }
                return Ok(None);
            } else {
                bail!("Expected: ImportNames::Aliases | ImportNames::Star");
            }
        }
        _ => bail!("Expected: SmallStatement::ImportFrom | SmallStatement::Import"),
    };

    // Preserve the trailing comma (or not) from the last entry.
    let trailing_comma = aliases.last().and_then(|alias| alias.comma.clone());

    for member in member_names {
        let alias_index = aliases
            .iter()
            .position(|alias| member == compose_module_path(&alias.name));
        if let Some(index) = alias_index {
            aliases.remove(index);
        }
    }

    // But avoid destroying any trailing comments.
    if let Some(alias) = aliases.last_mut() {
        let has_comment = if let Some(comma) = &alias.comma {
            match &comma.whitespace_after {
                ParenthesizableWhitespace::SimpleWhitespace(_) => false,
                ParenthesizableWhitespace::ParenthesizedWhitespace(whitespace) => {
                    whitespace.first_line.comment.is_some()
                }
            }
        } else {
            false
        };
        if !has_comment {
            alias.comma = trailing_comma;
        }
    }

    if aliases.is_empty() {
        return Ok(None);
    }

    Ok(Some(tree.codegen_stylist(stylist)))
}

/// Given an import statement, remove any imports that are not specified in the `imports` slice.
///
/// Returns the modified import statement.
pub(crate) fn retain_imports(
    member_names: &[&str],
    stmt: &Stmt,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<String> {
    let module_text = locator.slice(stmt);
    let mut tree = match_statement(module_text)?;

    let Statement::Simple(body) = &mut tree else {
        bail!("Expected Statement::Simple");
    };

    let aliases = match body.body.first_mut() {
        Some(SmallStatement::Import(import_body)) => &mut import_body.names,
        Some(SmallStatement::ImportFrom(import_body)) => {
            if let ImportNames::Aliases(names) = &mut import_body.names {
                names
            } else {
                bail!("Expected: ImportNames::Aliases");
            }
        }
        _ => bail!("Expected: SmallStatement::ImportFrom | SmallStatement::Import"),
    };

    // Preserve the trailing comma (or not) from the last entry.
    let trailing_comma = aliases.last().and_then(|alias| alias.comma.clone());

    aliases.retain(|alias| {
        member_names
            .iter()
            .any(|member| *member == compose_module_path(&alias.name))
    });

    // But avoid destroying any trailing comments.
    if let Some(alias) = aliases.last_mut() {
        let has_comment = if let Some(comma) = &alias.comma {
            match &comma.whitespace_after {
                ParenthesizableWhitespace::SimpleWhitespace(_) => false,
                ParenthesizableWhitespace::ParenthesizedWhitespace(whitespace) => {
                    whitespace.first_line.comment.is_some()
                }
            }
        } else {
            false
        };
        if !has_comment {
            alias.comma = trailing_comma;
        }
    }

    Ok(tree.codegen_stylist(stylist))
}
