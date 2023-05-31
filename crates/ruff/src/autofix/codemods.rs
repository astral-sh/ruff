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
            let full_name = match import_module {
                Some((relative, module)) => {
                    let module = module.map(compose_module_path);
                    let member = compose_module_path(&alias.name);
                    let mut full_name = String::with_capacity(
                        relative.len() + module.as_ref().map_or(0, String::len) + member.len() + 1,
                    );
                    for _ in 0..relative.len() {
                        full_name.push('.');
                    }
                    if let Some(module) = module {
                        full_name.push_str(&module);
                        full_name.push('.');
                    }
                    full_name.push_str(&member);
                    full_name
                }
                None => compose_module_path(&alias.name),
            };
            full_name == *import
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

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);
    Ok(state.to_string())
}
