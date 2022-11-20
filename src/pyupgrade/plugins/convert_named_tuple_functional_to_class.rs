use anyhow::{bail, Result};
use log::error;
use rustpython_ast::{Constant, Expr, ExprContext, ExprKind, Keyword, Stmt, StmtKind};

use crate::ast::helpers::match_module_member;
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::code_gen::SourceGenerator;
use crate::python::identifiers::IDENTIFIER_REGEX;
use crate::python::keyword::KWLIST;

/// Return the typename, args, keywords and mother class
fn match_named_tuple_assign<'a>(
    checker: &Checker,
    targets: &'a [Expr],
    value: &'a Expr,
) -> Option<(&'a str, &'a [Expr], &'a [Keyword], &'a ExprKind)> {
    if let Some(target) = targets.get(0) {
        if let ExprKind::Name { id: typename, .. } = &target.node {
            if let ExprKind::Call {
                func,
                args,
                keywords,
            } = &value.node
            {
                if match_module_member(
                    func,
                    "typing",
                    "NamedTuple",
                    &checker.from_imports,
                    &checker.import_aliases,
                ) {
                    return Some((typename, args, keywords, &func.node));
                }
            }
        }
    }
    None
}

/// Generate a `StmtKind::AnnAssign` representing the provided property
/// definition.
fn create_property_assignment_stmt(
    property: &str,
    annotation: &ExprKind,
    value: Option<&ExprKind>,
) -> Stmt {
    Stmt::new(
        Default::default(),
        Default::default(),
        StmtKind::AnnAssign {
            target: Box::new(Expr::new(
                Default::default(),
                Default::default(),
                ExprKind::Name {
                    id: property.to_string(),
                    ctx: ExprContext::Load,
                },
            )),
            annotation: Box::new(Expr::new(
                Default::default(),
                Default::default(),
                annotation.clone(),
            )),
            value: value
                .map(|v| Box::new(Expr::new(Default::default(), Default::default(), v.clone()))),
            simple: 1,
        },
    )
}

/// Match the `defaults` keyword in a `NamedTuple(...)` call.
fn match_defaults(keywords: &[Keyword]) -> Result<&[Expr]> {
    match keywords.iter().find(|keyword| {
        if let Some(arg) = &keyword.node.arg {
            arg.as_str() == "defaults"
        } else {
            false
        }
    }) {
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
    if let Some(fields) = args.get(1) {
        if let ExprKind::List { elts, .. } = &fields.node {
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
                    if let ExprKind::Tuple { elts, .. } = &field.node {
                        if let [field_name, annotation] = elts.as_slice() {
                            if let ExprKind::Constant {
                                value: Constant::Str(property),
                                ..
                            } = &field_name.node
                            {
                                if IDENTIFIER_REGEX.is_match(property)
                                    && !KWLIST.contains(&property.as_str())
                                {
                                    Ok(create_property_assignment_stmt(
                                        property,
                                        &annotation.node,
                                        default.map(|d| &d.node),
                                    ))
                                } else {
                                    bail!("Invalid property name: {}", property)
                                }
                            } else {
                                bail!("Expected `field_name` to be `Constant::Str`")
                            }
                        } else {
                            bail!("Expected `elts` to have exactly two elements")
                        }
                    } else {
                        bail!("Expected `field` to be `ExprKind::Tuple`")
                    }
                })
                .collect()
        } else {
            bail!("Expected argument to be `ExprKind::List`")
        }
    } else {
        Ok(vec![])
    }
}

/// Generate a `StmtKind:ClassDef` statement based on the provided body and
/// keywords.
fn create_class_def_stmt(typename: &str, body: Vec<Stmt>, base_class: &ExprKind) -> Stmt {
    Stmt::new(
        Default::default(),
        Default::default(),
        StmtKind::ClassDef {
            name: typename.to_string(),
            bases: vec![Expr::new(
                Default::default(),
                Default::default(),
                base_class.clone(),
            )],
            keywords: vec![],
            body,
            decorator_list: vec![],
        },
    )
}

fn convert_to_class(
    stmt: &Stmt,
    typename: &str,
    body: Vec<Stmt>,
    base_class: &ExprKind,
) -> Result<Fix> {
    let mut generator = SourceGenerator::new();
    generator.unparse_stmt(&create_class_def_stmt(typename, body, base_class))?;
    let content = generator.generate()?;
    Ok(Fix::replacement(
        content,
        stmt.location,
        stmt.end_location.unwrap(),
    ))
}

/// U014
pub fn convert_named_tuple_functional_to_class(
    checker: &mut Checker,
    stmt: &Stmt,
    targets: &[Expr],
    value: &Expr,
) {
    if let Some((typename, args, keywords, base_class)) =
        match_named_tuple_assign(checker, targets, value)
    {
        match match_defaults(keywords) {
            Ok(defaults) => {
                if let Ok(properties) = create_properties_from_args(args, defaults) {
                    let mut check = Check::new(
                        CheckKind::ConvertNamedTupleFunctionalToClass(typename.to_string()),
                        Range::from_located(stmt),
                    );
                    if checker.patch(check.kind.code()) {
                        match convert_to_class(stmt, typename, properties, base_class) {
                            Ok(fix) => check.amend(fix),
                            Err(err) => error!("Failed to convert `NamedTuple`: {}", err),
                        }
                    }
                    checker.add_check(check);
                }
            }
            Err(err) => error!("Failed to parse defaults: {}", err),
        }
    }
}
