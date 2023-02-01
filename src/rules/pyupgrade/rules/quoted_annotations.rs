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

/// UP037
pub fn quoted_annotations_funcdef(
    checker: &mut Checker,
    args: &Box<Arguments>,
    the_return: &Option<Box<Expr>>,
) {
    println!("{:?}", the_return);
    if let Some(return_item) = &the_return {
        remove_quotes(checker, return_item);
    }
    let arg_list = argument_list(args);
    for argument in arg_list {
        let annotate = match &argument.node.annotation {
            Some(item) => item,
            None => continue,
        };
        remove_quotes(checker, annotate);
    }
}

/// UP037
pub fn quoted_annotations_annassign(checker: &mut Checker, annotation: &Box<Expr>) {
    remove_quotes(checker, annotation);
}
