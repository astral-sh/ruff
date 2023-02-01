use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::violation::AlwaysAutofixableViolation;
use ruff_macros::derive_message_formats;
use rustpython_ast::{Arg, Arguments, Constant, Expr, ExprKind};

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

fn argument_list(args: &Box<Arguments>) -> Vec<Arg> {
    let mut final_result: Vec<Arg> = vec![];
    final_result.extend(args.posonlyargs.clone());
    final_result.extend(args.args.clone());
    final_result.extend(args.kwonlyargs.clone());
    final_result
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

fn process_call(node: &Expr) -> Vec<&Expr> {
    let to_add: Vec<&Expr> = vec![];
    if let ExprKind::Call {
        func,
        args,
        keywords,
    } = &node.node
    {
        println!("{:?}", func);
        return to_add;
    }
    to_add
}

fn replace_string_literal(checker: &mut Checker, annotation: &Box<Expr>) {
    let mut nodes: Vec<&Expr> = vec![annotation.as_ref()];
    while nodes.len() > 0 {
        let node = match nodes.pop() {
            Some(item) => item,
            None => continue,
        };
        match node.node {
            ExprKind::Call { .. } => nodes.extend(process_call(node)),
            _ => continue,
        }
    }
}

/// UP037
pub fn quoted_annotations_funcdef(
    checker: &mut Checker,
    args: &Box<Arguments>,
    the_return: &Option<Box<Expr>>,
) {
    println!("{:?}", the_return);
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
}

/// UP037
pub fn quoted_annotations_annassign(checker: &mut Checker, annotation: &Box<Expr>) {
    println!("STARTING");
    replace_string_literal(checker, annotation);
}
