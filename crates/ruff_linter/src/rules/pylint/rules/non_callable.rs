use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, ExprCall, UnaryOp};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{FixAvailability, Violation};

#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct NonCallable {}

impl Violation for NonCallable {
    // No reliable fix can be available, since we can't know the author's intent.
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        "This type of object is not callable.".to_string()
    }
}

/// PLE1102
pub(crate) fn non_callable(checker: &Checker, call: &ExprCall) {
    let (func, arguments) = (&*call.func, &call.arguments);

    // Check if expression is callable.
    match func {
        // Hard no, all except TString are detected by `type_inference`.
        Expr::StringLiteral(string_literal) => {
            checker.report_diagnostic(NonCallable {}, string_literal.range());
        }
        Expr::BooleanLiteral(expr_boolean_literal) => todo!(),
        Expr::BytesLiteral(expr_bytes_literal) => todo!(),
        Expr::Dict(expr_dict) => todo!(),
        Expr::DictComp(expr_dict_comp) => todo!(),
        Expr::EllipsisLiteral(expr_ellipsis_literal) => todo!(),
        Expr::FString(expr_fstring) => todo!(),
        Expr::Generator(expr_generator) => todo!(),
        Expr::List(expr_list) => todo!(),
        Expr::ListComp(expr_list_comp) => todo!(),
        Expr::NoneLiteral(expr_none_literal) => todo!(),
        Expr::NumberLiteral(expr_number_literal) => todo!(),
        Expr::Set(expr_set) => todo!(),
        Expr::SetComp(expr_set_comp) => todo!(),
        Expr::TString(expr_tstring) => todo!(),
        Expr::Tuple(expr_tuple) => todo!(),

        // Yes, actual callable.
        // `type_inference` identifies it as Unknown, so we'll just let it pass.
        Expr::Lambda(expr_lambda) => todo!(),
        // Unknowns - just ignore.
        Expr::Name(expr_name) => todo!(),
        Expr::Yield(expr_yield) => todo!(),
        Expr::Call(expr_call) => todo!(),
        Expr::Attribute(expr_attribute) => todo!(),
        Expr::Subscript(expr_subscript) => todo!(),
        Expr::Await(expr_await) => todo!(),
        Expr::YieldFrom(expr_yield_from) => todo!(),
        // Overridable using dunders, treat as unknowns.
        Expr::BinOp(expr_bin_op) => todo!(),
        Expr::Compare(expr_compare) => todo!(),

        // If and BoolOp basically result in union of types
        // Using `type_inference` they will either collapse to a single simple type
        // or union of simple types to flag or to Unknown to skip.
        Expr::If(expr_if) => todo!(),
        Expr::BoolOp(expr_bool_op) => todo!(),
        // `type_inference` will unwrap it and check.
        Expr::Named(expr_named) => todo!(),
        // Could be no (`not x` -> `bool`) or Unknown (depends on dunder implementations).
        // Will be handled by `type_inference`.
        Expr::UnaryOp(expr_unary_op) => todo!(),

        // Impossible
        // All catched as SyntaxErrors.
        Expr::Slice(expr_slice) => todo!(),
        Expr::Starred(expr_starred) => todo!(),
        Expr::IpyEscapeCommand(expr_ipy_escape_command) => todo!(),
    }
}
