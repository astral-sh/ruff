use anyhow::Result;
use libcst_native::{
    AsName, AssignTargetExpression, Attribute, Codegen, CodegenState, Dot, Expression, Import,
    ImportAlias, ImportFrom, ImportNames, Name, NameOrAttribute, ParenthesizableWhitespace,
};
use log::error;
use rustpython_ast::{Expr, ExprKind, Stmt, StmtKind};

use crate::ast::helpers::collect_call_paths;
use crate::ast::types::Range;
use crate::ast::whitespace::indentation;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::cst::matchers::{match_import, match_import_from, match_module};
use crate::registry::{Check, CheckCode, CheckKind, MockReference};
use crate::source_code_locator::SourceCodeLocator;
use crate::source_code_style::SourceCodeStyleDetector;

/// Return a vector of all non-`mock` imports.
fn clean_import_aliases(aliases: Vec<ImportAlias>) -> (Vec<ImportAlias>, Vec<Option<AsName>>) {
    // Preserve the trailing comma (or not) from the last entry.
    let trailing_comma = aliases.last().and_then(|alias| alias.comma.clone());

    let mut clean_aliases: Vec<ImportAlias> = vec![];
    let mut mock_aliases: Vec<Option<AsName>> = vec![];
    for alias in aliases {
        match &alias.name {
            // Ex) `import mock`
            NameOrAttribute::N(name_struct) => {
                if name_struct.value == "mock" {
                    mock_aliases.push(alias.asname.clone());
                    continue;
                }
                clean_aliases.push(alias);
            }
            // Ex) `import mock.mock`
            NameOrAttribute::A(attribute_struct) => {
                if let Expression::Name(name_struct) = &*attribute_struct.value {
                    if name_struct.value == "mock" && attribute_struct.attr.value == "mock" {
                        mock_aliases.push(alias.asname.clone());
                        continue;
                    }
                }
                clean_aliases.push(alias);
            }
        }
    }

    // But avoid destroying any trailing comments.
    if let Some(alias) = clean_aliases.last_mut() {
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

    (clean_aliases, mock_aliases)
}

/// Return `true` if the aliases contain `mock`.
fn includes_mock_member(aliases: &[ImportAlias]) -> bool {
    for alias in aliases {
        let ImportAlias { name, .. } = &alias;
        // Ex) `import mock.mock`
        if let NameOrAttribute::A(attribute_struct) = name {
            if let Expression::Name(name_struct) = &*attribute_struct.value {
                if name_struct.value == "mock" && attribute_struct.attr.value == "mock" {
                    return true;
                }
            }
        }
    }
    false
}

fn format_mocks(
    aliases: Vec<Option<AsName>>,
    indent: &str,
    stylist: &SourceCodeStyleDetector,
) -> String {
    let mut content = String::new();
    for alias in aliases {
        match alias {
            None => {
                if !content.is_empty() {
                    content.push_str(stylist.line_ending());
                    content.push_str(indent);
                }
                content.push_str("from unittest import mock");
            }
            Some(as_name) => {
                if let AssignTargetExpression::Name(name) = as_name.name {
                    if !content.is_empty() {
                        content.push_str(stylist.line_ending());
                        content.push_str(indent);
                    }
                    content.push_str("from unittest import mock as ");
                    content.push_str(name.value);
                }
            }
        }
    }
    content
}

/// Format the `import mock` rewrite.
fn format_import(
    stmt: &Stmt,
    indent: &str,
    locator: &SourceCodeLocator,
    stylist: &SourceCodeStyleDetector,
) -> Result<String> {
    let module_text = locator.slice_source_code_range(&Range::from_located(stmt));
    let mut tree = match_module(&module_text)?;
    let mut import = match_import(&mut tree)?;

    let Import { names, .. } = import.clone();
    let (clean_aliases, mock_aliases) = clean_import_aliases(names);

    Ok(if clean_aliases.is_empty() {
        format_mocks(mock_aliases, indent, stylist)
    } else {
        import.names = clean_aliases;

        let mut state = CodegenState::default();
        tree.codegen(&mut state);

        let mut content = state.to_string();
        content.push_str(stylist.line_ending());
        content.push_str(indent);
        content.push_str(&format_mocks(mock_aliases, indent, stylist));
        content
    })
}

/// Format the `from mock import ...` rewrite.
fn format_import_from(
    stmt: &Stmt,
    indent: &str,
    locator: &SourceCodeLocator,
    stylist: &SourceCodeStyleDetector,
) -> Result<String> {
    let module_text = locator.slice_source_code_range(&Range::from_located(stmt));
    let mut tree = match_module(&module_text).unwrap();
    let mut import = match_import_from(&mut tree)?;

    let ImportFrom {
        names: ImportNames::Aliases(aliases),
        ..
    } = import.clone() else {
        unreachable!("Expected ImportNames::Aliases");
    };

    let has_mock_member = includes_mock_member(&aliases);
    let (clean_aliases, mock_aliases) = clean_import_aliases(aliases);

    Ok(if clean_aliases.is_empty() {
        format_mocks(mock_aliases, indent, stylist)
    } else {
        import.names = ImportNames::Aliases(clean_aliases);
        import.module = Some(NameOrAttribute::A(Box::new(Attribute {
            value: Box::new(Expression::Name(Box::new(Name {
                value: "unittest",
                lpar: vec![],
                rpar: vec![],
            }))),
            attr: Name {
                value: "mock",
                lpar: vec![],
                rpar: vec![],
            },
            dot: Dot {
                whitespace_before: ParenthesizableWhitespace::default(),
                whitespace_after: ParenthesizableWhitespace::default(),
            },
            lpar: vec![],
            rpar: vec![],
        })));

        let mut state = CodegenState::default();
        tree.codegen(&mut state);

        let mut content = state.to_string();
        if has_mock_member {
            content.push_str(stylist.line_ending());
            content.push_str(indent);
            content.push_str(&format_mocks(mock_aliases, indent, stylist));
        }
        content
    })
}

/// UP026
pub fn rewrite_mock_attribute(checker: &mut Checker, expr: &Expr) {
    if let ExprKind::Attribute { value, .. } = &expr.node {
        if collect_call_paths(value) == ["mock", "mock"] {
            let mut check = Check::new(
                CheckKind::RewriteMockImport(MockReference::Attribute),
                Range::from_located(value),
            );
            if checker.patch(&CheckCode::UP026) {
                check.amend(Fix::replacement(
                    "mock".to_string(),
                    value.location,
                    value.end_location.unwrap(),
                ));
            }
            checker.add_check(check);
        }
    }
}

/// UP026
pub fn rewrite_mock_import(checker: &mut Checker, stmt: &Stmt) {
    match &stmt.node {
        StmtKind::Import { names } => {
            // Find all `mock` imports.
            if names
                .iter()
                .any(|name| name.node.name == "mock" || name.node.name == "mock.mock")
            {
                // Generate the fix, if needed, which is shared between all `mock` imports.
                let content = if checker.patch(&CheckCode::UP026) {
                    let indent = indentation(checker, stmt);
                    match format_import(stmt, &indent, checker.locator, checker.style) {
                        Ok(content) => Some(content),
                        Err(e) => {
                            error!("Failed to rewrite `mock` import: {e}");
                            None
                        }
                    }
                } else {
                    None
                };

                // Add a `Check` for each `mock` import.
                for name in names {
                    if name.node.name == "mock" || name.node.name == "mock.mock" {
                        let mut check = Check::new(
                            CheckKind::RewriteMockImport(MockReference::Import),
                            Range::from_located(name),
                        );
                        if let Some(content) = content.as_ref() {
                            check.amend(Fix::replacement(
                                content.clone(),
                                stmt.location,
                                stmt.end_location.unwrap(),
                            ));
                        }
                        checker.add_check(check);
                    }
                }
            }
        }
        StmtKind::ImportFrom {
            module: Some(module),
            level,
            ..
        } => {
            if level.map_or(false, |level| level > 0) {
                return;
            }

            if module == "mock" {
                let mut check = Check::new(
                    CheckKind::RewriteMockImport(MockReference::Import),
                    Range::from_located(stmt),
                );
                if checker.patch(&CheckCode::UP026) {
                    let indent = indentation(checker, stmt);
                    match format_import_from(stmt, &indent, checker.locator, checker.style) {
                        Ok(content) => {
                            check.amend(Fix::replacement(
                                content,
                                stmt.location,
                                stmt.end_location.unwrap(),
                            ));
                        }
                        Err(e) => error!("Failed to rewrite `mock` import: {e}"),
                    }
                }
                checker.add_check(check);
            }
        }
        _ => (),
    }
}
