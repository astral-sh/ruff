use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::violation::AlwaysAutofixableViolation;
use ruff_macros::derive_message_formats;
use rustpython_ast::{Arg, Arguments, Constant, Expr, ExprKind, Keyword, Stmt, StmtKind};

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

fn argument_list(args: &Arguments) -> Vec<Arg> {
    let mut final_result: Vec<Arg> = vec![];
    final_result.extend(args.posonlyargs.clone());
    final_result.extend(args.args.clone());
    final_result.extend(args.kwonlyargs.clone());
    final_result
}

fn get_name(expr: &Box<Expr>) -> Result<String, ()> {
    match &expr.node {
        ExprKind::Name { id, .. } => Ok(id.to_string()),
        ExprKind::Attribute { value, .. } => get_name(&value),
        _ => Err(()),
    }
}

fn remove_quotes(checker: &mut Checker, annotation: &Box<Expr>) {
    if let ExprKind::Constant { value, .. } = &annotation.node {
        if let Constant::Str(type_str) = value {
            let mut diagnostic =
                Diagnostic::new(QuotedAnnotations, Range::from_located(&annotation));
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
}

fn process_call<'a>(
    func: &Box<Expr>,
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
                to_add.push(&value);
            }
        } else {
            return Err(());
        }
    }
    Ok(to_add)
}

fn replace_string_literal(annotation: Box<Expr>) {
    let mut nodes: Vec<&Expr> = vec![annotation.as_ref()];
    while nodes.len() > 0 {
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
            _ => continue,
        }
    }
}

fn handle_functiondef(args: &Box<Arguments>, returns: &Option<Box<Expr>>) {
    if let Some(return_item) = returns {
        replace_string_literal(return_item.clone());
    }
    let arg_list = argument_list(&args);
    for arg in arg_list {
        if let Some(annotation) = arg.node.annotation {
            replace_string_literal(annotation);
        }
    }
}

/// UP037
pub fn quoted_annotations(checker: &mut Checker, stmt: &Stmt) {
    match &stmt.node {
        StmtKind::FunctionDef { args, returns, .. } => handle_functiondef(args, returns),
        StmtKind::AsyncFunctionDef { args, returns, .. } => handle_functiondef(args, returns),
        StmtKind::AnnAssign { annotation, .. } => replace_string_literal(annotation.clone()),
        _ => return,
    }

    /*
    if let Some(return_item) = &the_return {
        replace_string_literal(checker, return_item);
    }
    let arg_list = argument_list(args);
    for argument in arg_list {
        let annotate = match &argument.node.annotation {
            Some(item) => item,
            None => continue,
        };
        replace_string_literal(checker, annotate);
    }
    */
}
