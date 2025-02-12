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
    /// Precedence of bitwise `|` and `^` operators.
    BitXorOr,
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

impl OperatorPrecedence {
    pub fn from_expr(expr: &Expr) -> Self {
        match expr {
            // Binding or parenthesized expression, list display, dictionary display, set display
            Expr::Tuple(_)
            | Expr::Dict(_)
            | Expr::Set(_)
            | Expr::ListComp(_)
            | Expr::List(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_)
            | Expr::Generator(_)
            | Expr::Name(_)
            | Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
            | Expr::NumberLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::NoneLiteral(_)
            | Expr::EllipsisLiteral(_)
            | Expr::FString(_) => Self::Atomic,
            // Subscription, slicing, call, attribute reference
            Expr::Attribute(_) | Expr::Subscript(_) | Expr::Call(_) | Expr::Slice(_) => {
                Self::CallAttribute
            }

            // Await expression
            Expr::Await(_) => Self::Await,

            // Exponentiation **
            // Handled below along with other binary operators

            // Unary operators: +x, -x, ~x (except boolean not)
            Expr::UnaryOp(operator) => match operator.op {
                UnaryOp::UAdd | UnaryOp::USub | UnaryOp::Invert => Self::PosNegBitNot,
                UnaryOp::Not => Self::Not,
            },

            // Math binary ops
            Expr::BinOp(binary_operation) => Self::from(binary_operation.op),

            // Comparisons: <, <=, >, >=, ==, !=, in, not in, is, is not
            Expr::Compare(_) => Self::ComparisonsMembershipIdentity,

            // Boolean not
            // Handled above in unary operators

            // Boolean operations: and, or
            Expr::BoolOp(bool_op) => Self::from(bool_op.op),

            // Conditional expressions: x if y else z
            Expr::If(_) => Self::IfElse,

            // Lambda expressions
            Expr::Lambda(_) => Self::Lambda,

            // Unpacking also omitted in the docs, but has almost the lowest precedence,
            // except for assignment & yield expressions. E.g. `[*(v := [1,2])]` is valid
            // but `[*v := [1,2]] would fail on incorrect syntax because * will associate
            // `v` before the assignment.
            Expr::Starred(_) => Self::Starred,

            // Assignment expressions (aka named)
            Expr::Named(_) => Self::Assign,

            // Although omitted in docs, yield expressions may be used inside an expression
            // but must be parenthesized. So for our purposes we assume they just have
            // the lowest "real" precedence.
            Expr::Yield(_) | Expr::YieldFrom(_) => Self::Yield,

            // Not a real python expression, so treat as lowest as well
            Expr::IpyEscapeCommand(_) => Self::None,
        }
    }
}

impl From<&Expr> for OperatorPrecedence {
    fn from(expr: &Expr) -> Self {
        Self::from_expr(expr)
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
            Operator::BitXor | Operator::BitOr => Self::BitXorOr,
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

