use super::rules;
use crate::checkers::ast::Checker;
use crate::registry::Rule;
use ruff_python_ast::flat;
use ruff_python_ast::{Expr, ExprCall};

/// Select rules that rewrite a one-argument call to a collection constructor.
pub(crate) fn single_argument_collection_call(checker: &Checker, call: flat::View<'_, ExprCall>) {
    let arguments = call.arguments();
    let Some([argument]) = arguments.args().as_array() else {
        return;
    };
    if !arguments.keywords().is_empty() {
        return;
    }
    let Some(function) = call.func().as_name_expr() else {
        return;
    };
    match (function.id().as_str(), argument.classify()) {
        ("list", flat::ExprKind::Generator(_))
            if checker.is_rule_enabled(Rule::UnnecessaryGeneratorList) =>
        {
            rules::unnecessary_generator_list(checker, call, argument);
        }
        ("set", flat::ExprKind::Generator(_))
            if checker.is_rule_enabled(Rule::UnnecessaryGeneratorSet) =>
        {
            rules::unnecessary_generator_set(checker, call, argument);
        }
        ("dict", flat::ExprKind::Generator(_))
            if checker.is_rule_enabled(Rule::UnnecessaryGeneratorDict) =>
        {
            rules::unnecessary_generator_dict(checker, call.into(), argument);
        }
        ("set", flat::ExprKind::ListComp(_))
            if checker.is_rule_enabled(Rule::UnnecessaryListComprehensionSet) =>
        {
            rules::unnecessary_list_comprehension_set(checker, call, argument);
        }
        ("dict", flat::ExprKind::ListComp(_))
            if checker.is_rule_enabled(Rule::UnnecessaryListComprehensionDict) =>
        {
            rules::unnecessary_list_comprehension_dict(checker, call.into(), argument);
        }
        ("set", flat::ExprKind::List(_) | flat::ExprKind::Tuple(_))
            if checker.is_rule_enabled(Rule::UnnecessaryLiteralSet) =>
        {
            rules::unnecessary_literal_set(checker, call, argument);
        }
        ("dict", flat::ExprKind::List(_) | flat::ExprKind::Tuple(_))
            if checker.is_rule_enabled(Rule::UnnecessaryLiteralDict) =>
        {
            rules::unnecessary_literal_dict(checker, call.into(), argument);
        }
        ("tuple", flat::ExprKind::List(_) | flat::ExprKind::Tuple(_))
            if checker.is_rule_enabled(Rule::UnnecessaryLiteralWithinTupleCall) =>
        {
            rules::unnecessary_literal_within_tuple_call(checker, call, argument);
        }
        _ => {}
    }
}

pub(super) fn first_argument_with_matching_function<'a>(
    name: &str,
    func: flat::View<'_, Expr>,
    args: flat::NodeList<'a, Expr>,
) -> Option<flat::View<'a, Expr>> {
    if func.as_name_expr().is_some_and(|func| func.id() == name) {
        args.first()
    } else {
        None
    }
}
