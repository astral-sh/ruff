use crate::{BoolOp, Expr, ExprRef, Operator, UnaryOp};

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

impl OperatorPrecedence {
    pub fn from_expr_ref(expr: &ExprRef) -> Self {
        match expr {
            // Binding or parenthesized expression, list display, dictionary display, set display
            ExprRef::Tuple(_)
            | ExprRef::Dict(_)
            | ExprRef::Set(_)
            | ExprRef::ListComp(_)
            | ExprRef::List(_)
            | ExprRef::SetComp(_)
            | ExprRef::DictComp(_)
            | ExprRef::Generator(_)
            | ExprRef::Name(_)
            | ExprRef::StringLiteral(_)
            | ExprRef::BytesLiteral(_)
            | ExprRef::NumberLiteral(_)
            | ExprRef::BooleanLiteral(_)
            | ExprRef::NoneLiteral(_)
            | ExprRef::EllipsisLiteral(_)
            | ExprRef::FString(_) => Self::Atomic,
            // Subscription, slicing, call, attribute reference
            ExprRef::Attribute(_)
            | ExprRef::Subscript(_)
            | ExprRef::Call(_)
            | ExprRef::Slice(_) => Self::CallAttribute,

            // Await expression
            ExprRef::Await(_) => Self::Await,

            // Exponentiation **
            // Handled below along with other binary operators

            // Unary operators: +x, -x, ~x (except boolean not)
            ExprRef::UnaryOp(operator) => match operator.op {
                UnaryOp::UAdd | UnaryOp::USub | UnaryOp::Invert => Self::PosNegBitNot,
                UnaryOp::Not => Self::Not,
            },

            // Math binary ops
            ExprRef::BinOp(binary_operation) => Self::from(binary_operation.op),

            // Comparisons: <, <=, >, >=, ==, !=, in, not in, is, is not
            ExprRef::Compare(_) => Self::ComparisonsMembershipIdentity,

            // Boolean not
            // Handled above in unary operators

            // Boolean operations: and, or
            ExprRef::BoolOp(bool_op) => Self::from(bool_op.op),

            // Conditional expressions: x if y else z
            ExprRef::If(_) => Self::IfElse,

            // Lambda expressions
            ExprRef::Lambda(_) => Self::Lambda,

            // Unpacking also omitted in the docs, but has almost the lowest precedence,
            // except for assignment & yield expressions. E.g. `[*(v := [1,2])]` is valid
            // but `[*v := [1,2]] would fail on incorrect syntax because * will associate
            // `v` before the assignment.
            ExprRef::Starred(_) => Self::Starred,

            // Assignment expressions (aka named)
            ExprRef::Named(_) => Self::Assign,

            // Although omitted in docs, yield expressions may be used inside an expression
            // but must be parenthesized. So for our purposes we assume they just have
            // the lowest "real" precedence.
            ExprRef::Yield(_) | ExprRef::YieldFrom(_) => Self::Yield,

            // Not a real python expression, so treat as lowest as well
            ExprRef::IpyEscapeCommand(_) => Self::None,
        }
    }

    pub fn from_expr(expr: &Expr) -> Self {
        Self::from(&ExprRef::from(expr))
    }

    /// Returns `true` if the precedence is right-associative i.e., the operations are evaluated
    /// from right to left.
    pub fn is_right_associative(self) -> bool {
        matches!(self, OperatorPrecedence::Exponent)
    }
}

impl From<&Expr> for OperatorPrecedence {
    fn from(expr: &Expr) -> Self {
        Self::from_expr(expr)
    }
}

impl<'a> From<&ExprRef<'a>> for OperatorPrecedence {
    fn from(expr_ref: &ExprRef<'a>) -> Self {
        Self::from_expr_ref(expr_ref)
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
