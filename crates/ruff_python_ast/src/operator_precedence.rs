use crate::flat;
use crate::flat::ExprKind;
use crate::{BoolOp, Expr, Operator, UnaryOp};

/// Represents the precedence levels for Python expressions.
/// Variants at the top have lower precedence and variants at the bottom have
/// higher precedence.
///
/// See: <https://docs.python.org/3/reference/expressions.html#operator-precedence>
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum OperatorPrecedence {
    /// The lowest (virtual) precedence level
    None,
    /// Precedence of `yield` and `yield from` expressions.
    Yield,
    /// Precedence of assignment expressions (`name := expr`).
    Assign,
    /// Precedence of starred expressions (`*expr`).
    Starred,
    /// Precedence of lambda expressions (`lambda args: expr`).
    Lambda,
    /// Precedence of if/else expressions (`expr if cond else expr`).
    IfElse,
    /// Precedence of boolean `or` expressions.
    Or,
    /// Precedence of boolean `and` expressions.
    And,
    /// Precedence of boolean `not` expressions.
    Not,
    /// Precedence of comparisons (`<`, `<=`, `>`, `>=`, `!=`, `==`),
    /// memberships (`in`, `not in`) and identity tests (`is`, `is not`).
    ComparisonsMembershipIdentity,
    /// Precedence of bitwise `|` operator.
    BitOr,
    /// Precedence of bitwise `^` operator.
    BitXor,
    /// Precedence of bitwise `&` operator.
    BitAnd,
    /// Precedence of left and right shift expressions (`<<`, `>>`).
    LeftRightShift,
    /// Precedence of addition and subtraction expressions (`+`, `-`).
    AddSub,
    /// Precedence of multiplication (`*`), matrix multiplication (`@`), division (`/`),
    /// floor division (`//`) and remainder (`%`) expressions.
    MulDivRemain,
    /// Precedence of unary positive (`+`), negative (`-`), and bitwise NOT (`~`) expressions.
    PosNegBitNot,
    /// Precedence of exponentiation expressions (`**`).
    Exponent,
    /// Precedence of `await` expressions.
    Await,
    /// Precedence of call expressions (`()`), attribute access (`.`), and subscript (`[]`) expressions.
    CallAttribute,
    /// Precedence of atomic expressions (literals, names, containers).
    Atomic,
}

// Owned syntax stores the operator in a field. Parsed syntax reads the same
// operator from its payload. Both forms follow the same precedence table.
macro_rules! expression_precedence {
    ($expr:expr, $kind:ident, $($op:tt)+) => {
        match $expr {
            // Binding or parenthesized expression, list display, dictionary display, set display
            $kind::Tuple(_)
            | $kind::Dict(_)
            | $kind::Set(_)
            | $kind::ListComp(_)
            | $kind::List(_)
            | $kind::SetComp(_)
            | $kind::DictComp(_)
            | $kind::Generator(_)
            | $kind::Name(_)
            | $kind::StringLiteral(_)
            | $kind::BytesLiteral(_)
            | $kind::NumberLiteral(_)
            | $kind::BooleanLiteral(_)
            | $kind::NoneLiteral(_)
            | $kind::EllipsisLiteral(_)
            | $kind::FString(_)
            | $kind::TString(_) => Self::Atomic,
            // Subscription, slicing, call, attribute reference
            $kind::Attribute(_)
            | $kind::Subscript(_)
            | $kind::Call(_)
            | $kind::Slice(_) => Self::CallAttribute,

            // Await expression
            $kind::Await(_) => Self::Await,

            // Exponentiation **
            // Handled below along with other binary operators

            // Unary operators: +x, -x, ~x (except boolean not)
            $kind::UnaryOp(operator) => match operator.$($op)+ {
                UnaryOp::UAdd | UnaryOp::USub | UnaryOp::Invert => Self::PosNegBitNot,
                UnaryOp::Not => Self::Not,
            },

            // Math binary ops
            $kind::BinOp(binary_operation) => Self::from(binary_operation.$($op)+),

            // Comparisons: <, <=, >, >=, ==, !=, in, not in, is, is not
            $kind::Compare(_) => Self::ComparisonsMembershipIdentity,

            // Boolean not
            // Handled above in unary operators

            // Boolean operations: and, or
            $kind::BoolOp(bool_op) => Self::from(bool_op.$($op)+),

            // Conditional expressions: x if y else z
            $kind::If(_) => Self::IfElse,

            // Lambda expressions
            $kind::Lambda(_) => Self::Lambda,

            // Unpacking also omitted in the docs, but has almost the lowest precedence,
            // except for assignment & yield expressions. E.g. `[*(v := [1,2])]` is valid
            // but `[*v := [1,2]] would fail on incorrect syntax because * will associate
            // `v` before the assignment.
            $kind::Starred(_) => Self::Starred,

            // Assignment expressions (aka named)
            $kind::Named(_) => Self::Assign,

            // Although omitted in docs, yield expressions may be used inside an expression
            // but must be parenthesized. So for our purposes we assume they just have
            // the lowest "real" precedence.
            $kind::Yield(_) | $kind::YieldFrom(_) => Self::Yield,

            // Not a real python expression, so treat as lowest as well
            $kind::IpyEscapeCommand(_) => Self::None,
        }
    };
}

impl OperatorPrecedence {
    pub fn from_expr(expr: flat::View<'_, Expr>) -> Self {
        expression_precedence!(expr.classify(), ExprKind, op())
    }

    /// Returns `true` if the precedence is right-associative i.e., the operations are evaluated
    /// from right to left.
    pub fn is_right_associative(self) -> bool {
        matches!(self, OperatorPrecedence::Exponent)
    }
}

impl From<flat::View<'_, Expr>> for OperatorPrecedence {
    fn from(expr: flat::View<'_, Expr>) -> Self {
        Self::from_expr(expr)
    }
}

impl From<&Expr> for OperatorPrecedence {
    fn from(expr: &Expr) -> Self {
        expression_precedence!(expr, Expr, op)
    }
}

impl From<Operator> for OperatorPrecedence {
    fn from(operator: Operator) -> Self {
        match operator {
            // Multiplication, matrix multiplication, division, floor division, remainder:
            // *, @, /, //, %
            Operator::Mult
            | Operator::MatMult
            | Operator::Div
            | Operator::Mod
            | Operator::FloorDiv => Self::MulDivRemain,
            // Addition, subtraction
            Operator::Add | Operator::Sub => Self::AddSub,
            // Bitwise shifts: <<, >>
            Operator::LShift | Operator::RShift => Self::LeftRightShift,
            // Bitwise operations: &, ^, |
            Operator::BitAnd => Self::BitAnd,
            Operator::BitXor => Self::BitXor,
            Operator::BitOr => Self::BitOr,
            // Exponentiation **
            Operator::Pow => Self::Exponent,
        }
    }
}

impl From<BoolOp> for OperatorPrecedence {
    fn from(operator: BoolOp) -> Self {
        match operator {
            BoolOp::And => Self::And,
            BoolOp::Or => Self::Or,
        }
    }
}

impl From<UnaryOp> for OperatorPrecedence {
    fn from(unary_op: UnaryOp) -> Self {
        match unary_op {
            UnaryOp::UAdd | UnaryOp::USub | UnaryOp::Invert => Self::PosNegBitNot,
            UnaryOp::Not => Self::Not,
        }
    }
}
