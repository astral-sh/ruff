use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::violation::AlwaysAutofixableViolation;
use ruff_macros::derive_message_formats;
use rustpython_ast::{Arg, ArgData, Arguments, Constant, ExprKind, Located};

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

/// UP038
pub fn quoted_annotations(
    checker: &mut Checker,
    args: &Box<Arguments>,
    type_comment: &Option<String>,
) {
    println!("{:?}", type_comment);
    let arg_list = argument_list(args);
    for argument in arg_list {
        if let ArgData {
            arg, annotation, ..
        } = argument.node
        {
            let annotate = match annotation {
                Some(item) => item,
                None => continue,
            };
            if let ExprKind::Constant { value, .. } = &annotate.node {
                if let Constant::Str(type_str) = value {
                    let mut diagnostic =
                        Diagnostic::new(QuotedAnnotations, Range::from_located(&annotate));
                    if checker.patch(&Rule::PrintfStringFormatting) {
                        diagnostic.amend(Fix::replacement(
                            type_str.to_string(),
                            annotate.location,
                            annotate.end_location.unwrap(),
                        ));
                    }
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
    }
}
