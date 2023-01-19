use anyhow::{bail, Result};
use log::debug;
use rustpython_ast::{Constant, Expr, ExprContext, ExprKind, Keyword, Stmt, StmtKind};

use crate::ast::helpers::{create_expr, create_stmt, unparse_stmt};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::python::identifiers::IDENTIFIER_REGEX;
use crate::python::keyword::KWLIST;
use crate::registry::Diagnostic;
use crate::source_code::Stylist;
use crate::violations;

/// Return the typename, args, keywords, and base class.
fn match_named_tuple_assign<'a>(
    checker: &Checker,
    targets: &'a [Expr],
    value: &'a Expr,
) -> Option<(&'a str, &'a [Expr], &'a [Keyword], &'a Expr)> {
    let target = targets.get(0)?;
    let ExprKind::Name { id: typename, .. } = &target.node else {
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
        call_path.as_slice() == ["typing", "NamedTuple"]
    }) {
        return None;
    }
    Some((typename, args, keywords, func))
}

/// Generate a `StmtKind::AnnAssign` representing the provided property
/// definition.
fn create_property_assignment_stmt(
    property: &str,
    annotation: &Expr,
    value: Option<&Expr>,
) -> Stmt {
    create_stmt(StmtKind::AnnAssign {
        target: Box::new(create_expr(ExprKind::Name {
            id: property.to_string(),
            ctx: ExprContext::Load,
        })),
        annotation: Box::new(annotation.clone()),
        value: value.map(|value| Box::new(value.clone())),
        simple: 1,
    })
}

/// Match the `defaults` keyword in a `NamedTuple(...)` call.
fn match_defaults(keywords: &[Keyword]) -> Result<&[Expr]> {
    let defaults = keywords.iter().find(|keyword| {
        if let Some(arg) = &keyword.node.arg {
            arg.as_str() == "defaults"
        } else {
            false
        }
    });
    match defaults {
        Some(defaults) => match &defaults.node.value.node {
            ExprKind::List { elts, .. } => Ok(elts),
            ExprKind::Tuple { elts, .. } => Ok(elts),
            _ => bail!("Expected defaults to be `ExprKind::List` | `ExprKind::Tuple`"),
        },
        None => Ok(&[]),
    }
}

/// Create a list of property assignments from the `NamedTuple` arguments.
fn create_properties_from_args(args: &[Expr], defaults: &[Expr]) -> Result<Vec<Stmt>> {
    let Some(fields) = args.get(1) else {
        return Ok(vec![]);
    };
    let ExprKind::List { elts, .. } = &fields.node else {
        bail!("Expected argument to be `ExprKind::List`");
    };
    let padded_defaults = if elts.len() >= defaults.len() {
        std::iter::repeat(None)
            .take(elts.len() - defaults.len())
            .chain(defaults.iter().map(Some))
    } else {
        bail!("Defaults must be `None` or an iterable of at least the number of fields")
    };
    elts.iter()
        .zip(padded_defaults)
        .map(|(field, default)| {
            let ExprKind::Tuple { elts, .. } = &field.node else {
                bail!("Expected `field` to be `ExprKind::Tuple`")
            };
            let [field_name, annotation] = elts.as_slice() else {
                bail!("Expected `elts` to have exactly two elements")
            };
            let ExprKind::Constant {
                value: Constant::Str(property),
                ..
            } = &field_name.node else {
                bail!("Expected `field_name` to be `Constant::Str`")
            };
            if !IDENTIFIER_REGEX.is_match(property) || KWLIST.contains(&property.as_str()) {
                bail!("Invalid property name: {}", property)
            }
            Ok(create_property_assignment_stmt(
                property, annotation, default,
            ))
        })
        .collect()
}

/// Generate a `StmtKind:ClassDef` statement based on the provided body and
/// keywords.
fn create_class_def_stmt(typename: &str, body: Vec<Stmt>, base_class: &Expr) -> Stmt {
    create_stmt(StmtKind::ClassDef {
        name: typename.to_string(),
        bases: vec![base_class.clone()],
        keywords: vec![],
        body,
        decorator_list: vec![],
    })
}

/// Generate a `Fix` to convert a `NamedTuple` assignment to a class definition.
fn convert_to_class(
    stmt: &Stmt,
    typename: &str,
    body: Vec<Stmt>,
    base_class: &Expr,
    stylist: &Stylist,
) -> Fix {
    Fix::replacement(
        unparse_stmt(&create_class_def_stmt(typename, body, base_class), stylist),
        stmt.location,
        stmt.end_location.unwrap(),
    )
}

/// UP014
pub fn convert_named_tuple_functional_to_class(
    checker: &mut Checker,
    stmt: &Stmt,
    targets: &[Expr],
    value: &Expr,
) {
    let Some((typename, args, keywords, base_class)) =
        match_named_tuple_assign(checker, targets, value) else
    {
        return;
    };
    let mut diagnostic = Diagnostic::new(
        violations::ConvertNamedTupleFunctionalToClass(typename.to_string()),
        Range::from_located(stmt),
    );
    if checker.patch(diagnostic.kind.rule()) {
        match match_defaults(keywords)
            .and_then(|defaults| create_properties_from_args(args, defaults))
        {
            Ok(properties) => {
                diagnostic.amend(convert_to_class(
                    stmt,
                    typename,
                    properties,
                    base_class,
                    checker.stylist,
                ));
            }
            Err(err) => debug!("Skipping ineligible `NamedTuple` \"{typename}\": {err}"),
        };
    }
    checker.diagnostics.push(diagnostic);
}
