use anyhow::{bail, Result};
use log::debug;
use ruff_macros::{define_violation, derive_message_formats};
use ruff_python::identifiers::is_identifier;
use ruff_python::keyword::KWLIST;
use rustpython_parser::ast::{Constant, Expr, ExprContext, ExprKind, Keyword, Stmt, StmtKind};

use crate::ast::helpers::{create_expr, create_stmt, unparse_stmt};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::source_code::Stylist;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct ConvertTypedDictFunctionalToClass {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for ConvertTypedDictFunctionalToClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConvertTypedDictFunctionalToClass { name } = self;
        format!("Convert `{name}` from `TypedDict` functional to class syntax")
    }

    fn autofix_title(&self) -> String {
        let ConvertTypedDictFunctionalToClass { name } = self;
        format!("Convert `{name}` to class syntax")
    }
}

/// Return the class name, arguments, keywords and base class for a `TypedDict`
/// assignment.
fn match_typed_dict_assign<'a>(
    checker: &Checker,
    targets: &'a [Expr],
    value: &'a Expr,
) -> Option<(&'a str, &'a [Expr], &'a [Keyword], &'a Expr)> {
    let target = targets.get(0)?;
    let ExprKind::Name { id: class_name, .. } = &target.node else {
        return None;
    };
    let ExprKind::Call {
        func,
        args,
        keywords,
    } = &value.node else {
        return None;
    };
    if !checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["typing", "TypedDict"]
    }) {
        return None;
    }
    Some((class_name, args, keywords, func))
}

/// Generate a `StmtKind::AnnAssign` representing the provided property
/// definition.
fn create_property_assignment_stmt(property: &str, annotation: &ExprKind) -> Stmt {
    create_stmt(StmtKind::AnnAssign {
        target: Box::new(create_expr(ExprKind::Name {
            id: property.to_string(),
            ctx: ExprContext::Load,
        })),
        annotation: Box::new(create_expr(annotation.clone())),
        value: None,
        simple: 1,
    })
}

/// Generate a `StmtKind::Pass` statement.
fn create_pass_stmt() -> Stmt {
    create_stmt(StmtKind::Pass)
}

/// Generate a `StmtKind:ClassDef` statement based on the provided body,
/// keywords and base class.
fn create_class_def_stmt(
    class_name: &str,
    body: Vec<Stmt>,
    total_keyword: Option<&Keyword>,
    base_class: &Expr,
) -> Stmt {
    let keywords = match total_keyword {
        Some(keyword) => vec![keyword.clone()],
        None => vec![],
    };
    create_stmt(StmtKind::ClassDef {
        name: class_name.to_string(),
        bases: vec![base_class.clone()],
        keywords,
        body,
        decorator_list: vec![],
    })
}

fn properties_from_dict_literal(keys: &[Option<Expr>], values: &[Expr]) -> Result<Vec<Stmt>> {
    keys.iter()
        .zip(values.iter())
        .map(|(key, value)| match key {
            Some(Expr {
                node:
                    ExprKind::Constant {
                        value: Constant::Str(property),
                        ..
                    },
                ..
            }) => {
                if is_identifier(property) && !KWLIST.contains(&property.as_str()) {
                    Ok(create_property_assignment_stmt(property, &value.node))
                } else {
                    bail!("Property name is not valid identifier: {}", property)
                }
            }
            _ => bail!("Expected `key` to be `Constant::Str`"),
        })
        .collect()
}

fn properties_from_dict_call(func: &Expr, keywords: &[Keyword]) -> Result<Vec<Stmt>> {
    let ExprKind::Name { id, .. } = &func.node else {
        bail!("Expected `func` to be `ExprKind::Name`")
    };
    if id != "dict" {
        bail!("Expected `id` to be `\"dict\"`")
    }
    properties_from_keywords(keywords)
}

// Deprecated in Python 3.11, removed in Python 3.13.
fn properties_from_keywords(keywords: &[Keyword]) -> Result<Vec<Stmt>> {
    keywords
        .iter()
        .map(|keyword| {
            if let Some(property) = &keyword.node.arg {
                Ok(create_property_assignment_stmt(
                    property,
                    &keyword.node.value.node,
                ))
            } else {
                bail!("Expected `arg` to be `Some`")
            }
        })
        .collect()
}

// The only way to have the `total` keyword is to use the args version, like:
// ```
// TypedDict('name', {'a': int}, total=True)
// ```
fn match_total_from_only_keyword(keywords: &[Keyword]) -> Option<&Keyword> {
    let keyword = keywords.get(0)?;
    let arg = &keyword.node.arg.as_ref()?;
    match arg.as_str() {
        "total" => Some(keyword),
        _ => None,
    }
}

fn match_properties_and_total<'a>(
    args: &'a [Expr],
    keywords: &'a [Keyword],
) -> Result<(Vec<Stmt>, Option<&'a Keyword>)> {
    // We don't have to manage the hybrid case because it's not possible to have a
    // dict and keywords. For example, the following is illegal:
    // ```
    // MyType = TypedDict('MyType', {'a': int, 'b': str}, a=int, b=str)
    // ```
    if let Some(dict) = args.get(1) {
        let total = match_total_from_only_keyword(keywords);
        match &dict.node {
            ExprKind::Dict { keys, values } => {
                Ok((properties_from_dict_literal(keys, values)?, total))
            }
            ExprKind::Call { func, keywords, .. } => {
                Ok((properties_from_dict_call(func, keywords)?, total))
            }
            _ => Ok((vec![create_pass_stmt()], total)),
        }
    } else if !keywords.is_empty() {
        Ok((properties_from_keywords(keywords)?, None))
    } else {
        Ok((vec![create_pass_stmt()], None))
    }
}

/// Generate a `Fix` to convert a `TypedDict` from functional to class.
fn convert_to_class(
    stmt: &Stmt,
    class_name: &str,
    body: Vec<Stmt>,
    total_keyword: Option<&Keyword>,
    base_class: &Expr,
    stylist: &Stylist,
) -> Fix {
    Fix::replacement(
        unparse_stmt(
            &create_class_def_stmt(class_name, body, total_keyword, base_class),
            stylist,
        ),
        stmt.location,
        stmt.end_location.unwrap(),
    )
}

/// UP013
pub fn convert_typed_dict_functional_to_class(
    checker: &mut Checker,
    stmt: &Stmt,
    targets: &[Expr],
    value: &Expr,
) {
    let Some((class_name, args, keywords, base_class)) =
        match_typed_dict_assign(checker, targets, value) else
    {
        return;
    };

    let mut diagnostic = Diagnostic::new(
        ConvertTypedDictFunctionalToClass {
            name: class_name.to_string(),
        },
        Range::from_located(stmt),
    );
    // TODO(charlie): Preserve indentation, to remove the first-column requirement.
    if checker.patch(diagnostic.kind.rule()) && stmt.location.column() == 0 {
        match match_properties_and_total(args, keywords) {
            Ok((body, total_keyword)) => {
                diagnostic.amend(convert_to_class(
                    stmt,
                    class_name,
                    body,
                    total_keyword,
                    base_class,
                    checker.stylist,
                ));
            }
            Err(err) => debug!("Skipping ineligible `TypedDict` \"{class_name}\": {err}"),
        };
    }
    checker.diagnostics.push(diagnostic);
}
