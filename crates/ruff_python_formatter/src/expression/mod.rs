use std::cmp::Ordering;
use std::slice;

use ruff_formatter::{
    write, FormatOwnedWithRule, FormatRefWithRule, FormatRule, FormatRuleWithOptions,
};
use ruff_python_ast as ast;
use ruff_python_ast::parenthesize::parentheses_iterator;
use ruff_python_ast::visitor::source_order::{walk_expr, SourceOrderVisitor};
use ruff_python_ast::{AnyNodeRef, Expr, ExpressionRef, Operator};
use ruff_python_trivia::CommentRanges;
use ruff_text_size::Ranged;

use crate::builders::parenthesize_if_expands;
use crate::comments::{leading_comments, trailing_comments, LeadingDanglingTrailingComments};
use crate::context::{NodeLevel, WithNodeLevel};
use crate::expression::parentheses::{
    is_expression_parenthesized, optional_parentheses, parenthesized, NeedsParentheses,
    OptionalParentheses, Parentheses, Parenthesize,
};
use crate::prelude::*;
use crate::preview::{
    is_empty_parameters_no_unnecessary_parentheses_around_return_value_enabled,
    is_hug_parens_with_braces_and_square_brackets_enabled,
};

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

impl FormatRuleWithOptions<Expr, PyFormatContext<'_>> for FormatExpr {
    type Options = Parentheses;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.parentheses = options;
        self
    }
}

impl FormatRule<Expr, PyFormatContext<'_>> for FormatExpr {
    fn fmt(&self, expression: &Expr, f: &mut PyFormatter) -> FormatResult<()> {
        let parentheses = self.parentheses;

        let format_expr = format_with(|f| match expression {
            Expr::BoolOp(expr) => expr.format().fmt(f),
            Expr::Named(expr) => expr.format().fmt(f),
            Expr::BinOp(expr) => expr.format().fmt(f),
            Expr::UnaryOp(expr) => expr.format().fmt(f),
            Expr::Lambda(expr) => expr.format().fmt(f),
            Expr::If(expr) => expr.format().fmt(f),
            Expr::Dict(expr) => expr.format().fmt(f),
            Expr::Set(expr) => expr.format().fmt(f),
            Expr::ListComp(expr) => expr.format().fmt(f),
            Expr::SetComp(expr) => expr.format().fmt(f),
            Expr::DictComp(expr) => expr.format().fmt(f),
            Expr::Generator(expr) => expr.format().fmt(f),
            Expr::Await(expr) => expr.format().fmt(f),
            Expr::Yield(expr) => expr.format().fmt(f),
            Expr::YieldFrom(expr) => expr.format().fmt(f),
            Expr::Compare(expr) => expr.format().fmt(f),
            Expr::Call(expr) => expr.format().fmt(f),
            Expr::FString(expr) => expr.format().fmt(f),
            Expr::StringLiteral(expr) => expr.format().fmt(f),
            Expr::BytesLiteral(expr) => expr.format().fmt(f),
            Expr::NumberLiteral(expr) => expr.format().fmt(f),
            Expr::BooleanLiteral(expr) => expr.format().fmt(f),
            Expr::NoneLiteral(expr) => expr.format().fmt(f),
            Expr::EllipsisLiteral(expr) => expr.format().fmt(f),
            Expr::Attribute(expr) => expr.format().fmt(f),
            Expr::Subscript(expr) => expr.format().fmt(f),
            Expr::Starred(expr) => expr.format().fmt(f),
            Expr::Name(expr) => expr.format().fmt(f),
            Expr::List(expr) => expr.format().fmt(f),
            Expr::Tuple(expr) => expr.format().fmt(f),
            Expr::Slice(expr) => expr.format().fmt(f),
            Expr::IpyEscapeCommand(expr) => expr.format().fmt(f),
        });
        let parenthesize = match parentheses {
            Parentheses::Preserve => is_expression_parenthesized(
                expression.into(),
                f.context().comments().ranges(),
                f.context().source(),
            ),
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
    expression: &Expr,
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
    let range_with_parens = parentheses_iterator(
        expression.into(),
        None,
        f.context().comments().ranges(),
        f.context().source(),
    )
    .last();

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

    let fmt_fields = format_with(|f| match expression {
        Expr::BoolOp(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::Named(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::BinOp(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::UnaryOp(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::Lambda(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::If(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::Dict(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::Set(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::ListComp(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::SetComp(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::DictComp(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::Generator(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::Await(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::Yield(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::YieldFrom(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::Compare(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::Call(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::FString(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::StringLiteral(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::BytesLiteral(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::NumberLiteral(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::BooleanLiteral(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::NoneLiteral(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::EllipsisLiteral(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::Attribute(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::Subscript(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::Starred(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::Name(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::List(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::Tuple(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::Slice(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
        Expr::IpyEscapeCommand(expr) => FormatNodeRule::fmt_fields(expr.format().rule(), expr, f),
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
    expression: &'a Expr,
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
    expression: &'a Expr,
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

        let preserve_parentheses = parenthesize.is_optional()
            && is_expression_parenthesized(
                (*expression).into(),
                f.context().comments().ranges(),
                f.context().source(),
            );

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
                if !is_empty_parameters_no_unnecessary_parentheses_around_return_value_enabled(
                    f.context(),
                ) {
                    OptionalParentheses::Never
                } else if matches!(parenthesize, Parenthesize::IfBreaksParenthesizedNested) {
                    return parenthesize_if_expands(
                        &expression.format().with_options(Parentheses::Never),
                    )
                    .with_indent(!is_expression_huggable(expression, f.context()))
                    .fmt(f);
                } else {
                    return expression.format().with_options(Parentheses::Never).fmt(f);
                }
            }
            needs_parentheses => needs_parentheses,
        };

        match needs_parentheses {
            OptionalParentheses::Multiline => match parenthesize {

                Parenthesize::IfBreaksParenthesized | Parenthesize::IfBreaksParenthesizedNested if !is_empty_parameters_no_unnecessary_parentheses_around_return_value_enabled(f.context()) => {
                    parenthesize_if_expands(&expression.format().with_options(Parentheses::Never))
                        .fmt(f)
                }
                Parenthesize::IfRequired => {
                    expression.format().with_options(Parentheses::Never).fmt(f)
                }

                Parenthesize::Optional | Parenthesize::IfBreaks | Parenthesize::IfBreaksParenthesized | Parenthesize::IfBreaksParenthesizedNested => {
                    if can_omit_optional_parentheses(expression, f.context()) {
                        optional_parentheses(&expression.format().with_options(Parentheses::Never))
                            .fmt(f)
                    } else {
                        parenthesize_if_expands(
                            &expression.format().with_options(Parentheses::Never),
                        )
                        .fmt(f)
                    }
                }
            },
            OptionalParentheses::BestFit => match parenthesize {
                Parenthesize::IfBreaksParenthesized | Parenthesize::IfBreaksParenthesizedNested => {
                    parenthesize_if_expands(&expression.format().with_options(Parentheses::Never))
                        .fmt(f)
                }

                Parenthesize::Optional | Parenthesize::IfRequired => {
                    expression.format().with_options(Parentheses::Never).fmt(f)
                }

                Parenthesize::IfBreaks => {
                    if node_comments.has_trailing() {
                        expression.format().with_options(Parentheses::Always).fmt(f)
                    } else {
                        // The group id is necessary because the nested expressions may reference it.
                        let group_id = f.group_id("optional_parentheses");
                        let f = &mut WithNodeLevel::new(NodeLevel::Expression(Some(group_id)), f);

                        best_fit_parenthesize(&expression.format().with_options(Parentheses::Never))
                            .with_group_id(Some(group_id))
                            .fmt(f)
                    }
                }
            },
            OptionalParentheses::Never => match parenthesize {
                Parenthesize::IfBreaksParenthesized |  Parenthesize::IfBreaksParenthesizedNested if !is_empty_parameters_no_unnecessary_parentheses_around_return_value_enabled(f.context()) => {
                    parenthesize_if_expands(&expression.format().with_options(Parentheses::Never))
                        .with_indent(!is_expression_huggable(expression, f.context()))
                        .fmt(f)
                }

                Parenthesize::Optional | Parenthesize::IfBreaks | Parenthesize::IfRequired | Parenthesize::IfBreaksParenthesized |  Parenthesize::IfBreaksParenthesizedNested => {
                    expression.format().with_options(Parentheses::Never).fmt(f)
                }
            },

            OptionalParentheses::Always => {
                expression.format().with_options(Parentheses::Always).fmt(f)
            }
        }
    }
}

impl NeedsParentheses for Expr {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        match self {
            Expr::BoolOp(expr) => expr.needs_parentheses(parent, context),
            Expr::Named(expr) => expr.needs_parentheses(parent, context),
            Expr::BinOp(expr) => expr.needs_parentheses(parent, context),
            Expr::UnaryOp(expr) => expr.needs_parentheses(parent, context),
            Expr::Lambda(expr) => expr.needs_parentheses(parent, context),
            Expr::If(expr) => expr.needs_parentheses(parent, context),
            Expr::Dict(expr) => expr.needs_parentheses(parent, context),
            Expr::Set(expr) => expr.needs_parentheses(parent, context),
            Expr::ListComp(expr) => expr.needs_parentheses(parent, context),
            Expr::SetComp(expr) => expr.needs_parentheses(parent, context),
            Expr::DictComp(expr) => expr.needs_parentheses(parent, context),
            Expr::Generator(expr) => expr.needs_parentheses(parent, context),
            Expr::Await(expr) => expr.needs_parentheses(parent, context),
            Expr::Yield(expr) => expr.needs_parentheses(parent, context),
            Expr::YieldFrom(expr) => expr.needs_parentheses(parent, context),
            Expr::Compare(expr) => expr.needs_parentheses(parent, context),
            Expr::Call(expr) => expr.needs_parentheses(parent, context),
            Expr::FString(expr) => expr.needs_parentheses(parent, context),
            Expr::StringLiteral(expr) => expr.needs_parentheses(parent, context),
            Expr::BytesLiteral(expr) => expr.needs_parentheses(parent, context),
            Expr::NumberLiteral(expr) => expr.needs_parentheses(parent, context),
            Expr::BooleanLiteral(expr) => expr.needs_parentheses(parent, context),
            Expr::NoneLiteral(expr) => expr.needs_parentheses(parent, context),
            Expr::EllipsisLiteral(expr) => expr.needs_parentheses(parent, context),
            Expr::Attribute(expr) => expr.needs_parentheses(parent, context),
            Expr::Subscript(expr) => expr.needs_parentheses(parent, context),
            Expr::Starred(expr) => expr.needs_parentheses(parent, context),
            Expr::Name(expr) => expr.needs_parentheses(parent, context),
            Expr::List(expr) => expr.needs_parentheses(parent, context),
            Expr::Tuple(expr) => expr.needs_parentheses(parent, context),
            Expr::Slice(expr) => expr.needs_parentheses(parent, context),
            Expr::IpyEscapeCommand(expr) => expr.needs_parentheses(parent, context),
        }
    }
}

impl<'ast> AsFormat<PyFormatContext<'ast>> for Expr {
    type Format<'a> = FormatRefWithRule<'a, Expr, FormatExpr, PyFormatContext<'ast>>;

    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, FormatExpr::default())
    }
}

impl<'ast> IntoFormat<PyFormatContext<'ast>> for Expr {
    type Format = FormatOwnedWithRule<Expr, FormatExpr, PyFormatContext<'ast>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatExpr::default())
    }
}

/// Tests if it is safe to omit the optional parentheses.
///
/// We prefer parentheses at least in the following cases:
/// * The expression contains more than one unparenthesized expression with the same precedence. For example,
///     the expression `a * b * c` contains two multiply operations. We prefer parentheses in that case.
///     `(a * b) * c` or `a * b + c` are okay, because the subexpression is parenthesized, or the expression uses operands with a lower precedence
/// * The expression contains at least one parenthesized sub expression (optimization to avoid unnecessary work)
///
/// This mimics Black's [`_maybe_split_omitting_optional_parens`](https://github.com/psf/black/blob/d1248ca9beaf0ba526d265f4108836d89cf551b7/src/black/linegen.py#L746-L820)
#[allow(clippy::if_same_then_else)]
pub(crate) fn can_omit_optional_parentheses(expr: &Expr, context: &PyFormatContext) -> bool {
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
        fn is_parenthesized(expr: &Expr, context: &PyFormatContext) -> bool {
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
    last: Option<&'input Expr>,
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
    fn visit_subexpression(&mut self, expr: &'input Expr) {
        match expr {
            Expr::Dict(_)
            | Expr::List(_)
            | Expr::Set(_)
            | Expr::ListComp(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_) => {
                self.any_parenthesized_expressions = true;
                // The values are always parenthesized, don't visit.
                return;
            }

            Expr::Tuple(ast::ExprTuple {
                parenthesized: true,
                ..
            }) => {
                self.any_parenthesized_expressions = true;
                // The values are always parenthesized, don't visit.
                return;
            }

            Expr::Generator(generator) if generator.parenthesized => {
                self.any_parenthesized_expressions = true;
                // The values are always parenthesized, don't visit.
                return;
            }

            // It's impossible for a file smaller or equal to 4GB to contain more than 2^32 comparisons
            // because each comparison requires a left operand, and `n` `operands` and right sides.
            #[allow(clippy::cast_possible_truncation)]
            Expr::BoolOp(ast::ExprBoolOp {
                range: _,
                op: _,
                values,
            }) => self.update_max_precedence_with_count(
                OperatorPrecedence::BooleanOperation,
                values.len().saturating_sub(1) as u32,
            ),
            Expr::BinOp(ast::ExprBinOp {
                op,
                left: _,
                right: _,
                range: _,
            }) => self.update_max_precedence(OperatorPrecedence::from(*op)),

            Expr::If(_) => {
                // + 1 for the if and one for the else
                self.update_max_precedence_with_count(OperatorPrecedence::Conditional, 2);
            }

            // It's impossible for a file smaller or equal to 4GB to contain more than 2^32 comparisons
            // because each comparison requires a left operand, and `n` `operands` and right sides.
            #[allow(clippy::cast_possible_truncation)]
            Expr::Compare(ast::ExprCompare {
                range: _,
                left: _,
                ops,
                comparators: _,
            }) => {
                self.update_max_precedence_with_count(
                    OperatorPrecedence::Comparator,
                    ops.len() as u32,
                );
            }
            Expr::Call(ast::ExprCall {
                range: _,
                func,
                arguments: _,
            }) => {
                self.any_parenthesized_expressions = true;
                // Only walk the function, the arguments are always parenthesized
                self.visit_expr(func);
                self.last = Some(expr);
                return;
            }
            Expr::Subscript(ast::ExprSubscript { value, .. }) => {
                self.any_parenthesized_expressions = true;
                // Only walk the function, the subscript is always parenthesized
                self.visit_expr(value);
                self.last = Some(expr);
                // Don't walk the slice, because the slice is always parenthesized.
                return;
            }

            // `[a, b].test.test[300].dot`
            Expr::Attribute(ast::ExprAttribute {
                range: _,
                value,
                attr: _,
                ctx: _,
            }) => {
                self.visit_expr(value);
                if has_parentheses(value, self.context).is_some() {
                    self.update_max_precedence(OperatorPrecedence::Attribute);
                }
                self.last = Some(expr);
                return;
            }

            Expr::StringLiteral(ast::ExprStringLiteral { value, .. })
                if value.is_implicit_concatenated() =>
            {
                self.update_max_precedence(OperatorPrecedence::String);
            }
            Expr::BytesLiteral(ast::ExprBytesLiteral { value, .. })
                if value.is_implicit_concatenated() =>
            {
                self.update_max_precedence(OperatorPrecedence::String);
            }
            Expr::FString(ast::ExprFString { value, .. }) if value.is_implicit_concatenated() => {
                self.update_max_precedence(OperatorPrecedence::String);
                return;
            }

            // Non terminal nodes that don't have a termination token.
            Expr::Named(_) | Expr::Generator(_) | Expr::Tuple(_) => {}

            // Expressions with sub expressions but a preceding token
            // Mark this expression as first expression and not the sub expression.
            // Visit the sub-expressions because the sub expressions may be the end of the entire expression.
            Expr::UnaryOp(ast::ExprUnaryOp {
                range: _,
                op,
                operand: _,
            }) => {
                if op.is_invert() {
                    self.update_max_precedence(OperatorPrecedence::BitwiseInversion);
                }
                self.first.set_if_none(First::Token);
            }

            Expr::Lambda(_)
            | Expr::Await(_)
            | Expr::Yield(_)
            | Expr::YieldFrom(_)
            | Expr::Starred(_) => {
                self.first.set_if_none(First::Token);
            }

            // Terminal nodes or nodes that wrap a sub-expression (where the sub expression can never be at the end).
            Expr::FString(_)
            | Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
            | Expr::NumberLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::NoneLiteral(_)
            | Expr::EllipsisLiteral(_)
            | Expr::Name(_)
            | Expr::Slice(_)
            | Expr::IpyEscapeCommand(_) => {
                return;
            }
        };

        walk_expr(self, expr);
    }
}

impl<'input> SourceOrderVisitor<'input> for CanOmitOptionalParenthesesVisitor<'input> {
    fn visit_expr(&mut self, expr: &'input Expr) {
        self.last = Some(expr);

        // Rule only applies for non-parenthesized expressions.
        if is_expression_parenthesized(
            expr.into(),
            self.context.comments().ranges(),
            self.context.source(),
        ) {
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

    Expression(&'a Expr),
}

impl<'a> First<'a> {
    #[inline]
    fn set_if_none(&mut self, first: First<'a>) {
        if matches!(self, First::None) {
            *self = first;
        }
    }

    fn expression(self) -> Option<&'a Expr> {
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
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum CallChainLayout {
    /// The root of a call chain
    #[default]
    Default,

    /// A nested call chain element that uses fluent style.
    Fluent,

    /// A nested call chain element not using fluent style.
    NonFluent,
}

impl CallChainLayout {
    pub(crate) fn from_expression(
        mut expr: ExpressionRef,
        comment_ranges: &CommentRanges,
        source: &str,
    ) -> Self {
        let mut attributes_after_parentheses = 0;
        loop {
            match expr {
                ExpressionRef::Attribute(ast::ExprAttribute { value, .. }) => {
                    // ```
                    // f().g
                    // ^^^ value
                    // data[:100].T
                    // ^^^^^^^^^^ value
                    // ```
                    if is_expression_parenthesized(value.into(), comment_ranges, source) {
                        // `(a).b`. We preserve these parentheses so don't recurse
                        attributes_after_parentheses += 1;
                        break;
                    } else if matches!(value.as_ref(), Expr::Call(_) | Expr::Subscript(_)) {
                        attributes_after_parentheses += 1;
                    }

                    expr = ExpressionRef::from(value.as_ref());
                }
                // ```
                // f()
                // ^^^ expr
                // ^ func
                // data[:100]
                // ^^^^^^^^^^ expr
                // ^^^^ value
                // ```
                ExpressionRef::Call(ast::ExprCall { func: inner, .. })
                | ExpressionRef::Subscript(ast::ExprSubscript { value: inner, .. }) => {
                    expr = ExpressionRef::from(inner.as_ref());
                }
                _ => {
                    // We to format the following in fluent style:
                    // ```
                    // f2 = (a).w().t(1,)
                    //       ^ expr
                    // ```
                    if is_expression_parenthesized(expr, comment_ranges, source) {
                        attributes_after_parentheses += 1;
                    }

                    break;
                }
            }

            // We preserve these parentheses so don't recurse
            if is_expression_parenthesized(expr, comment_ranges, source) {
                break;
            }
        }
        if attributes_after_parentheses < 2 {
            CallChainLayout::NonFluent
        } else {
            CallChainLayout::Fluent
        }
    }

    /// Determine whether to actually apply fluent layout in attribute, call and subscript
    /// formatting
    pub(crate) fn apply_in_node<'a>(
        self,
        item: impl Into<ExpressionRef<'a>>,
        f: &mut PyFormatter,
    ) -> CallChainLayout {
        match self {
            CallChainLayout::Default => {
                if f.context().node_level().is_parenthesized() {
                    CallChainLayout::from_expression(
                        item.into(),
                        f.context().comments().ranges(),
                        f.context().source(),
                    )
                } else {
                    CallChainLayout::NonFluent
                }
            }
            layout @ (CallChainLayout::Fluent | CallChainLayout::NonFluent) => layout,
        }
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
pub(crate) fn has_parentheses(expr: &Expr, context: &PyFormatContext) -> Option<OwnParentheses> {
    let own_parentheses = has_own_parentheses(expr, context);

    // If the node has its own non-empty parentheses, we don't need to check for surrounding
    // parentheses (e.g., `[1]`, or `([1])`).
    if own_parentheses == Some(OwnParentheses::NonEmpty) {
        return own_parentheses;
    }

    // Otherwise, if the node lacks parentheses (e.g., `(1)`) or only contains empty parentheses
    // (e.g., `([])`), we need to check for surrounding parentheses.
    if is_expression_parenthesized(expr.into(), context.comments().ranges(), context.source()) {
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
    expr: &Expr,
    context: &PyFormatContext,
) -> Option<OwnParentheses> {
    match expr {
        // These expressions are always non-empty.
        Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) | Expr::Subscript(_) => {
            Some(OwnParentheses::NonEmpty)
        }

        Expr::Generator(generator) if generator.parenthesized => Some(OwnParentheses::NonEmpty),

        // These expressions must contain _some_ child or trivia token in order to be non-empty.
        Expr::List(ast::ExprList { elts, .. }) | Expr::Set(ast::ExprSet { elts, .. }) => {
            if !elts.is_empty() || context.comments().has_dangling(AnyNodeRef::from(expr)) {
                Some(OwnParentheses::NonEmpty)
            } else {
                Some(OwnParentheses::Empty)
            }
        }

        Expr::Tuple(
            tuple @ ast::ExprTuple {
                parenthesized: true,
                ..
            },
        ) => {
            if !tuple.is_empty() || context.comments().has_dangling(AnyNodeRef::from(expr)) {
                Some(OwnParentheses::NonEmpty)
            } else {
                Some(OwnParentheses::Empty)
            }
        }

        Expr::Dict(ast::ExprDict { items, .. }) => {
            if !items.is_empty() || context.comments().has_dangling(AnyNodeRef::from(expr)) {
                Some(OwnParentheses::NonEmpty)
            } else {
                Some(OwnParentheses::Empty)
            }
        }
        Expr::Call(ast::ExprCall { arguments, .. }) => {
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
pub(crate) fn is_expression_huggable(expr: &Expr, context: &PyFormatContext) -> bool {
    match expr {
        Expr::Tuple(_)
        | Expr::List(_)
        | Expr::Set(_)
        | Expr::Dict(_)
        | Expr::ListComp(_)
        | Expr::SetComp(_)
        | Expr::DictComp(_) => is_hug_parens_with_braces_and_square_brackets_enabled(context),

        Expr::Starred(ast::ExprStarred { value, .. }) => is_expression_huggable(value, context),

        Expr::BoolOp(_)
        | Expr::Named(_)
        | Expr::BinOp(_)
        | Expr::UnaryOp(_)
        | Expr::Lambda(_)
        | Expr::If(_)
        | Expr::Generator(_)
        | Expr::Await(_)
        | Expr::Yield(_)
        | Expr::YieldFrom(_)
        | Expr::Compare(_)
        | Expr::Call(_)
        | Expr::Attribute(_)
        | Expr::Subscript(_)
        | Expr::Name(_)
        | Expr::Slice(_)
        | Expr::IpyEscapeCommand(_)
        | Expr::NumberLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::NoneLiteral(_)
        | Expr::StringLiteral(_)
        | Expr::BytesLiteral(_)
        | Expr::FString(_)
        | Expr::EllipsisLiteral(_) => false,
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
    // Implicit string concatenation
    String,
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
pub(crate) fn is_splittable_expression(expr: &Expr, context: &PyFormatContext) -> bool {
    match expr {
        // Single token expressions. They never have any split points.
        Expr::Named(_)
        | Expr::Name(_)
        | Expr::NumberLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::NoneLiteral(_)
        | Expr::EllipsisLiteral(_)
        | Expr::Slice(_)
        | Expr::IpyEscapeCommand(_) => false,

        // Expressions that insert split points when parenthesized.
        Expr::Compare(_)
        | Expr::BinOp(_)
        | Expr::BoolOp(_)
        | Expr::If(_)
        | Expr::Generator(_)
        | Expr::Subscript(_)
        | Expr::Await(_)
        | Expr::ListComp(_)
        | Expr::SetComp(_)
        | Expr::DictComp(_)
        | Expr::YieldFrom(_) => true,

        // Sequence types can split if they contain at least one element.
        Expr::Tuple(tuple) => !tuple.is_empty(),
        Expr::Dict(dict) => !dict.is_empty(),
        Expr::Set(set) => !set.is_empty(),
        Expr::List(list) => !list.is_empty(),

        Expr::UnaryOp(unary) => is_splittable_expression(unary.operand.as_ref(), context),
        Expr::Yield(ast::ExprYield { value, .. }) => value.is_some(),

        Expr::Call(ast::ExprCall {
            arguments, func, ..
        }) => {
            !arguments.is_empty()
                || is_expression_parenthesized(
                    func.as_ref().into(),
                    context.comments().ranges(),
                    context.source(),
                )
        }

        // String like literals can expand if they are implicit concatenated.
        Expr::FString(fstring) => fstring.value.is_implicit_concatenated(),
        Expr::StringLiteral(string) => string.value.is_implicit_concatenated(),
        Expr::BytesLiteral(bytes) => bytes.value.is_implicit_concatenated(),

        // Expressions that have no split points per se, but they contain nested sub expressions that might expand.
        Expr::Lambda(ast::ExprLambda {
            body: expression, ..
        })
        | Expr::Starred(ast::ExprStarred {
            value: expression, ..
        })
        | Expr::Attribute(ast::ExprAttribute {
            value: expression, ..
        }) => {
            is_expression_parenthesized(
                expression.into(),
                context.comments().ranges(),
                context.source(),
            ) || is_splittable_expression(expression.as_ref(), context)
        }
    }
}
