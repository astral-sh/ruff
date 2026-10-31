use std::cmp::Ordering;
use std::slice;

use ruff_formatter::{FormatOwnedWithRule, FormatRule, FormatRuleWithOptions, write};
use ruff_python_ast::flat::visitor::source_order::{SourceOrderVisitor, walk_expr};
use ruff_python_ast::token::parentheses_iterator;
use ruff_python_ast::{
    Expr, Operator,
    flat::AnyNodeRef,
    flat::{self, fields as ast},
};
use ruff_python_trivia::TriviaRanges;
use ruff_text_size::Ranged;

use crate::builders::parenthesize_if_expands;
use crate::comments::{LeadingDanglingTrailingComments, leading_comments, trailing_comments};
use crate::context::{NodeLevel, WithNodeLevel};
use crate::expression::parentheses::{
    NeedsParentheses, OptionalParentheses, Parentheses, Parenthesize, optional_parentheses,
    parenthesized,
};
use crate::prelude::*;
use crate::preview::is_hug_parens_with_braces_and_square_brackets_enabled;

mod binary_like;
pub(crate) mod expr_attribute;
pub(crate) mod expr_await;
pub(crate) mod expr_bin_op;
pub(crate) mod expr_bool_op;
pub(crate) mod expr_boolean_literal;
pub(crate) mod expr_bytes_literal;
pub(crate) mod expr_call;
pub(crate) mod expr_compare;
pub(crate) mod expr_dict;
pub(crate) mod expr_dict_comp;
pub(crate) mod expr_ellipsis_literal;
pub(crate) mod expr_f_string;
pub(crate) mod expr_generator;
pub(crate) mod expr_if;
pub(crate) mod expr_ipy_escape_command;
pub(crate) mod expr_lambda;
pub(crate) mod expr_list;
pub(crate) mod expr_list_comp;
pub(crate) mod expr_name;
pub(crate) mod expr_named;
pub(crate) mod expr_none_literal;
pub(crate) mod expr_number_literal;
pub(crate) mod expr_set;
pub(crate) mod expr_set_comp;
pub(crate) mod expr_slice;
pub(crate) mod expr_starred;
pub(crate) mod expr_string_literal;
pub(crate) mod expr_subscript;
pub(crate) mod expr_t_string;
pub(crate) mod expr_tuple;
pub(crate) mod expr_unary_op;
pub(crate) mod expr_yield;
pub(crate) mod expr_yield_from;
mod operator;
pub(crate) mod parentheses;

#[derive(Copy, Clone, PartialEq, Eq, Default)]
pub struct FormatExpr {
    parentheses: Parentheses,
}

impl FormatRuleWithOptions<flat::View<'_, Expr>, PyFormatContext<'_>> for FormatExpr {
    type Options = Parentheses;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.parentheses = options;
        self
    }
}

impl FormatRule<flat::View<'_, Expr>, PyFormatContext<'_>> for FormatExpr {
    fn fmt(&self, expression: &flat::View<'_, Expr>, f: &mut PyFormatter) -> FormatResult<()> {
        let expression = *expression;
        let parentheses = self.parentheses;

        let format_expr = format_with(|f| match expression.classify() {
            flat::ExprKind::BoolOp(expr) => expr.format().fmt(f),
            flat::ExprKind::Named(expr) => expr.format().fmt(f),
            flat::ExprKind::BinOp(expr) => expr.format().fmt(f),
            flat::ExprKind::UnaryOp(expr) => expr.format().fmt(f),
            flat::ExprKind::Lambda(expr) => expr.format().fmt(f),
            flat::ExprKind::If(expr) => expr.format().fmt(f),
            flat::ExprKind::Dict(expr) => expr.format().fmt(f),
            flat::ExprKind::Set(expr) => expr.format().fmt(f),
            flat::ExprKind::ListComp(expr) => expr.format().fmt(f),
            flat::ExprKind::SetComp(expr) => expr.format().fmt(f),
            flat::ExprKind::DictComp(expr) => expr.format().fmt(f),
            flat::ExprKind::Generator(expr) => expr.format().fmt(f),
            flat::ExprKind::Await(expr) => expr.format().fmt(f),
            flat::ExprKind::Yield(expr) => expr.format().fmt(f),
            flat::ExprKind::YieldFrom(expr) => expr.format().fmt(f),
            flat::ExprKind::Compare(expr) => expr.format().fmt(f),
            flat::ExprKind::Call(expr) => expr.format().fmt(f),
            flat::ExprKind::FString(expr) => expr.format().fmt(f),
            flat::ExprKind::TString(expr) => expr.format().fmt(f),
            flat::ExprKind::StringLiteral(expr) => expr.format().fmt(f),
            flat::ExprKind::BytesLiteral(expr) => expr.format().fmt(f),
            flat::ExprKind::NumberLiteral(expr) => expr.format().fmt(f),
            flat::ExprKind::BooleanLiteral(expr) => expr.format().fmt(f),
            flat::ExprKind::NoneLiteral(expr) => expr.format().fmt(f),
            flat::ExprKind::EllipsisLiteral(expr) => expr.format().fmt(f),
            flat::ExprKind::Attribute(expr) => expr.format().fmt(f),
            flat::ExprKind::Subscript(expr) => expr.format().fmt(f),
            flat::ExprKind::Starred(expr) => expr.format().fmt(f),
            flat::ExprKind::Name(expr) => expr.format().fmt(f),
            flat::ExprKind::List(expr) => expr.format().fmt(f),
            flat::ExprKind::Tuple(expr) => expr.format().fmt(f),
            flat::ExprKind::Slice(expr) => expr.format().fmt(f),
            flat::ExprKind::IpyEscapeCommand(expr) => expr.format().fmt(f),
        });
        let parenthesize = match parentheses {
            Parentheses::Preserve => f.context().is_expression_parenthesized(expression),
            Parentheses::Always => true,
            // Fluent style means we already have parentheses
            Parentheses::Never => false,
        };
        if parenthesize {
            let comments = f.context().comments().clone();
            let node_comments = comments.leading_dangling_trailing(expression);
            if !node_comments.has_leading() && !node_comments.has_trailing() {
                parenthesized("(", &format_expr, ")")
                    .with_hugging(is_expression_huggable(expression, f.context()))
                    .fmt(f)
            } else {
                format_with_parentheses_comments(expression, &node_comments, f)
            }
        } else {
            let level = match f.context().node_level() {
                NodeLevel::TopLevel(_) | NodeLevel::CompoundStatement => {
                    NodeLevel::Expression(None)
                }
                saved_level @ (NodeLevel::Expression(_) | NodeLevel::ParenthesizedExpression) => {
                    saved_level
                }
            };

            let mut f = WithNodeLevel::new(level, f);

            write!(f, [format_expr])
        }
    }
}

/// The comments below are trailing on the addition, but it's also outside the
/// parentheses
/// ```python
/// x = [
///     # comment leading
///     (1 + 2)  # comment trailing
/// ]
/// ```
/// as opposed to
/// ```python
/// x = [(
///     # comment leading
///     1 + 2  # comment trailing
/// )]
/// ```
/// , where the comments are inside the parentheses. That is also affects list
/// formatting, where we want to avoid moving the comments after the comma inside
/// the parentheses:
/// ```python
/// data = [
///     (
///         b"\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00"
///         b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"
///     ),  # Point (0 0)
/// ]
/// ```
/// We could mark those comments as trailing in list but it's easier to handle
/// them here too.
///
/// So given
/// ```python
/// x = [
///     # comment leading outer
///     (
///         # comment leading inner
///         1 + 2 # comment trailing inner
///     ) # comment trailing outer
/// ]
/// ```
/// we want to keep the inner an outer comments outside the parentheses and the inner ones inside.
/// This is independent of whether they are own line or end-of-line comments, though end-of-line
/// comments can become own line comments when we discard nested parentheses.
///
/// Style decision: When there are multiple nested parentheses around an expression, we consider the
/// outermost parentheses the relevant ones and discard the others.
fn format_with_parentheses_comments(
    expression: flat::View<'_, Expr>,
    node_comments: &LeadingDanglingTrailingComments,
    f: &mut PyFormatter,
) -> FormatResult<()> {
    // First part: Split the comments

    // TODO(konstin): We don't have the parent, which is a problem:
    // ```python
    // f(
    //     # a
    //     (a)
    // )
    // ```
    // gets formatted as
    // ```python
    // f(
    //     (
    //         # a
    //         a
    //     )
    // )
    // ```
    let range_with_parens = parentheses_iterator(expression, None, f.context().tokens()).last();

    let (leading_split, trailing_split) = if let Some(range_with_parens) = range_with_parens {
        let leading_split = node_comments
            .leading
            .partition_point(|comment| comment.start() < range_with_parens.start());
        let trailing_split = node_comments
            .trailing
            .partition_point(|comment| comment.start() < range_with_parens.end());
        (leading_split, trailing_split)
    } else {
        (0, node_comments.trailing.len())
    };

    let (leading_outer, leading_inner) = node_comments.leading.split_at(leading_split);
    let (trailing_inner, trailing_outer) = node_comments.trailing.split_at(trailing_split);

    // Preserve an opening parentheses comment
    // ```python
    // a = ( # opening parentheses comment
    //     # leading inner
    //     1
    // )
    // ```
    let (parentheses_comment, leading_inner) = match leading_inner.split_first() {
        Some((first, rest)) if first.line_position().is_end_of_line() => {
            (slice::from_ref(first), rest)
        }
        _ => (Default::default(), node_comments.leading),
    };

    // Second Part: Format

    // The code order is a bit strange here, we format:
    // * outer leading comment
    // * opening parenthesis
    // * opening parenthesis comment
    // * inner leading comments
    // * the expression itself
    // * inner trailing comments
    // * the closing parenthesis
    // * outer trailing comments

    let fmt_fields = format_with(|f| match expression.classify() {
        flat::ExprKind::BoolOp(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::Named(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::BinOp(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::UnaryOp(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::Lambda(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::If(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::Dict(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::Set(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::ListComp(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::SetComp(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::DictComp(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::Generator(expr) => {
            FormatNodeRule::fmt_fields(expr.format().rule(), expr, f)
        }
        flat::ExprKind::Await(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::Yield(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::YieldFrom(expr) => {
            FormatNodeRule::fmt_fields(expr.format().rule(), expr, f)
        }
        flat::ExprKind::Compare(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::Call(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::FString(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::TString(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::StringLiteral(expr) => {
            FormatNodeRule::fmt_fields(expr.format().rule(), expr, f)
        }
        flat::ExprKind::BytesLiteral(expr) => {
            FormatNodeRule::fmt_fields(expr.format().rule(), expr, f)
        }
        flat::ExprKind::NumberLiteral(expr) => {
            FormatNodeRule::fmt_fields(expr.format().rule(), expr, f)
        }
        flat::ExprKind::BooleanLiteral(expr) => {
            FormatNodeRule::fmt_fields(expr.format().rule(), expr, f)
        }
        flat::ExprKind::NoneLiteral(expr) => {
            FormatNodeRule::fmt_fields(expr.format().rule(), expr, f)
        }
        flat::ExprKind::EllipsisLiteral(expr) => {
            FormatNodeRule::fmt_fields(expr.format().rule(), expr, f)
        }
        flat::ExprKind::Attribute(expr) => {
            FormatNodeRule::fmt_fields(expr.format().rule(), expr, f)
        }
        flat::ExprKind::Subscript(expr) => {
            FormatNodeRule::fmt_fields(expr.format().rule(), expr, f)
        }
        flat::ExprKind::Starred(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::Name(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::List(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::Tuple(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::Slice(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        flat::ExprKind::IpyEscapeCommand(expr) => {
            FormatNodeRule::fmt_fields(expr.format().rule(), expr, f)
        }
    });

    leading_comments(leading_outer).fmt(f)?;

    // Custom FormatNodeRule::fmt variant that only formats the inner comments
    let format_node_rule_fmt = format_with(|f| {
        // No need to handle suppression comments, those are statement only
        write!(
            f,
            [
                leading_comments(leading_inner),
                fmt_fields,
                trailing_comments(trailing_inner)
            ]
        )
    });

    // The actual parenthesized formatting
    write!(
        f,
        [
            parenthesized("(", &format_node_rule_fmt, ")")
                .with_dangling_comments(parentheses_comment),
            trailing_comments(trailing_outer)
        ]
    )
}

/// Wraps an expression in optional parentheses except if its [`NeedsParentheses::needs_parentheses`] implementation
/// indicates that it is okay to omit the parentheses. For example, parentheses can always be omitted for lists,
/// because they already bring their own parentheses.
pub(crate) fn maybe_parenthesize_expression<'a, T>(
    expression: flat::View<'a, Expr>,
    parent: T,
    parenthesize: Parenthesize,
) -> MaybeParenthesizeExpression<'a>
where
    T: Into<AnyNodeRef<'a>>,
{
    MaybeParenthesizeExpression {
        expression,
        parent: parent.into(),
        parenthesize,
    }
}

pub(crate) struct MaybeParenthesizeExpression<'a> {
    expression: flat::View<'a, Expr>,
    parent: AnyNodeRef<'a>,
    parenthesize: Parenthesize,
}

impl Format<PyFormatContext<'_>> for MaybeParenthesizeExpression<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        let MaybeParenthesizeExpression {
            expression,
            parent,
            parenthesize,
        } = self;

        let preserve_parentheses =
            parenthesize.is_optional() && f.context().is_expression_parenthesized(*expression);

        // If we want to preserve parentheses, short-circuit.
        if preserve_parentheses {
            return expression.format().with_options(Parentheses::Always).fmt(f);
        }

        let comments = f.context().comments().clone();
        let node_comments = comments.leading_dangling_trailing(*expression);

        // If the expression has comments, we always want to preserve the parentheses. This also
        // ensures that we correctly handle parenthesized comments, and don't need to worry about
        // them in the implementation below.
        if node_comments.has_leading() || node_comments.has_trailing_own_line() {
            return expression.format().with_options(Parentheses::Always).fmt(f);
        }

        let needs_parentheses = match expression.needs_parentheses(*parent, f.context()) {
            OptionalParentheses::Always => OptionalParentheses::Always,
            // The reason to add parentheses is to avoid a syntax error when breaking an expression over multiple lines.
            // Therefore, it is unnecessary to add an additional pair of parentheses if an outer expression
            // is parenthesized. Unless, it's the `Parenthesize::IfBreaksParenthesizedNested` layout
            // where parenthesizing nested `maybe_parenthesized_expression` is explicitly desired.
            _ if f.context().node_level().is_parenthesized() => {
                return if matches!(parenthesize, Parenthesize::IfBreaksParenthesizedNested) {
                    parenthesize_if_expands(&expression.format().with_options(Parentheses::Never))
                        .with_indent(!is_expression_huggable(*expression, f.context()))
                        .fmt(f)
                } else {
                    expression.format().with_options(Parentheses::Never).fmt(f)
                };
            }
            needs_parentheses => needs_parentheses,
        };

        let unparenthesized = expression.format().with_options(Parentheses::Never);

        match needs_parentheses {
            OptionalParentheses::Multiline => match parenthesize {
                Parenthesize::IfRequired => unparenthesized.fmt(f),

                Parenthesize::Optional
                | Parenthesize::IfBreaks
                | Parenthesize::IfBreaksParenthesized
                | Parenthesize::IfBreaksParenthesizedNested => {
                    if can_omit_optional_parentheses(*expression, f.context()) {
                        optional_parentheses(&unparenthesized).fmt(f)
                    } else {
                        parenthesize_if_expands(&unparenthesized).fmt(f)
                    }
                }
            },
            OptionalParentheses::BestFit => match parenthesize {
                Parenthesize::IfBreaksParenthesized | Parenthesize::IfBreaksParenthesizedNested => {
                    // Can-omit layout is relevant for `"abcd".call`. We don't want to add unnecessary
                    // parentheses in this case.
                    if can_omit_optional_parentheses(*expression, f.context()) {
                        optional_parentheses(&unparenthesized).fmt(f)
                    } else {
                        parenthesize_if_expands(&unparenthesized).fmt(f)
                    }
                }

                Parenthesize::Optional | Parenthesize::IfRequired => unparenthesized.fmt(f),

                Parenthesize::IfBreaks => {
                    if node_comments.has_trailing() {
                        expression.format().with_options(Parentheses::Always).fmt(f)
                    } else {
                        // The group id is necessary because the nested expressions may reference it.
                        let group_id = f.group_id("optional_parentheses");
                        let f = &mut WithNodeLevel::new(NodeLevel::Expression(Some(group_id)), f);

                        best_fit_parenthesize(&unparenthesized)
                            .with_group_id(Some(group_id))
                            .fmt(f)
                    }
                }
            },
            OptionalParentheses::Never => match parenthesize {
                Parenthesize::Optional
                | Parenthesize::IfBreaks
                | Parenthesize::IfRequired
                | Parenthesize::IfBreaksParenthesized
                | Parenthesize::IfBreaksParenthesizedNested => unparenthesized.fmt(f),
            },

            OptionalParentheses::Always => {
                expression.format().with_options(Parentheses::Always).fmt(f)
            }
        }
    }
}

impl NeedsParentheses for flat::View<'_, Expr> {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        match self.classify() {
            flat::ExprKind::BoolOp(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::Named(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::BinOp(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::UnaryOp(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::Lambda(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::If(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::Dict(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::Set(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::ListComp(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::SetComp(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::DictComp(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::Generator(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::Await(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::Yield(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::YieldFrom(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::Compare(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::Call(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::FString(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::TString(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::StringLiteral(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::BytesLiteral(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::NumberLiteral(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::BooleanLiteral(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::NoneLiteral(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::EllipsisLiteral(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::Attribute(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::Subscript(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::Starred(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::Name(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::List(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::Tuple(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::Slice(expr) => expr.needs_parentheses(parent, context),
            flat::ExprKind::IpyEscapeCommand(expr) => expr.needs_parentheses(parent, context),
        }
    }
}

impl<'ast> IntoFormat<PyFormatContext<'ast>> for flat::View<'_, Expr> {
    type Format = FormatOwnedWithRule<Self, FormatExpr, PyFormatContext<'ast>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatExpr::default())
    }
}

/// Tests if it is safe to omit the optional parentheses.
///
/// We prefer parentheses at least in the following cases:
/// * The expression contains more than one unparenthesized expression with the same precedence. For example,
///   the expression `a * b * c` contains two multiply operations. We prefer parentheses in that case.
///   `(a * b) * c` or `a * b + c` are okay, because the subexpression is parenthesized, or the expression uses operands with a lower precedence
/// * The expression contains at least one parenthesized sub expression (optimization to avoid unnecessary work)
///
/// This mimics Black's [`_maybe_split_omitting_optional_parens`](https://github.com/psf/black/blob/d1248ca9beaf0ba526d265f4108836d89cf551b7/src/black/linegen.py#L746-L820)
pub(crate) fn can_omit_optional_parentheses(
    expr: flat::View<'_, Expr>,
    context: &PyFormatContext,
) -> bool {
    let mut visitor = CanOmitOptionalParenthesesVisitor::new(context);
    visitor.visit_subexpression(expr);

    if !visitor.any_parenthesized_expressions {
        // Only use the more complex IR when there is any expression that we can possibly split by
        false
    } else if visitor.max_precedence_count > 1 {
        false
    } else if visitor.max_precedence == OperatorPrecedence::None {
        // Micha: This seems to apply for lambda expressions where the body ends in a subscript.
        // Subscripts are excluded by default because breaking them looks odd, but it seems to be fine for lambda expression.
        //
        // ```python
        // mapper = lambda x: dict_with_default[
        //  np.nan if isinstance(x, float) and np.isnan(x) else x
        // ]
        // ```
        //
        // to prevent that it gets formatted as:
        //
        // ```python
        // mapper = (
        //      lambda x: dict_with_default[
        //          np.nan if isinstance(x, float) and np.isnan(x) else x
        //      ]
        // )
        // ```
        // I think we should remove this check in the future and instead parenthesize the body of the lambda expression:
        //
        // ```python
        // mapper = lambda x: (
        //      dict_with_default[
        //          np.nan if isinstance(x, float) and np.isnan(x) else x
        //     ]
        // )
        // ```
        //
        // Another case are method chains:
        // ```python
        // xxxxxxxx.some_kind_of_method(
        //     some_argument=[
        //         "first",
        //         "second",
        //         "third",
        //     ]
        // ).another_method(a)
        // ```
        true
    } else if visitor.max_precedence == OperatorPrecedence::Attribute {
        // A single method call inside a named expression (`:=`) or as the body of a lambda function:
        // ```python
        // kwargs["open_with"] = lambda path, _: fsspec.open(
        //      path, "wb", **(storage_options or {})
        // ).open()
        //
        // if ret := subprocess.run(
        //      ["git", "rev-parse", "--short", "HEAD"],
        //      cwd=package_dir,
        //      capture_output=True,
        //      encoding="ascii",
        //      errors="surrogateescape",
        // ).stdout:
        // ```
        true
    } else {
        fn is_parenthesized(expr: flat::View<'_, Expr>, context: &PyFormatContext) -> bool {
            // Don't break subscripts except in parenthesized context. It looks weird.
            !expr.is_subscript_expr()
                && has_parentheses(expr, context).is_some_and(OwnParentheses::is_non_empty)
        }

        // Only use the layout if the first expression starts with parentheses
        // or the last expression ends with parentheses of some sort, and
        // those parentheses are non-empty.
        visitor
            .last
            .is_some_and(|last| is_parenthesized(last, context))
            || visitor
                .first
                .expression()
                .is_some_and(|first| is_parenthesized(first, context))
    }
}

#[derive(Clone, Debug)]
struct CanOmitOptionalParenthesesVisitor<'input> {
    max_precedence: OperatorPrecedence,
    max_precedence_count: u32,
    any_parenthesized_expressions: bool,
    last: Option<flat::View<'input, Expr>>,
    first: First<'input>,
    context: &'input PyFormatContext<'input>,
}

impl<'input> CanOmitOptionalParenthesesVisitor<'input> {
    fn new(context: &'input PyFormatContext) -> Self {
        Self {
            context,
            max_precedence: OperatorPrecedence::None,
            max_precedence_count: 0,
            any_parenthesized_expressions: false,
            last: None,
            first: First::None,
        }
    }

    fn update_max_precedence(&mut self, precedence: OperatorPrecedence) {
        self.update_max_precedence_with_count(precedence, 1);
    }

    fn update_max_precedence_with_count(&mut self, precedence: OperatorPrecedence, count: u32) {
        match self.max_precedence.cmp(&precedence) {
            Ordering::Less => {
                self.max_precedence_count = count;
                self.max_precedence = precedence;
            }
            Ordering::Equal => {
                self.max_precedence_count += count;
            }
            Ordering::Greater => {}
        }
    }

    // Visits a subexpression, ignoring whether it is parenthesized or not
    fn visit_subexpression(&mut self, expr: flat::View<'input, Expr>) {
        match expr.classify() {
            flat::ExprKind::Dict(_)
            | flat::ExprKind::List(_)
            | flat::ExprKind::Set(_)
            | flat::ExprKind::ListComp(_)
            | flat::ExprKind::SetComp(_)
            | flat::ExprKind::DictComp(_) => {
                self.any_parenthesized_expressions = true;
                // The values are always parenthesized, don't visit.
                return;
            }

            flat::ExprKind::Tuple(expr_tuple) if expr_tuple.parenthesized() => {
                self.any_parenthesized_expressions = true;
                // The values are always parenthesized, don't visit.
                return;
            }

            flat::ExprKind::Generator(generator) if generator.parenthesized() => {
                self.any_parenthesized_expressions = true;
                // The values are always parenthesized, don't visit.
                return;
            }

            // It's impossible for a file smaller or equal to 4GB to contain more than 2^32 comparisons
            // because each comparison requires a left operand, and `n` `operands` and right sides.
            #[expect(clippy::cast_possible_truncation)]
            flat::ExprKind::BoolOp(expr_bool_op) => {
                let values = expr_bool_op.values();
                self.update_max_precedence_with_count(
                    OperatorPrecedence::BooleanOperation,
                    values.len().saturating_sub(1) as u32,
                );
            }
            flat::ExprKind::BinOp(expr_bin_op) => {
                let op = expr_bin_op.op();
                self.update_max_precedence(OperatorPrecedence::from(op));
            }

            flat::ExprKind::If(_) => {
                // + 1 for the if and one for the else
                self.update_max_precedence_with_count(OperatorPrecedence::Conditional, 2);
            }

            // It's impossible for a file smaller or equal to 4GB to contain more than 2^32 comparisons
            // because each comparison requires a left operand, and `n` `operands` and right sides.
            #[expect(clippy::cast_possible_truncation)]
            flat::ExprKind::Compare(expr_compare) => {
                let ops = expr_compare.ops();

                self.update_max_precedence_with_count(
                    OperatorPrecedence::Comparator,
                    ops.len() as u32,
                );
            }
            flat::ExprKind::Call(expr_call) => {
                let func = expr_call.func();

                self.any_parenthesized_expressions = true;
                // Only walk the function, the arguments are always parenthesized
                self.visit_expr(func);
                self.last = Some(expr);
                return;
            }
            flat::ExprKind::Subscript(expr_subscript) => {
                let value = expr_subscript.value();

                self.any_parenthesized_expressions = true;
                // Only walk the function, the subscript is always parenthesized
                self.visit_expr(value);
                self.last = Some(expr);
                // Don't walk the slice, because the slice is always parenthesized.
                return;
            }

            // `[a, b].test.test[300].dot`
            flat::ExprKind::Attribute(expr_attribute) => {
                let value = expr_attribute.value();

                self.visit_expr(value);
                if has_parentheses(value, self.context).is_some() {
                    self.update_max_precedence(OperatorPrecedence::Attribute);
                }
                self.last = Some(expr);
                return;
            }

            // Non terminal nodes that don't have a termination token.
            flat::ExprKind::Named(_) | flat::ExprKind::Generator(_) | flat::ExprKind::Tuple(_) => {}

            // Expressions with sub expressions but a preceding token
            // Mark this expression as first expression and not the sub expression.
            // Visit the sub-expressions because the sub expressions may be the end of the entire expression.
            flat::ExprKind::UnaryOp(expr_unary_op) => {
                let op = expr_unary_op.op();

                if op.is_invert() {
                    self.update_max_precedence(OperatorPrecedence::BitwiseInversion);
                }
                self.first.set_if_none(First::Token);
            }

            flat::ExprKind::Lambda(_)
            | flat::ExprKind::Await(_)
            | flat::ExprKind::Yield(_)
            | flat::ExprKind::YieldFrom(_)
            | flat::ExprKind::Starred(_) => {
                self.first.set_if_none(First::Token);
            }

            // Terminal nodes or nodes that wrap a sub-expression (where the sub expression can never be at the end).
            flat::ExprKind::FString(_)
            | flat::ExprKind::TString(_)
            | flat::ExprKind::StringLiteral(_)
            | flat::ExprKind::BytesLiteral(_)
            | flat::ExprKind::NumberLiteral(_)
            | flat::ExprKind::BooleanLiteral(_)
            | flat::ExprKind::NoneLiteral(_)
            | flat::ExprKind::EllipsisLiteral(_)
            | flat::ExprKind::Name(_)
            | flat::ExprKind::Slice(_)
            | flat::ExprKind::IpyEscapeCommand(_) => {
                return;
            }
        }

        walk_expr(self, expr);
    }
}

impl<'input> SourceOrderVisitor<'input> for CanOmitOptionalParenthesesVisitor<'input> {
    fn visit_expr(&mut self, expr: flat::View<'input, Expr>) {
        self.last = Some(expr);

        // Rule only applies for non-parenthesized expressions.
        if self.context.is_expression_parenthesized(expr) {
            self.any_parenthesized_expressions = true;
        } else {
            self.visit_subexpression(expr);
        }

        self.first.set_if_none(First::Expression(expr));
    }
}

#[derive(Copy, Clone, Debug)]
enum First<'a> {
    None,

    /// Expression starts with a non-parentheses token. E.g. `not a`
    Token,

    Expression(flat::View<'a, Expr>),
}

impl<'a> First<'a> {
    #[inline]
    fn set_if_none(&mut self, first: First<'a>) {
        if matches!(self, First::None) {
            *self = first;
        }
    }

    fn expression(self) -> Option<flat::View<'a, Expr>> {
        match self {
            First::None | First::Token => None,
            First::Expression(expr) => Some(expr),
        }
    }
}

/// A call chain consists only of attribute access (`.` operator), function/method calls and
/// subscripts. We use fluent style for the call chain if there are at least two attribute dots
/// after call parentheses or subscript brackets. In case of fluent style the parentheses/bracket
/// will close on the previous line and the dot gets its own line, otherwise the line will start
/// with the closing parentheses/bracket and the dot follows immediately after.
///
/// Below, the left hand side of the addition has only a single attribute access after a call, the
/// second `.filter`. The first `.filter` is a call, but it doesn't follow a call. The right hand
/// side has two, the `.limit_results` after the call and the `.filter` after the subscript, so it
/// gets formatted in fluent style. The outer expression we assign to `blogs` has zero since the
/// `.all` follows attribute parentheses and not call parentheses.
///
/// ```python
/// blogs = (
///     Blog.objects.filter(
///         entry__headline__contains="Lennon",
///     ).filter(
///         entry__pub_date__year=2008,
///     )
///     + Blog.objects.filter(
///         entry__headline__contains="McCartney",
///     )
///     .limit_results[:10]
///     .filter(
///         entry__pub_date__year=2010,
///     )
/// ).all()
/// ```
///
/// In [`preview`](crate::preview::is_fluent_layout_split_first_call_enabled), we also track the position of the leftmost call or
/// subscript on an attribute in the chain and break just before the dot.
///
/// So, for example, the right-hand summand in the above expression
/// would get formatted as:
/// ```python
///     Blog.objects
///     .filter(
///         entry__headline__contains="McCartney",
///     )
///     .limit_results[:10]
///     .filter(
///         entry__pub_date__year=2010,
///     )
/// ```
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum CallChainLayout {
    /// The root of a call chain
    #[default]
    Default,

    /// A nested call chain element that uses fluent style.
    Fluent(AttributeState),

    /// A nested call chain element not using fluent style.
    NonFluent,
}

/// Records information about the current position within
/// a call chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeState {
    /// Stores the number of calls or subscripts
    /// to the left of the current position in a chain.
    ///
    /// Consecutive calls/subscripts on a single
    /// object only count once. For example, if we are at
    /// `c` in `a.b()[0]()().c()` then this number would be 1.
    ///
    /// Caveat: If the root of the chain is parenthesized,
    /// it contributes +1 to this count, even if it is not
    /// a call or subscript. But the name
    /// `CallLikeOrParenthesizedRootPreceding`
    /// is a tad unwieldy, and this also rarely occurs.
    CallLikePreceding(u32),
    /// Indicates that we are at the first called or
    /// subscripted object in the chain
    ///
    /// For example, if we are at `b` in `a.b()[0]()().c()`
    FirstCallLike,
    /// Indicates that we are to the left of the first
    /// called or subscripted object in the chain, and therefore
    /// need not break.
    ///
    /// For example, if we are at `a` in `a.b()[0]()().c()`
    BeforeFirstCallLike,
}

impl CallChainLayout {
    /// Returns new state decreasing count of remaining calls/subscripts
    /// to traverse, or the state `FirstCallOrSubscript`, as appropriate.
    #[must_use]
    fn decrement_call_like_count(self) -> Self {
        match self {
            Self::Fluent(AttributeState::CallLikePreceding(x)) => {
                if x > 1 {
                    // Recall that we traverse call chains from right to
                    // left. So after moving from a call/subscript into
                    // an attribute, we _decrease_ the count of
                    // _remaining_ calls or subscripts to the left of our
                    // current position.
                    Self::Fluent(AttributeState::CallLikePreceding(x - 1))
                } else {
                    Self::Fluent(AttributeState::FirstCallLike)
                }
            }
            _ => self,
        }
    }

    /// Returns with state change
    /// `FirstCallOrSubscript` -> `BeforeFirstCallOrSubscript`
    /// and otherwise returns unchanged.
    #[must_use]
    fn transition_after_attribute(self) -> Self {
        match self {
            Self::Fluent(AttributeState::FirstCallLike) => {
                Self::Fluent(AttributeState::BeforeFirstCallLike)
            }
            _ => self,
        }
    }

    fn is_first_call_like(self) -> bool {
        matches!(self, Self::Fluent(AttributeState::FirstCallLike))
    }

    /// Returns either `Fluent` or `NonFluent` depending on a
    /// heuristic computed for the whole chain.
    ///
    /// Explicitly, the criterion to return `Fluent` is
    /// as follows:
    ///
    /// 1. Beginning from the right (i.e. the `expr` itself),
    ///    traverse inwards past calls, subscripts, and attribute
    ///    expressions until we meet the first expression that is
    ///    either none of these or else is parenthesized. This will
    ///    be the _root_ of the call chain.
    /// 2. Count the number of _attribute values_ that are _called
    ///    or subscripted_ in the chain (note that this includes the
    ///    root but excludes the rightmost attribute in the chain since
    ///    it is not the _value_ of some attribute).
    /// 3. If the root is parenthesized, add 1 to that value.
    /// 4. If the total is at least 2, return `Fluent`. Otherwise
    ///    return `NonFluent`
    fn from_expression(mut expr: flat::View<'_, Expr>, context: &PyFormatContext) -> Self {
        // TODO(dylan): Once the fluent layout preview style is
        // stabilized, see if it is possible to simplify some of
        // the logic around parenthesized roots. (While supporting
        // both styles it is more difficult to do this.)

        // Count of attribute _values_ which are called or
        // subscripted, after the leftmost parenthesized
        // value.
        //
        // Examples:
        // ```
        // # Count of 3 - notice that .d()
        // # does not contribute
        // a().b().c[0]()().d()
        // # Count of 2 - notice that a()
        // # does not contribute
        // (a()).b().c[0].d
        // ```
        let mut computed_attribute_values_after_parentheses = 0;

        // Similar to the above, but instead looks at all calls
        // and subscripts rather than looking only at those on
        // _attribute values_. So this count can differ from the
        // above.
        //
        // Examples of `computed_attribute_values_after_parentheses` vs
        // `call_like_count`:
        //
        // a().b --->  1 vs 1
        // a.b().c --> 1 vs 1
        // a.b() --->  0 vs 1
        let mut call_like_count = 0;

        // Going from right to left, we traverse calls, subscripts,
        // and attributes until we get to an expression of a different
        // kind _or_ to a parenthesized expression. This records
        // the case where we end the traversal at a parenthesized expression.
        //
        // In these cases, the inferred semantics of the chain are different.
        // We interpret this as the user indicating:
        // "this parenthesized value is the object of interest and we are
        // doing transformations on it". This increases our confidence that
        // this should be fluently formatted, and also means we should make
        // our first break after this value.
        let mut root_value_parenthesized = false;
        loop {
            match expr.fields() {
                ast::Expr::Attribute(ast::ExprAttribute { value, .. }) => {
                    // ```
                    // f().g
                    // ^^^ value
                    // data[:100].T
                    // ^^^^^^^^^^ value
                    // ```
                    if context.is_expression_parenthesized(value) {
                        // `(a).b`. We preserve these parentheses so don't recurse
                        root_value_parenthesized = true;
                        break;
                    } else if matches!(
                        value.classify(),
                        flat::ExprKind::Call(_) | flat::ExprKind::Subscript(_)
                    ) {
                        computed_attribute_values_after_parentheses += 1;
                    }

                    expr = value;
                }
                // ```
                // f()
                // ^^^ expr
                // ^ func
                // data[:100]
                // ^^^^^^^^^^ expr
                // ^^^^ value
                // ```
                ast::Expr::Call(ast::ExprCall { func: inner, .. })
                | ast::Expr::Subscript(ast::ExprSubscript { value: inner, .. }) => {
                    // We preserve these parentheses so don't recurse
                    // e.g. (a)[0].x().y().z()
                    //         ^stop here
                    if context.is_expression_parenthesized(inner) {
                        break;
                    }

                    // Accumulate the `call_like_count`, but we only
                    // want to count things like `a()[0]()()` once.
                    if !inner.is_call_expr() && !inner.is_subscript_expr() {
                        call_like_count += 1;
                    }

                    expr = inner;
                }
                _ => {
                    break;
                }
            }
        }

        if computed_attribute_values_after_parentheses + u32::from(root_value_parenthesized) < 2 {
            CallChainLayout::NonFluent
        } else {
            CallChainLayout::Fluent(AttributeState::CallLikePreceding(
                // We count a parenthesized root value as an extra
                // call for the purposes of tracking state.
                //
                // The reason is that, in this case, we want the first
                // "special" break to happen right after the root, as
                // opposed to right after the first called/subscripted
                // attribute.
                //
                // For example:
                //
                // ```
                // (object_of_interest)
                // .data.filter()
                // .agg()
                // .etc()
                // ```
                //
                // instead of (in preview):
                //
                // ```
                // (object_of_interest)
                // .data
                // .filter()
                // .etc()
                // ```
                //
                // For comparison, if we didn't have parentheses around
                // the root, we want (and get, in preview):
                //
                // ```
                // object_of_interest.data
                // .filter()
                // .agg()
                // .etc()
                // ```
                call_like_count + u32::from(root_value_parenthesized),
            ))
        }
    }

    /// Determine whether to actually apply fluent layout in attribute, call and subscript
    /// formatting
    fn apply_in_node<'a>(
        self,
        item: impl Into<flat::View<'a, Expr>>,
        f: &mut PyFormatter,
    ) -> CallChainLayout {
        match self {
            CallChainLayout::Default => {
                if f.context().node_level().is_parenthesized() {
                    CallChainLayout::from_expression(item.into(), f.context())
                } else {
                    CallChainLayout::NonFluent
                }
            }
            layout @ (CallChainLayout::Fluent(_) | CallChainLayout::NonFluent) => layout,
        }
    }

    fn is_fluent(self) -> bool {
        matches!(self, CallChainLayout::Fluent(_))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum OwnParentheses {
    /// The node has parentheses, but they are empty (e.g., `[]` or `f()`).
    Empty,
    /// The node has parentheses, and they are non-empty (e.g., `[1]` or `f(1)`).
    NonEmpty,
}

impl OwnParentheses {
    const fn is_non_empty(self) -> bool {
        matches!(self, OwnParentheses::NonEmpty)
    }
}

/// Returns the [`OwnParentheses`] value for a given [`Expr`], to indicate whether it has its
/// own parentheses or is itself parenthesized.
///
/// Differs from [`has_own_parentheses`] in that it returns [`OwnParentheses::NonEmpty`] for
/// parenthesized expressions, like `(1)` or `([1])`, regardless of whether those expression have
/// their _own_ parentheses.
pub(crate) fn has_parentheses(
    expr: flat::View<'_, Expr>,
    context: &PyFormatContext,
) -> Option<OwnParentheses> {
    let own_parentheses = has_own_parentheses(expr, context);

    // If the node has its own non-empty parentheses, we don't need to check for surrounding
    // parentheses (e.g., `[1]`, or `([1])`).
    if own_parentheses == Some(OwnParentheses::NonEmpty) {
        return own_parentheses;
    }

    // Otherwise, if the node lacks parentheses (e.g., `(1)`) or only contains empty parentheses
    // (e.g., `([])`), we need to check for surrounding parentheses.
    if context.is_expression_parenthesized(expr) {
        return Some(OwnParentheses::NonEmpty);
    }

    own_parentheses
}

/// Returns the [`OwnParentheses`] value for a given [`Expr`], to indicate whether it has its
/// own parentheses, and whether those parentheses are empty.
///
/// A node is considered to have its own parentheses if it includes a `[]`, `()`, or `{}` pair
/// that is inherent to the node (e.g., as in `f()`, `[]`, or `{1: 2}`, but not `(a.b.c)`).
///
/// Parentheses are considered to be non-empty if they contain any elements or comments.
pub(crate) fn has_own_parentheses(
    expr: flat::View<'_, Expr>,
    context: &PyFormatContext,
) -> Option<OwnParentheses> {
    match expr.fields() {
        // These expressions are always non-empty.
        ast::Expr::ListComp(_)
        | ast::Expr::SetComp(_)
        | ast::Expr::DictComp(_)
        | ast::Expr::Subscript(_) => Some(OwnParentheses::NonEmpty),

        ast::Expr::Generator(ast::ExprGenerator {
            node: generator, ..
        }) if generator.parenthesized() => Some(OwnParentheses::NonEmpty),

        // These expressions must contain _some_ child or trivia token in order to be non-empty.
        ast::Expr::List(ast::ExprList { elts, .. }) | ast::Expr::Set(ast::ExprSet { elts, .. }) => {
            if !elts.is_empty() || context.comments().has_dangling(AnyNodeRef::from(expr)) {
                Some(OwnParentheses::NonEmpty)
            } else {
                Some(OwnParentheses::Empty)
            }
        }

        ast::Expr::Tuple(ast::ExprTuple {
            node: tuple,
            parenthesized: true,
            ..
        }) => {
            if !tuple.elts().is_empty() || context.comments().has_dangling(AnyNodeRef::from(expr)) {
                Some(OwnParentheses::NonEmpty)
            } else {
                Some(OwnParentheses::Empty)
            }
        }

        ast::Expr::Dict(ast::ExprDict { items, .. }) => {
            if !items.is_empty() || context.comments().has_dangling(AnyNodeRef::from(expr)) {
                Some(OwnParentheses::NonEmpty)
            } else {
                Some(OwnParentheses::Empty)
            }
        }
        ast::Expr::Call(ast::ExprCall { arguments, .. }) => {
            if !arguments.is_empty() || context.comments().has_dangling(AnyNodeRef::from(expr)) {
                Some(OwnParentheses::NonEmpty)
            } else {
                Some(OwnParentheses::Empty)
            }
        }

        _ => None,
    }
}

/// Returns `true` if the expression can hug directly to enclosing parentheses, as in Black's
/// `hug_parens_with_braces_and_square_brackets` or `multiline_string_handling` preview styles behavior.
///
/// For example, in preview style, given:
/// ```python
/// ([1, 2, 3,])
/// ```
///
/// We want to format it as:
/// ```python
/// ([
///     1,
///     2,
///     3,
/// ])
/// ```
///
/// As opposed to:
/// ```python
/// (
///     [
///         1,
///         2,
///         3,
///     ]
/// )
/// ```
pub(crate) fn is_expression_huggable(
    expr: flat::View<'_, Expr>,
    context: &PyFormatContext,
) -> bool {
    match expr.classify() {
        flat::ExprKind::Tuple(_)
        | flat::ExprKind::List(_)
        | flat::ExprKind::Set(_)
        | flat::ExprKind::Dict(_)
        | flat::ExprKind::ListComp(_)
        | flat::ExprKind::SetComp(_)
        | flat::ExprKind::DictComp(_) => {
            is_hug_parens_with_braces_and_square_brackets_enabled(context)
        }

        flat::ExprKind::Starred(expr_starred) => {
            let value = expr_starred.value();
            is_expression_huggable(value, context)
        }

        flat::ExprKind::BoolOp(_)
        | flat::ExprKind::Named(_)
        | flat::ExprKind::BinOp(_)
        | flat::ExprKind::UnaryOp(_)
        | flat::ExprKind::Lambda(_)
        | flat::ExprKind::If(_)
        | flat::ExprKind::Generator(_)
        | flat::ExprKind::Await(_)
        | flat::ExprKind::Yield(_)
        | flat::ExprKind::YieldFrom(_)
        | flat::ExprKind::Compare(_)
        | flat::ExprKind::Call(_)
        | flat::ExprKind::Attribute(_)
        | flat::ExprKind::Subscript(_)
        | flat::ExprKind::Name(_)
        | flat::ExprKind::Slice(_)
        | flat::ExprKind::IpyEscapeCommand(_)
        | flat::ExprKind::NumberLiteral(_)
        | flat::ExprKind::BooleanLiteral(_)
        | flat::ExprKind::NoneLiteral(_)
        | flat::ExprKind::StringLiteral(_)
        | flat::ExprKind::BytesLiteral(_)
        | flat::ExprKind::FString(_)
        | flat::ExprKind::TString(_)
        | flat::ExprKind::EllipsisLiteral(_) => false,
    }
}

/// The precedence of [python operators](https://docs.python.org/3/reference/expressions.html#operator-precedence) from
/// highest to lowest priority.
///
/// Ruff uses the operator precedence to decide in which order to split operators:
/// Operators with a lower precedence split before higher-precedence operators.
/// Splitting by precedence ensures that the visual grouping reflects the precedence.
#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
enum OperatorPrecedence {
    None,
    Attribute,
    Exponential,
    BitwiseInversion,
    Multiplicative,
    Additive,
    Shift,
    BitwiseAnd,
    BitwiseXor,
    BitwiseOr,
    Comparator,
    BooleanOperation,
    Conditional,
}

impl From<Operator> for OperatorPrecedence {
    fn from(value: Operator) -> Self {
        match value {
            Operator::Add | Operator::Sub => OperatorPrecedence::Additive,
            Operator::Mult
            | Operator::MatMult
            | Operator::Div
            | Operator::Mod
            | Operator::FloorDiv => OperatorPrecedence::Multiplicative,
            Operator::Pow => OperatorPrecedence::Exponential,
            Operator::LShift | Operator::RShift => OperatorPrecedence::Shift,
            Operator::BitOr => OperatorPrecedence::BitwiseOr,
            Operator::BitXor => OperatorPrecedence::BitwiseXor,
            Operator::BitAnd => OperatorPrecedence::BitwiseAnd,
        }
    }
}

/// Returns `true` if `expr` is an expression that can be split into multiple lines.
///
/// Returns `false` for expressions that are guaranteed to never split.
pub(crate) fn is_splittable_expression(
    expr: flat::View<'_, Expr>,
    context: &PyFormatContext,
) -> bool {
    match expr.fields() {
        // Single token expressions. They never have any split points.
        ast::Expr::Named(_)
        | ast::Expr::Name(_)
        | ast::Expr::NumberLiteral(_)
        | ast::Expr::BooleanLiteral(_)
        | ast::Expr::NoneLiteral(_)
        | ast::Expr::EllipsisLiteral(_)
        | ast::Expr::Slice(_)
        | ast::Expr::IpyEscapeCommand(_) => false,

        // Expressions that insert split points when parenthesized.
        ast::Expr::Compare(_)
        | ast::Expr::BinOp(_)
        | ast::Expr::BoolOp(_)
        | ast::Expr::If(_)
        | ast::Expr::Generator(_)
        | ast::Expr::Subscript(_)
        | ast::Expr::Await(_)
        | ast::Expr::ListComp(_)
        | ast::Expr::SetComp(_)
        | ast::Expr::DictComp(_)
        | ast::Expr::YieldFrom(_) => true,

        // Sequence types can split if they contain at least one element.
        ast::Expr::Tuple(ast::ExprTuple { node: tuple, .. }) => !tuple.elts().is_empty(),
        ast::Expr::Dict(ast::ExprDict { node: dict, .. }) => !dict.items().is_empty(),
        ast::Expr::Set(ast::ExprSet { node: set, .. }) => !set.elts().is_empty(),
        ast::Expr::List(ast::ExprList { node: list, .. }) => !list.elts().is_empty(),

        ast::Expr::UnaryOp(ast::ExprUnaryOp { node: unary, .. }) => {
            is_splittable_expression(unary.operand(), context)
        }
        ast::Expr::Yield(ast::ExprYield { value, .. }) => value.is_some(),

        ast::Expr::Call(ast::ExprCall {
            arguments, func, ..
        }) => !arguments.is_empty() || context.is_expression_parenthesized(func),

        // String like literals can expand if they are implicit concatenated.
        ast::Expr::FString(ast::ExprFString { node: fstring, .. }) => {
            fstring.value().is_implicit_concatenated()
        }
        ast::Expr::TString(ast::ExprTString { node: tstring, .. }) => {
            tstring.value().is_implicit_concatenated()
        }
        ast::Expr::StringLiteral(ast::ExprStringLiteral { node: string, .. }) => {
            string.value().is_implicit_concatenated()
        }
        ast::Expr::BytesLiteral(ast::ExprBytesLiteral { node: bytes, .. }) => {
            bytes.value().is_implicit_concatenated()
        }

        // Expressions that have no split points per se, but they contain nested sub expressions that might expand.
        ast::Expr::Lambda(ast::ExprLambda {
            body: expression, ..
        })
        | ast::Expr::Starred(ast::ExprStarred {
            value: expression, ..
        })
        | ast::Expr::Attribute(ast::ExprAttribute {
            value: expression, ..
        }) => {
            context.is_expression_parenthesized(expression)
                || is_splittable_expression(expression, context)
        }
    }
}

/// Returns `true` if `expr` is invalid in a type-expression position unless the source keeps it
/// parenthesized.
pub(crate) fn is_invalid_type_expression(expr: flat::View<'_, Expr>) -> bool {
    matches!(
        expr.classify(),
        flat::ExprKind::Named(_)
            | flat::ExprKind::Await(_)
            | flat::ExprKind::Yield(_)
            | flat::ExprKind::YieldFrom(_)
    )
}

/// Returns the sub-expression to which the left-most character in expression belongs.
///
/// For example, in the expression `a + b * c`, the left-most subexpression is `a`. But for
/// the expression `{ "a": 1 }`, the left-most subexpression is the dictionary, and not `"a"` because
/// the `{` belongs to the dictionary.
///
/// Parenthesized expressions are treated as belonging to the enclosing expression. Therefore, the left
/// most expression for `(a + b) * c` is `a + b` and not `a`.
pub(crate) fn left_most<'expr>(
    expression: flat::View<'expr, Expr>,
    trivia: &TriviaRanges,
) -> flat::View<'expr, Expr> {
    let mut current = expression;
    loop {
        let left = match current.fields() {
            ast::Expr::BinOp(ast::ExprBinOp { left, .. })
            | ast::Expr::If(ast::ExprIf { body: left, .. })
            | ast::Expr::Call(ast::ExprCall { func: left, .. })
            | ast::Expr::Attribute(ast::ExprAttribute { value: left, .. })
            | ast::Expr::Subscript(ast::ExprSubscript { value: left, .. }) => Some(left),

            ast::Expr::BoolOp(ast::ExprBoolOp {
                node: expr_bool_op, ..
            }) => expr_bool_op.values().first(),
            ast::Expr::Compare(ast::ExprCompare { node: compare, .. }) => {
                Some(compare.first_operand())
            }

            ast::Expr::Generator(ast::ExprGenerator {
                node: generator, ..
            }) if !generator.parenthesized() => Some(generator.elt()),

            ast::Expr::Tuple(ast::ExprTuple { node: tuple, .. }) if !tuple.parenthesized() => {
                tuple.elts().first()
            }
            ast::Expr::Slice(ast::ExprSlice { node: slice, .. }) => slice.lower(),

            ast::Expr::List(_)
            | ast::Expr::Tuple(_)
            | ast::Expr::Name(_)
            | ast::Expr::Starred(_)
            | ast::Expr::FString(_)
            | ast::Expr::TString(_)
            | ast::Expr::StringLiteral(_)
            | ast::Expr::BytesLiteral(_)
            | ast::Expr::NumberLiteral(_)
            | ast::Expr::BooleanLiteral(_)
            | ast::Expr::NoneLiteral(_)
            | ast::Expr::EllipsisLiteral(_)
            | ast::Expr::Yield(_)
            | ast::Expr::YieldFrom(_)
            | ast::Expr::Await(_)
            | ast::Expr::DictComp(_)
            | ast::Expr::SetComp(_)
            | ast::Expr::ListComp(_)
            | ast::Expr::Set(_)
            | ast::Expr::Dict(_)
            | ast::Expr::UnaryOp(_)
            | ast::Expr::Lambda(_)
            | ast::Expr::Named(_)
            | ast::Expr::IpyEscapeCommand(_)
            | ast::Expr::Generator(_) => None,
        };

        let Some(left) = left else {
            break current;
        };

        if trivia.parenthesized().contains(left.range()) {
            break current;
        }

        current = left;
    }
}
