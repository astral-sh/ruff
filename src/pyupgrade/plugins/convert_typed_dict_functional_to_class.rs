use anyhow::{anyhow, bail, Result};
use log::error;
use rustpython_ast::{Constant, Expr, ExprContext, ExprKind, Keyword, KeywordData, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::code_gen::SourceGenerator;
use crate::python::identifiers::IDENTIFIER_REGEX;
use crate::python::keyword::KWLIST;

// Returns (class_name, args, keywords)
fn match_typed_dict_assign<'a>(
    targets: &'a [Expr],
    value: &'a Expr,
) -> Option<(&'a str, &'a [Expr], &'a [Keyword])> {
    if let Some(target) = targets.get(0) {
        if let ExprKind::Name { id: class_name, .. } = &target.node {
            if let ExprKind::Call {
                func,
                args,
                keywords,
            } = &value.node
            {
                if let ExprKind::Name { id: func_name, .. } = &func.node {
                    if func_name == "TypedDict" {
                        return Some((class_name, args, keywords));
                    }
                }
            }
        }
    }
    None
}

fn create_property_assignment_stmt(property: &String, type_ann: &ExprKind) -> Stmt {
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
                type_ann.clone(),
            )),
            value: None,
            simple: 1,
        },
    )
}

fn create_pass_stmt() -> Stmt {
    Stmt::new(Default::default(), Default::default(), StmtKind::Pass)
}

fn create_classdef_stmt(
    class_name: &str,
    body: Vec<Stmt>,
    total_keyword: Option<KeywordData>,
) -> Stmt {
    let keywords = match total_keyword {
        Some(keyword) => vec![Keyword::new(
            Default::default(),
            Default::default(),
            keyword,
        )],
        None => vec![],
    };
    Stmt::new(
        Default::default(),
        Default::default(),
        StmtKind::ClassDef {
            name: class_name.to_string(),
            bases: vec![Expr::new(
                Default::default(),
                Default::default(),
                ExprKind::Name {
                    id: "TypedDict".to_string(),
                    ctx: ExprContext::Load,
                },
            )],
            keywords,
            body,
            decorator_list: vec![],
        },
    )
}

fn get_properties_from_dict_literal(keys: &[Expr], values: &[Expr]) -> Result<Vec<Stmt>> {
    keys.iter()
        .zip(values.iter())
        .map(|(key, value)| match &key.node {
            ExprKind::Constant {
                value: Constant::Str(property),
                ..
            } => {
                if IDENTIFIER_REGEX.is_match(property) && !KWLIST.contains(&property.as_str()) {
                    Ok(create_property_assignment_stmt(property, &value.node))
                } else {
                    bail!("Invalid property name: {}", property)
                }
            }
            _ => bail!("key is not a Str"),
        })
        .collect()
}

fn get_properties_from_dict_call(func: &Expr, keywords: &[Keyword]) -> Result<Vec<Stmt>> {
    match &func.node {
        ExprKind::Name { id: func_name, .. } => match func_name.as_str() {
            "dict" => keywords
                .iter()
                .map(|keyword| {
                    let property = &keyword.node.arg.clone().ok_or_else(|| anyhow!("no arg"))?;
                    Ok(create_property_assignment_stmt(
                        property,
                        &keyword.node.value.node,
                    ))
                })
                .collect(),
            _ => bail!("func is not dict"),
        },
        _ => bail!("func is not a Name"),
    }
}

// Deprecated in Python 3.11, Removed in Python 3.13?
fn get_properties_from_keywords(keywords: &[Keyword]) -> Result<Vec<Stmt>> {
    keywords
        .iter()
        .map(|keyword| {
            let property = &keyword.node.arg.clone().ok_or_else(|| anyhow!("no arg"))?;
            Ok(create_property_assignment_stmt(
                property,
                &keyword.node.value.node,
            ))
        })
        .collect()
}

// The only way to have total keyword is to use the arg version
// (`TypedDict('name', {'a': int}, total=True)`)
fn get_total_from_only_keyword(keywords: &[Keyword]) -> Option<KeywordData> {
    match keywords.get(0) {
        Some(keyword) => match &keyword.node.arg {
            Some(arg) => match arg.as_str() {
                "total" => Some(keyword.node.clone()),
                _ => None,
            },
            None => None,
        },
        None => None,
    }
}

fn get_properties_and_total(
    args: &[Expr],
    keywords: &[Keyword],
) -> Result<(Vec<Stmt>, Option<KeywordData>)> {
    if let Some(dict) = args.get(1) {
        let total = get_total_from_only_keyword(keywords);
        match &dict.node {
            ExprKind::Dict { keys, values } => {
                Ok((get_properties_from_dict_literal(keys, values)?, total))
            }
            ExprKind::Call { func, keywords, .. } => {
                Ok((get_properties_from_dict_call(func, keywords)?, total))
            }
            _ => Ok((vec![create_pass_stmt()], total)),
        }
    } else if !keywords.is_empty() {
        Ok((get_properties_from_keywords(keywords)?, None))
    } else {
        Ok((vec![create_pass_stmt()], None))
    }
}

fn try_to_fix(
    stmt: &Stmt,
    class_name: &str,
    body: Vec<Stmt>,
    total_keyword: Option<KeywordData>,
) -> Result<Fix> {
    // We don't have to manage an hybrid case because it's not possible to have a
    // dict and keywords Illegal: `MyType = TypedDict('MyType', {'a': int, 'b':
    // str}, a=int, b=str)`
    let classdef_stmt = create_classdef_stmt(class_name, body, total_keyword);
    let mut generator = SourceGenerator::new();
    generator.unparse_stmt(&classdef_stmt)?;
    let content = generator.generate()?;
    Ok(Fix::replacement(
        content,
        stmt.location,
        stmt.end_location.unwrap(),
    ))
}

/// U013
pub fn convert_typed_dict_functional_to_class(
    checker: &mut Checker,
    stmt: &Stmt,
    targets: &[Expr],
    value: &Expr,
) {
    if let Some((class_name, args, keywords)) = match_typed_dict_assign(targets, value) {
        match get_properties_and_total(args, keywords) {
            Err(err) => error!("Failed to parse TypedDict: {}", err),
            Ok((body, total_keyword)) => {
                let mut check = Check::new(
                    CheckKind::ConvertTypedDictFunctionalToClass,
                    Range::from_located(stmt),
                );
                if checker.patch() {
                    match try_to_fix(stmt, class_name, body, total_keyword) {
                        Ok(fix) => check.amend(fix),
                        Err(err) => error!("Failed to convert TypedDict: {}", err),
                    };
                }
                checker.add_check(check);
            }
        }
    }
}
