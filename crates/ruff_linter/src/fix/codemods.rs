//! Interface for editing code snippets. These functions take statements or expressions as input,
//! and return the modified code snippet as output.
use std::borrow::Cow;

use anyhow::{bail, Result};
use libcst_native::{
    Codegen, CodegenState, Expression, ImportNames, NameOrAttribute, ParenthesizableWhitespace,
    SmallStatement, Statement,
};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::{smallvec, SmallVec};
use unicode_normalization::UnicodeNormalization;

use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::Stmt;
use ruff_python_codegen::Stylist;

use crate::cst::matchers::match_statement;
use crate::Locator;

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

    // Remove any imports that are specified in the `imports` iterator (but, e.g., if the name is
    // provided once, only remove the first occurrence).
    let mut counts = member_names.fold(FxHashMap::<&str, usize>::default(), |mut map, name| {
        map.entry(name).and_modify(|c| *c += 1).or_insert(1);
        map
    });
    aliases.retain(|alias| {
        let name = qualified_name_from_name_or_attribute(&alias.name);
        if let Some(count) = counts.get_mut(name.as_str()).filter(|count| **count > 0) {
            *count -= 1;
            false
        } else {
            true
        }
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

    // Retain any imports that are specified in the `imports` iterator.
    let member_names = member_names.iter().copied().collect::<FxHashSet<_>>();
    aliases.retain(|alias| {
        member_names.contains(qualified_name_from_name_or_attribute(&alias.name).as_str())
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

/// Create an NFKC-normalized qualified name from a libCST node.
fn qualified_name_from_name_or_attribute(module: &NameOrAttribute) -> String {
    fn collect_segments<'a>(expr: &'a Expression, parts: &mut SmallVec<[&'a str; 8]>) {
        match expr {
            Expression::Call(expr) => {
                collect_segments(&expr.func, parts);
            }
            Expression::Attribute(expr) => {
                collect_segments(&expr.value, parts);
                parts.push(expr.attr.value);
            }
            Expression::Name(expr) => {
                parts.push(expr.value);
            }
            _ => {}
        }
    }

    /// Attempt to create an [`UnqualifiedName`] from a libCST expression.
    ///
    /// Strictly speaking, the `UnqualifiedName` returned by this function may be invalid,
    /// since it hasn't been NFKC-normalized. In order for an `UnqualifiedName` to be
    /// comparable to one constructed from a `ruff_python_ast` node, it has to undergo
    /// NFKC normalization. As a local function, however, this is fine;
    /// the outer function always performs NFKC normalization before returning the
    /// qualified name to the caller.
    fn unqualified_name_from_expression<'a>(
        expr: &'a Expression<'a>,
    ) -> Option<UnqualifiedName<'a>> {
        let mut segments = smallvec![];
        collect_segments(expr, &mut segments);
        if segments.is_empty() {
            None
        } else {
            Some(segments.into_iter().collect())
        }
    }

    let unnormalized = match module {
        NameOrAttribute::N(name) => Cow::Borrowed(name.value),
        NameOrAttribute::A(attr) => {
            let name = attr.attr.value;
            let prefix = unqualified_name_from_expression(&attr.value);
            prefix.map_or_else(
                || Cow::Borrowed(name),
                |prefix| Cow::Owned(format!("{prefix}.{name}")),
            )
        }
    };

    unnormalized.nfkc().collect()
}
