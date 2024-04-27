use anyhow::Result;
use libcst_native::{
    AsName, AssignTargetExpression, Attribute, Dot, Expression, Import, ImportAlias, ImportFrom,
    ImportNames, Name, NameOrAttribute, ParenthesizableWhitespace,
};
use log::error;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::whitespace::indentation;
use ruff_python_ast::{self as ast, Stmt};
use ruff_python_codegen::Stylist;
use ruff_python_semantic::Modules;
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::cst::matchers::{match_import, match_import_from, match_statement};
use crate::fix::codemods::CodegenStylist;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum MockReference {
    Import,
    Attribute,
}

/// ## What it does
/// Checks for imports of the `mock` module that should be replaced with
/// `unittest.mock`.
///
/// ## Why is this bad?
/// Since Python 3.3, `mock` has been a part of the standard library as
/// `unittest.mock`. The `mock` package is deprecated; use `unittest.mock`
/// instead.
///
/// ## Example
/// ```python
/// import mock
/// ```
///
/// Use instead:
/// ```python
/// from unittest import mock
/// ```
///
/// ## References
/// - [Python documentation: `unittest.mock`](https://docs.python.org/3/library/unittest.mock.html)
/// - [PyPI: `mock`](https://pypi.org/project/mock/)
#[violation]
pub struct DeprecatedMockImport {
    reference_type: MockReference,
}

impl AlwaysFixableViolation for DeprecatedMockImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`mock` is deprecated, use `unittest.mock`")
    }

    fn fix_title(&self) -> String {
        let DeprecatedMockImport { reference_type } = self;
        match reference_type {
            MockReference::Import => "Import from `unittest.mock` instead".to_string(),
            MockReference::Attribute => "Replace `mock.mock` with `mock`".to_string(),
        }
    }
}

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

fn format_mocks(aliases: Vec<Option<AsName>>, indent: &str, stylist: &Stylist) -> String {
    let mut content = String::new();
    for alias in aliases {
        match alias {
            None => {
                if !content.is_empty() {
                    content.push_str(&stylist.line_ending());
                    content.push_str(indent);
                }
                content.push_str("from unittest import mock");
            }
            Some(as_name) => {
                if let AssignTargetExpression::Name(name) = as_name.name {
                    if !content.is_empty() {
                        content.push_str(&stylist.line_ending());
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
    locator: &Locator,
    stylist: &Stylist,
) -> Result<String> {
    let module_text = locator.slice(stmt);
    let mut tree = match_statement(module_text)?;
    let import = match_import(&mut tree)?;

    let Import { names, .. } = import.clone();
    let (clean_aliases, mock_aliases) = clean_import_aliases(names);

    Ok(if clean_aliases.is_empty() {
        format_mocks(mock_aliases, indent, stylist)
    } else {
        import.names = clean_aliases;

        let mut content = tree.codegen_stylist(stylist);
        content.push_str(&stylist.line_ending());
        content.push_str(indent);
        content.push_str(&format_mocks(mock_aliases, indent, stylist));
        content
    })
}

/// Format the `from mock import ...` rewrite.
fn format_import_from(
    stmt: &Stmt,
    indent: &str,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<String> {
    let module_text = locator.slice(stmt);
    let mut tree = match_statement(module_text).unwrap();
    let import = match_import_from(&mut tree)?;

    if let ImportFrom {
        names: ImportNames::Star(..),
        ..
    } = import
    {
        // Ex) `from mock import *`
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
        Ok(tree.codegen_stylist(stylist))
    } else if let ImportFrom {
        names: ImportNames::Aliases(aliases),
        ..
    } = import
    {
        // Ex) `from mock import mock`
        let (clean_aliases, mock_aliases) = clean_import_aliases(aliases.clone());
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

            let mut content = tree.codegen_stylist(stylist);
            if !mock_aliases.is_empty() {
                content.push_str(&stylist.line_ending());
                content.push_str(indent);
                content.push_str(&format_mocks(mock_aliases, indent, stylist));
            }
            content
        })
    } else {
        panic!("Expected ImportNames::Aliases | ImportNames::Star");
    }
}

/// UP026
pub(crate) fn deprecated_mock_attribute(checker: &mut Checker, attribute: &ast::ExprAttribute) {
    if !checker.semantic().seen_module(Modules::MOCK) {
        return;
    }

    if UnqualifiedName::from_expr(&attribute.value)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["mock", "mock"]))
    {
        let mut diagnostic = Diagnostic::new(
            DeprecatedMockImport {
                reference_type: MockReference::Attribute,
            },
            attribute.value.range(),
        );
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            "mock".to_string(),
            attribute.value.range(),
        )));
        checker.diagnostics.push(diagnostic);
    }
}

/// UP026
pub(crate) fn deprecated_mock_import(checker: &mut Checker, stmt: &Stmt) {
    match stmt {
        Stmt::Import(ast::StmtImport { names, range: _ }) => {
            // Find all `mock` imports.
            if names
                .iter()
                .any(|name| &name.name == "mock" || &name.name == "mock.mock")
            {
                // Generate the fix, if needed, which is shared between all `mock` imports.
                let content = if let Some(indent) = indentation(checker.locator(), stmt) {
                    match format_import(stmt, indent, checker.locator(), checker.stylist()) {
                        Ok(content) => Some(content),
                        Err(e) => {
                            error!("Failed to rewrite `mock` import: {e}");
                            None
                        }
                    }
                } else {
                    None
                };

                // Add a `Diagnostic` for each `mock` import.
                for name in names {
                    if &name.name == "mock" || &name.name == "mock.mock" {
                        let mut diagnostic = Diagnostic::new(
                            DeprecatedMockImport {
                                reference_type: MockReference::Import,
                            },
                            name.range(),
                        );
                        if let Some(content) = content.as_ref() {
                            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                                content.clone(),
                                stmt.range(),
                            )));
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                }
            }
        }
        Stmt::ImportFrom(ast::StmtImportFrom {
            module: Some(module),
            level,
            ..
        }) => {
            if *level > 0 {
                return;
            }

            if module == "mock" {
                let mut diagnostic = Diagnostic::new(
                    DeprecatedMockImport {
                        reference_type: MockReference::Import,
                    },
                    stmt.range(),
                );
                if let Some(indent) = indentation(checker.locator(), stmt) {
                    diagnostic.try_set_fix(|| {
                        format_import_from(stmt, indent, checker.locator(), checker.stylist())
                            .map(|content| Edit::range_replacement(content, stmt.range()))
                            .map(Fix::safe_edit)
                    });
                }
                checker.diagnostics.push(diagnostic);
            }
        }
        _ => (),
    }
}
