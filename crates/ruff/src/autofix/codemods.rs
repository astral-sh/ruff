//! Interface for editing code snippets. These functions take statements or expressions as input,
//! and return the modified code snippet as output.
use anyhow::{bail, Result};
use libcst_native::{
    Codegen, CodegenState, ImportNames, ParenthesizableWhitespace, SmallStatement, Statement,
};
use rustpython_parser::ast::{Ranged, Stmt};

use ruff_python_ast::source_code::{Locator, Stylist};

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
    imports: impl Iterator<Item = &'a str>,
    stmt: &Stmt,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Option<String>> {
    let module_text = locator.slice(stmt.range());
    let mut tree = match_statement(module_text)?;

    let Statement::Simple(body) = &mut tree else {
        bail!("Expected Statement::Simple");
    };

    let (aliases, import_module) = match body.body.first_mut() {
        Some(SmallStatement::Import(import_body)) => (&mut import_body.names, None),
        Some(SmallStatement::ImportFrom(import_body)) => {
            if let ImportNames::Aliases(names) = &mut import_body.names {
                (
                    names,
                    Some((&import_body.relative, import_body.module.as_ref())),
                )
            } else if let ImportNames::Star(..) = &import_body.names {
                // Special-case: if the import is a `from ... import *`, then we delete the
                // entire statement.
                let mut found_star = false;
                for import in imports {
                    let qualified_name = match import_body.module.as_ref() {
                        Some(module_name) => format!("{}.*", compose_module_path(module_name)),
                        None => "*".to_string(),
                    };
                    if import == qualified_name {
                        found_star = true;
                    } else {
                        bail!("Expected \"*\" for unused import (got: \"{}\")", import);
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

    for import in imports {
        let alias_index = aliases.iter().position(|alias| {
            let qualified_name = match import_module {
                Some((relative, module)) => {
                    let module = module.map(compose_module_path);
                    let member = compose_module_path(&alias.name);
                    let mut qualified_name = String::with_capacity(
                        relative.len() + module.as_ref().map_or(0, String::len) + member.len() + 1,
                    );
                    for _ in 0..relative.len() {
                        qualified_name.push('.');
                    }
                    if let Some(module) = module {
                        qualified_name.push_str(&module);
                        qualified_name.push('.');
                    }
                    qualified_name.push_str(&member);
                    qualified_name
                }
                None => compose_module_path(&alias.name),
            };
            qualified_name == import
        });

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
    imports: &[&str],
    stmt: &Stmt,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<String> {
    let module_text = locator.slice(stmt.range());
    let mut tree = match_statement(module_text)?;

    let Statement::Simple(body) = &mut tree else {
        bail!("Expected Statement::Simple");
    };

    let (aliases, import_module) = match body.body.first_mut() {
        Some(SmallStatement::Import(import_body)) => (&mut import_body.names, None),
        Some(SmallStatement::ImportFrom(import_body)) => {
            if let ImportNames::Aliases(names) = &mut import_body.names {
                (
                    names,
                    Some((&import_body.relative, import_body.module.as_ref())),
                )
            } else {
                bail!("Expected: ImportNames::Aliases");
            }
        }
        _ => bail!("Expected: SmallStatement::ImportFrom | SmallStatement::Import"),
    };

    // Preserve the trailing comma (or not) from the last entry.
    let trailing_comma = aliases.last().and_then(|alias| alias.comma.clone());

    aliases.retain(|alias| {
        imports.iter().any(|import| {
            let qualified_name = match import_module {
                Some((relative, module)) => {
                    let module = module.map(compose_module_path);
                    let member = compose_module_path(&alias.name);
                    let mut qualified_name = String::with_capacity(
                        relative.len() + module.as_ref().map_or(0, String::len) + member.len() + 1,
                    );
                    for _ in 0..relative.len() {
                        qualified_name.push('.');
                    }
                    if let Some(module) = module {
                        qualified_name.push_str(&module);
                        qualified_name.push('.');
                    }
                    qualified_name.push_str(&member);
                    qualified_name
                }
                None => compose_module_path(&alias.name),
            };
            qualified_name == *import
        })
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
