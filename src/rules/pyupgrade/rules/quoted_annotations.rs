use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::cst::matchers::{match_annassign, match_functiondef, match_module};
use crate::define_violation;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::violation::AlwaysAutofixableViolation;
use libcst_native::{AnnAssign, Annotation, Call, Expression, FunctionDef, Param, Parameters};
use ruff_macros::derive_message_formats;
use rustpython_ast::{Constant, Expr, ExprKind, Stmt};

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

fn get_params<'a>(params: &Parameters<'a>) -> Vec<Param<'a>> {
    let mut final_result: Vec<Param<'a>> = vec![];
    final_result.extend(params.params.clone());
    final_result.extend(params.kwonly_params.clone());
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

fn process_call<'a>(node: &mut Box<Call<'a>>) -> Vec<&'a mut Annotation<'a>> {
    let to_add: Vec<&mut Annotation> = vec![];
    println!("{:?}", node);
    to_add
}

fn replace_string_literal(annotation: &mut Annotation) {
    let mut nodes: Vec<&mut Annotation> = vec![annotation];
    while nodes.len() > 0 {
        let node = match nodes.pop() {
            Some(item) => item,
            None => continue,
        };
        match &mut node.annotation {
            Expression::Call(item) => nodes.extend(process_call(item)),
            _ => continue,
        }
    }
}

fn handle_functiondef(funcdef: &mut FunctionDef) {
    let params = get_params(&funcdef.params);
    for param in params {
        if let Some(mut annotation) = param.annotation {
            replace_string_literal(&mut annotation);
        }
    }
}

fn handle_annassign(assign: &mut AnnAssign) {
    replace_string_literal(&mut assign.annotation);
}

/// UP037
pub fn quoted_annotations(checker: &mut Checker, stmt: &Stmt) {
    let module_text = checker
        .locator
        .slice_source_code_range(&Range::from_located(stmt));
    let mut tree = match_module(module_text).unwrap();
    // let mut funcdef = match_functiondef(&mut tree).unwrap();
    let mut annassign = match_annassign(&mut tree).unwrap();
    handle_annassign(&mut annassign);
    // handle_functiondef(&mut funcdef);
    // let mut import = match_import_from(&mut tree);
    /*
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
    */
}
