//! The LALRPOP based parser implementation.

use itertools::Itertools;
use lalrpop_util::ParseError as LalrpopError;

use ruff_python_ast::{
    Expr, ExprAttribute, ExprAwait, ExprBinOp, ExprBoolOp, ExprBooleanLiteral, ExprBytesLiteral,
    ExprCall, ExprCompare, ExprDict, ExprDictComp, ExprEllipsisLiteral, ExprFString,
    ExprGeneratorExp, ExprIfExp, ExprIpyEscapeCommand, ExprLambda, ExprList, ExprListComp,
    ExprName, ExprNamedExpr, ExprNoneLiteral, ExprNumberLiteral, ExprSet, ExprSetComp, ExprSlice,
    ExprStarred, ExprStringLiteral, ExprSubscript, ExprTuple, ExprUnaryOp, ExprYield,
    ExprYieldFrom, Mod,
};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::lexer::{LexResult, LexicalError, LexicalErrorType};
use crate::{Mode, ParseError, ParseErrorType, Tok};

mod context;
mod function;

#[rustfmt::skip]
#[allow(unreachable_pub)]
#[allow(clippy::type_complexity)]
#[allow(clippy::extra_unused_lifetimes)]
#[allow(clippy::needless_lifetimes)]
#[allow(clippy::unused_self)]
#[allow(clippy::cast_sign_loss)]
#[allow(clippy::default_trait_access)]
#[allow(clippy::let_unit_value)]
#[allow(clippy::just_underscores_and_digits)]
#[allow(clippy::no_effect_underscore_binding)]
#[allow(clippy::trivially_copy_pass_by_ref)]
#[allow(clippy::option_option)]
#[allow(clippy::unnecessary_wraps)]
#[allow(clippy::uninlined_format_args)]
#[allow(clippy::cloned_instead_of_copied)]
mod python {

    #[cfg(feature = "lalrpop")]
    include!(concat!(env!("OUT_DIR"), "/src/lalrpop/python.rs"));

    #[cfg(not(feature = "lalrpop"))]
    include!("python.rs");
}

pub(crate) fn parse_tokens(
    tokens: Vec<LexResult>,
    source: &str,
    mode: Mode,
) -> Result<Mod, ParseError> {
    let marker_token = (Tok::start_marker(mode), TextRange::default());
    let lexer = std::iter::once(Ok(marker_token)).chain(
        tokens
            .into_iter()
            .filter_ok(|token| !matches!(token, (Tok::Comment(..) | Tok::NonLogicalNewline, _))),
    );
    python::TopParser::new()
        .parse(
            source,
            mode,
            lexer.map_ok(|(t, range)| (range.start(), t, range.end())),
        )
        .map_err(parse_error_from_lalrpop)
}

fn parse_error_from_lalrpop(err: LalrpopError<TextSize, Tok, LexicalError>) -> ParseError {
    match err {
        // TODO: Are there cases where this isn't an EOF?
        LalrpopError::InvalidToken { location } => ParseError {
            error: ParseErrorType::Eof,
            location: TextRange::empty(location),
        },
        LalrpopError::ExtraToken { token } => ParseError {
            error: ParseErrorType::ExtraToken(token.1),
            location: TextRange::new(token.0, token.2),
        },
        LalrpopError::User { error } => ParseError {
            location: error.location(),
            error: ParseErrorType::Lexical(error.into_error()),
        },
        LalrpopError::UnrecognizedToken { token, expected } => {
            // Hacky, but it's how CPython does it. See PyParser_AddToken,
            // in particular "Only one possible expected token" comment.
            let expected = (expected.len() == 1).then(|| expected[0].clone());
            ParseError {
                error: ParseErrorType::UnrecognizedToken(token.1, expected),
                location: TextRange::new(token.0, token.2),
            }
        }
        LalrpopError::UnrecognizedEof { location, expected } => {
            // This could be an initial indentation error that we should ignore
            let indent_error = expected == ["Indent"];
            if indent_error {
                ParseError {
                    error: ParseErrorType::Lexical(LexicalErrorType::IndentationError),
                    location: TextRange::empty(location),
                }
            } else {
                ParseError {
                    error: ParseErrorType::Eof,
                    location: TextRange::empty(location),
                }
            }
        }
    }
}

/// An expression that may be parenthesized.
#[derive(Clone, Debug)]
struct ParenthesizedExpr {
    /// The range of the expression, including any parentheses.
    range: TextRange,
    /// The underlying expression.
    expr: Expr,
}

impl ParenthesizedExpr {
    /// Returns `true` if the expression is parenthesized.
    fn is_parenthesized(&self) -> bool {
        self.range.start() != self.expr.range().start()
    }
}

impl Ranged for ParenthesizedExpr {
    fn range(&self) -> TextRange {
        self.range
    }
}
impl From<Expr> for ParenthesizedExpr {
    fn from(expr: Expr) -> Self {
        ParenthesizedExpr {
            range: expr.range(),
            expr,
        }
    }
}
impl From<ParenthesizedExpr> for Expr {
    fn from(parenthesized_expr: ParenthesizedExpr) -> Self {
        parenthesized_expr.expr
    }
}
impl From<ExprIpyEscapeCommand> for ParenthesizedExpr {
    fn from(payload: ExprIpyEscapeCommand) -> Self {
        Expr::IpyEscapeCommand(payload).into()
    }
}
impl From<ExprBoolOp> for ParenthesizedExpr {
    fn from(payload: ExprBoolOp) -> Self {
        Expr::BoolOp(payload).into()
    }
}
impl From<ExprNamedExpr> for ParenthesizedExpr {
    fn from(payload: ExprNamedExpr) -> Self {
        Expr::NamedExpr(payload).into()
    }
}
impl From<ExprBinOp> for ParenthesizedExpr {
    fn from(payload: ExprBinOp) -> Self {
        Expr::BinOp(payload).into()
    }
}
impl From<ExprUnaryOp> for ParenthesizedExpr {
    fn from(payload: ExprUnaryOp) -> Self {
        Expr::UnaryOp(payload).into()
    }
}
impl From<ExprLambda> for ParenthesizedExpr {
    fn from(payload: ExprLambda) -> Self {
        Expr::Lambda(payload).into()
    }
}
impl From<ExprIfExp> for ParenthesizedExpr {
    fn from(payload: ExprIfExp) -> Self {
        Expr::IfExp(payload).into()
    }
}
impl From<ExprDict> for ParenthesizedExpr {
    fn from(payload: ExprDict) -> Self {
        Expr::Dict(payload).into()
    }
}
impl From<ExprSet> for ParenthesizedExpr {
    fn from(payload: ExprSet) -> Self {
        Expr::Set(payload).into()
    }
}
impl From<ExprListComp> for ParenthesizedExpr {
    fn from(payload: ExprListComp) -> Self {
        Expr::ListComp(payload).into()
    }
}
impl From<ExprSetComp> for ParenthesizedExpr {
    fn from(payload: ExprSetComp) -> Self {
        Expr::SetComp(payload).into()
    }
}
impl From<ExprDictComp> for ParenthesizedExpr {
    fn from(payload: ExprDictComp) -> Self {
        Expr::DictComp(payload).into()
    }
}
impl From<ExprGeneratorExp> for ParenthesizedExpr {
    fn from(payload: ExprGeneratorExp) -> Self {
        Expr::GeneratorExp(payload).into()
    }
}
impl From<ExprAwait> for ParenthesizedExpr {
    fn from(payload: ExprAwait) -> Self {
        Expr::Await(payload).into()
    }
}
impl From<ExprYield> for ParenthesizedExpr {
    fn from(payload: ExprYield) -> Self {
        Expr::Yield(payload).into()
    }
}
impl From<ExprYieldFrom> for ParenthesizedExpr {
    fn from(payload: ExprYieldFrom) -> Self {
        Expr::YieldFrom(payload).into()
    }
}
impl From<ExprCompare> for ParenthesizedExpr {
    fn from(payload: ExprCompare) -> Self {
        Expr::Compare(payload).into()
    }
}
impl From<ExprCall> for ParenthesizedExpr {
    fn from(payload: ExprCall) -> Self {
        Expr::Call(payload).into()
    }
}
impl From<ExprFString> for ParenthesizedExpr {
    fn from(payload: ExprFString) -> Self {
        Expr::FString(payload).into()
    }
}
impl From<ExprStringLiteral> for ParenthesizedExpr {
    fn from(payload: ExprStringLiteral) -> Self {
        Expr::StringLiteral(payload).into()
    }
}
impl From<ExprBytesLiteral> for ParenthesizedExpr {
    fn from(payload: ExprBytesLiteral) -> Self {
        Expr::BytesLiteral(payload).into()
    }
}
impl From<ExprNumberLiteral> for ParenthesizedExpr {
    fn from(payload: ExprNumberLiteral) -> Self {
        Expr::NumberLiteral(payload).into()
    }
}
impl From<ExprBooleanLiteral> for ParenthesizedExpr {
    fn from(payload: ExprBooleanLiteral) -> Self {
        Expr::BooleanLiteral(payload).into()
    }
}
impl From<ExprNoneLiteral> for ParenthesizedExpr {
    fn from(payload: ExprNoneLiteral) -> Self {
        Expr::NoneLiteral(payload).into()
    }
}
impl From<ExprEllipsisLiteral> for ParenthesizedExpr {
    fn from(payload: ExprEllipsisLiteral) -> Self {
        Expr::EllipsisLiteral(payload).into()
    }
}
impl From<ExprAttribute> for ParenthesizedExpr {
    fn from(payload: ExprAttribute) -> Self {
        Expr::Attribute(payload).into()
    }
}
impl From<ExprSubscript> for ParenthesizedExpr {
    fn from(payload: ExprSubscript) -> Self {
        Expr::Subscript(payload).into()
    }
}
impl From<ExprStarred> for ParenthesizedExpr {
    fn from(payload: ExprStarred) -> Self {
        Expr::Starred(payload).into()
    }
}
impl From<ExprName> for ParenthesizedExpr {
    fn from(payload: ExprName) -> Self {
        Expr::Name(payload).into()
    }
}
impl From<ExprList> for ParenthesizedExpr {
    fn from(payload: ExprList) -> Self {
        Expr::List(payload).into()
    }
}
impl From<ExprTuple> for ParenthesizedExpr {
    fn from(payload: ExprTuple) -> Self {
        Expr::Tuple(payload).into()
    }
}
impl From<ExprSlice> for ParenthesizedExpr {
    fn from(payload: ExprSlice) -> Self {
        Expr::Slice(payload).into()
    }
}

#[cfg(target_pointer_width = "64")]
mod size_assertions {
    use static_assertions::assert_eq_size;

    use super::ParenthesizedExpr;

    assert_eq_size!(ParenthesizedExpr, [u8; 72]);
}
