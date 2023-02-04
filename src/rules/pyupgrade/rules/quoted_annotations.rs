use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::settings::types::PythonVersion;
use crate::violation::AlwaysAutofixableViolation;
use ruff_macros::derive_message_formats;
use rustpython_ast::{
    Arg, Arguments, Comprehension, Constant, Expr, ExprKind, Keyword, Stmt, StmtKind,
};

define_violation!(
    pub struct QuotedAnnotations;
);
impl AlwaysAutofixableViolation for QuotedAnnotations {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Removed quotes from the type annotations")
    }

    fn autofix_title(&self) -> String {
        "Removed the quotes".to_string()
    }
}

const FUNC_NAMES: &[&str] = &[
    "Arg",
    "DefaultArg",
    "NamedArg",
    "DefaultNamedArg",
    "VarArg",
    "KwArg",
];

fn argument_list(args: &Arguments) -> Vec<Arg> {
    let mut final_result: Vec<Arg> = vec![];
    final_result.extend(args.posonlyargs.clone());
    final_result.extend(args.args.clone());
    if let Some(item) = &args.vararg {
        final_result.push(*item.clone());
    }
    final_result.extend(args.kwonlyargs.clone());
    if let Some(item) = &args.kwarg {
        final_result.push(*item.clone());
    }
    final_result
}

fn get_name(expr: &Expr) -> Result<String, ()> {
    match &expr.node {
        ExprKind::Name { id, .. } => Ok(id.to_string()),
        ExprKind::Attribute { value, .. } => get_name(value),
        _ => Err(()),
    }
}

fn remove_quotes(checker: &mut Checker, annotation: &Expr) {
    if let ExprKind::Constant {
        value: Constant::Str(type_str),
        ..
    } = &annotation.node
    {
        let mut diagnostic = Diagnostic::new(QuotedAnnotations, Range::from_located(annotation));
        if checker.patch(&Rule::PrintfStringFormatting) {
            diagnostic.amend(Fix::replacement(
                type_str.to_string(),
                annotation.location,
                annotation.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}

fn get_keyword_value<'a>(keywords: &'a Vec<Keyword>, name: &str) -> Option<&'a Expr> {
    for keyword in keywords {
        let kw = match &keyword.node.arg {
            Some(item) => item,
            None => continue,
        };
        if kw == name {
            return Some(&keyword.node.value);
        }
    }
    None
}

fn process_call<'a>(
    func: &Expr,
    args: &'a Vec<Expr>,
    keywords: &'a Vec<Keyword>,
) -> Result<Vec<&'a Expr>, ()> {
    let mut to_add: Vec<&Expr> = vec![];
    let name = get_name(func)?;
    if name == "TypedDict" {
        if !keywords.is_empty() {
            for keyword in keywords {
                to_add.push(&keyword.node.value);
            }
        } else if args.len() != 2 {
            // Garbage
        } else if let ExprKind::Dict { values, .. } = &args.get(1).unwrap().node {
            for value in values {
                to_add.push(value);
            }
        } else {
            return Err(());
        }
    } else if name == "NamedTuple" {
        let fields: &Expr;
        if args.len() == 2 {
            fields = args.get(1).unwrap();
        } else if !keywords.is_empty() {
            fields = match get_keyword_value(keywords, "fields") {
                Some(item) => item,
                // If there is no fields keyword, we don't need to make changes
                None => return Err(()),
            };
        } else {
            // If we don't have fields to return, we end early
            return Err(());
        }

        if let ExprKind::List { elts, .. } = &fields.node {
            for elt in elts {
                if let ExprKind::Tuple { elts, .. } = &elt.node {
                    if elts.len() == 2 {
                        to_add.push(elts.get(1).unwrap());
                    }
                }
            }
        }
    } else if FUNC_NAMES.contains(&&name[..]) {
        if args.is_empty() {
            let keyword_value = match get_keyword_value(keywords, "type") {
                Some(item) => item,
                None => return Err(()),
            };
            to_add.push(keyword_value);
        } else {
            to_add.push(args.get(0).unwrap());
        }
    }
    Ok(to_add)
}

fn get_comprehension(comp: &Comprehension) -> Vec<&Expr> {
    let mut to_add: Vec<&Expr> = vec![];
    to_add.push(&comp.target);
    to_add.push(&comp.iter);
    for if_state in &comp.ifs {
        to_add.push(if_state);
    }
    to_add
}

fn get_any(node: &Expr) -> Vec<&Expr> {
    let mut to_add: Vec<&Expr> = vec![];
    match &node.node {
        ExprKind::BoolOp { values, .. } => {
            for value in values {
                to_add.push(value);
            }
        }
        ExprKind::NamedExpr { target, value } => {
            to_add.push(target);
            to_add.push(value);
        }
        ExprKind::BinOp { left, right, .. } => {
            to_add.push(left);
            to_add.push(right);
        }
        ExprKind::UnaryOp { operand, .. } => {
            to_add.push(operand);
        }
        ExprKind::IfExp { test, body, orelse } => {
            to_add.push(test);
            to_add.push(body);
            to_add.push(orelse);
        }
        ExprKind::Dict { keys, values } => {
            to_add.extend(keys.iter().flatten());
            for value in values {
                to_add.push(value);
            }
        }
        ExprKind::Set { elts } => {
            for elt in elts {
                to_add.push(elt);
            }
        }
        ExprKind::ListComp { elt, generators }
        | ExprKind::SetComp { elt, generators }
        | ExprKind::GeneratorExp { elt, generators } => {
            to_add.push(elt);
            for generator in generators {
                to_add.extend(get_comprehension(generator));
            }
        }
        ExprKind::DictComp {
            key,
            value,
            generators,
        } => {
            to_add.push(key);
            to_add.push(value);
            for generator in generators {
                to_add.extend(get_comprehension(generator));
            }
        }
        ExprKind::Await { value } | ExprKind::YieldFrom { value } => {
            to_add.push(value);
        }
        ExprKind::Yield { value: Some(item) } => {
            to_add.push(item);
        }
        ExprKind::Compare {
            left, comparators, ..
        } => {
            to_add.push(left);
            for comparator in comparators {
                to_add.push(comparator);
            }
        }
        ExprKind::Call {
            func,
            args,
            keywords,
        } => {
            to_add.push(func);
            for arg in args {
                to_add.push(arg);
            }
            for keyword in keywords {
                to_add.push(&keyword.node.value);
            }
        }
        ExprKind::FormattedValue {
            value, format_spec, ..
        } => {
            to_add.push(value);
            if let Some(item) = format_spec {
                to_add.push(item);
            }
        }
        ExprKind::JoinedStr { values } => {
            for value in values {
                to_add.push(value);
            }
        }
        ExprKind::Attribute { value, .. } => {
            to_add.push(value);
        }
        ExprKind::Subscript { value, slice, .. } => {
            to_add.push(value);
            to_add.push(slice);
        }
        ExprKind::Starred { value, .. } => to_add.push(value),
        ExprKind::List { elts, .. } | ExprKind::Tuple { elts, .. } => {
            for elt in elts {
                to_add.push(elt);
            }
        }
        ExprKind::Slice { lower, upper, step } => {
            if let Some(item) = lower {
                to_add.push(item);
            }
            if let Some(item) = upper {
                to_add.push(item);
            }
            if let Some(item) = step {
                to_add.push(item);
            }
        }
        _ => (),
    }
    to_add
}

fn process_subscript<'a>(
    value: &Expr,
    slice: &'a Expr,
    py_version: PythonVersion,
) -> Result<Vec<&'a Expr>, ()> {
    let mut to_add: Vec<&Expr> = vec![];
    let name = get_name(&Box::new(value.clone()))?;
    if name == "Annotated" {
        let node_slice: &Expr = if py_version >= PythonVersion::Py39 {
            slice
        // FOR REVIEWER: There is a potential issue here, pyupgrade has a special case if there is
        // an Index token here. Index tokens were removed in python 3.9. rustpython only covers
        // python >= 3.10, so it does not have access to the index token. How should we proceed?
        // Pyupgrade had an elif with the Index token
        } else {
            slice
        };

        if let ExprKind::Tuple { elts, .. } = &node_slice.node {
            if let Some(item) = elts.get(0) {
                to_add.push(item);
            }
        }
    } else if name != "Literal" {
        to_add.push(slice);
    }
    Ok(to_add)
}

fn replace_string_literal(annotation: &Expr, checker: &mut Checker) {
    let mut nodes: Vec<&Expr> = vec![annotation];
    while !nodes.is_empty() {
        let node = match nodes.pop() {
            Some(item) => item,
            None => continue,
        };
        match &node.node {
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                let new_nodes = match process_call(func, args, keywords) {
                    Ok(item) => item,
                    Err(_) => continue,
                };
                nodes.extend(new_nodes);
            }
            ExprKind::Subscript { value, slice, .. } => {
                let new_nodes =
                    match process_subscript(value, slice, checker.settings.target_version) {
                        Ok(item) => item,
                        Err(_) => continue,
                    };
                nodes.extend(new_nodes);
            }
            ExprKind::Constant { value, .. } => {
                if let Constant::Str(_) = value {
                    remove_quotes(checker, node);
                }
            }
            _ => nodes.extend(get_any(node)),
        }
    }
}

fn handle_functiondef(args: &Arguments, returns: &Option<Box<Expr>>, checker: &mut Checker) {
    if let Some(return_item) = returns {
        replace_string_literal(return_item, checker);
    }
    let arg_list = argument_list(args);
    for arg in arg_list {
        if let Some(annotation) = arg.node.annotation {
            replace_string_literal(&annotation, checker);
        }
    }
}

/// UP037
pub fn quoted_annotations(checker: &mut Checker, stmt: &Stmt) {
    match &stmt.node {
        StmtKind::FunctionDef { args, returns, .. }
        | StmtKind::AsyncFunctionDef { args, returns, .. } => {
            handle_functiondef(args, returns, checker);
        }
        StmtKind::AnnAssign { annotation, .. } => {
            replace_string_literal(annotation, checker);
        }
        _ => (),
    }
}
