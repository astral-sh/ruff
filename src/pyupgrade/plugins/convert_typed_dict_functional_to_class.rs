use anyhow::{bail, Result};
use log::error;
use rustpython_ast::{Constant, Expr, ExprContext, ExprKind, Keyword, KeywordData, Stmt, StmtKind};

use crate::ast::helpers::match_module_member;
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::code_gen::SourceGenerator;
use crate::python::identifiers::IDENTIFIER_REGEX;
use crate::python::keyword::KWLIST;

/// Return the class name, arguments, keywords and base class for a `TypedDict`
/// assignment.
fn match_typed_dict_assign<'a>(
    checker: &Checker,
    targets: &'a [Expr],
    value: &'a Expr,
) -> Option<(&'a str, &'a [Expr], &'a [Keyword], &'a ExprKind)> {
    if let Some(target) = targets.get(0) {
        if let ExprKind::Name { id: class_name, .. } = &target.node {
            if let ExprKind::Call {
                func,
                args,
                keywords,
            } = &value.node
            {
                if match_module_member(
                    func,
                    "typing",
                    "TypedDict",
                    &checker.from_imports,
                    &checker.import_aliases,
                ) {
                    return Some((class_name, args, keywords, &func.node));
                }
            }
        }
    }
    None
}

/// Generate a `StmtKind::AnnAssign` representing the provided property
/// definition.
fn create_property_assignment_stmt(property: &str, annotation: &ExprKind) -> Stmt {
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
            value: None,
            simple: 1,
        },
    )
}

/// Generate a `StmtKind::Pass` statement.
fn create_pass_stmt() -> Stmt {
    Stmt::new(Default::default(), Default::default(), StmtKind::Pass)
}

/// Generate a `StmtKind:ClassDef` statement based on the provided body,
/// keywords and base class.
fn create_class_def_stmt(
    class_name: &str,
    body: Vec<Stmt>,
    total_keyword: Option<KeywordData>,
    base_class: &ExprKind,
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
                base_class.clone(),
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
            _ => bail!("Expected `key` to be `Constant::Str`"),
        })
        .collect()
}

fn get_properties_from_dict_call(func: &Expr, keywords: &[Keyword]) -> Result<Vec<Stmt>> {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "dict" {
            get_properties_from_keywords(keywords)
        } else {
            bail!("Expected `id` to be `\"dict\"`")
        }
    } else {
        bail!("Expected `func` to be `ExprKind::Name`")
    }
}

// Deprecated in Python 3.11, removed in Python 3.13.
fn get_properties_from_keywords(keywords: &[Keyword]) -> Result<Vec<Stmt>> {
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
// (`TypedDict('name', {'a': int}, total=True)`)
fn get_total_from_only_keyword(keywords: &[Keyword]) -> Option<&KeywordData> {
    match keywords.get(0) {
        Some(keyword) => match &keyword.node.arg {
            Some(arg) => match arg.as_str() {
                "total" => Some(&keyword.node),
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
    // We don't have to manage the hybrid case because it's not possible to have a
    // dict and keywords. For example, the following is illegal:
    //   MyType = TypedDict('MyType', {'a': int, 'b': str}, a=int, b=str)
    if let Some(dict) = args.get(1) {
        let total = get_total_from_only_keyword(keywords).cloned();
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

/// Generate a `Fix` to convert a `TypedDict` from functional to class.
fn convert_to_class(
    stmt: &Stmt,
    class_name: &str,
    body: Vec<Stmt>,
    total_keyword: Option<KeywordData>,
    base_class: &ExprKind,
) -> Result<Fix> {
    let mut generator = SourceGenerator::new();
    generator.unparse_stmt(&create_class_def_stmt(
        class_name,
        body,
        total_keyword,
        base_class,
    ))?;
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
    if let Some((class_name, args, keywords, base_class)) =
        match_typed_dict_assign(checker, targets, value)
    {
        match get_properties_and_total(args, keywords) {
            Err(err) => error!("Failed to parse TypedDict: {}", err),
            Ok((body, total_keyword)) => {
                let mut check = Check::new(
                    CheckKind::ConvertTypedDictFunctionalToClass(class_name.to_string()),
                    Range::from_located(stmt),
                );
                if checker.patch(check.kind.code()) {
                    match convert_to_class(stmt, class_name, body, total_keyword, base_class) {
                        Ok(fix) => check.amend(fix),
                        Err(err) => error!("Failed to convert TypedDict: {}", err),
                    };
                }
                checker.add_check(check);
            }
        }
    }
}
