use anyhow::{bail, Result};
use libcst_native::{
    Call, Codegen, CodegenState, Dict, DictElement, Expression, ImportNames,
    ParenthesizableWhitespace, SmallStatement, Statement,
};
use rustpython_ast::{Expr, Stmt};

use crate::ast::types::Range;
use crate::autofix::{helpers, Fix};
use crate::cst::helpers::compose_module_path;
use crate::cst::matchers::{match_expr, match_module};
use crate::python::string::strip_quotes_and_prefixes;
use crate::source_code_locator::SourceCodeLocator;

/// Generate a `Fix` to remove any unused imports from an `import` statement.
pub fn remove_unused_imports(
    unused_imports: &Vec<(&String, &Range)>,
    stmt: &Stmt,
    parent: Option<&Stmt>,
    deleted: &[&Stmt],
    locator: &SourceCodeLocator,
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
        helpers::delete_stmt(stmt, parent, deleted, locator)
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

/// Generate a `Fix` to remove unused keys from format dict.
pub fn remove_unused_format_arguments_from_dict(
    unused_arguments: &[&str],
    stmt: &Expr,
    locator: &SourceCodeLocator,
) -> Result<Fix> {
    let module_text = locator.slice_source_code_range(&Range::from_located(stmt));
    let mut tree = match_module(&module_text)?;
    let mut body = match_expr(&mut tree)?;

    let new_dict = {
        let Expression::Dict(dict) = &body.value else {
            bail!("Expected Expression::Dict")
        };

        Dict {
            lbrace: dict.lbrace.clone(),
            lpar: dict.lpar.clone(),
            rbrace: dict.rbrace.clone(),
            rpar: dict.rpar.clone(),
            elements: dict
                .elements
                .iter()
                .filter_map(|e| match e {
                    DictElement::Simple {
                        key: Expression::SimpleString(name),
                        ..
                    } if unused_arguments.contains(&strip_quotes_and_prefixes(name.value)) => None,
                    e => Some(e.clone()),
                })
                .collect(),
        }
    };

    body.value = Expression::Dict(Box::new(new_dict));

    let mut state = CodegenState::default();
    tree.codegen(&mut state);

    Ok(Fix::replacement(
        state.to_string(),
        stmt.location,
        stmt.end_location.unwrap(),
    ))
}

/// Generate a `Fix` to remove unused keyword arguments from format call.
pub fn remove_unused_keyword_arguments_from_format_call(
    unused_arguments: &[&str],
    location: Range,
    locator: &SourceCodeLocator,
) -> Result<Fix> {
    let module_text = locator.slice_source_code_range(&location);
    let mut tree = match_module(&module_text)?;
    let mut body = match_expr(&mut tree)?;

    let new_call = {
        let Expression::Call(call) = &body.value else {
            bail!("Expected Expression::Call")
        };

        Call {
            func: call.func.clone(),
            lpar: call.lpar.clone(),
            rpar: call.rpar.clone(),
            whitespace_before_args: call.whitespace_before_args.clone(),
            whitespace_after_func: call.whitespace_after_func.clone(),
            args: call
                .args
                .iter()
                .filter_map(|e| match &e.keyword {
                    Some(kw) if unused_arguments.contains(&kw.value) => None,
                    _ => Some(e.clone()),
                })
                .collect(),
        }
    };

    body.value = Expression::Call(Box::new(new_call));

    let mut state = CodegenState::default();
    tree.codegen(&mut state);

    Ok(Fix::replacement(
        state.to_string(),
        location.location,
        location.end_location,
    ))
}
