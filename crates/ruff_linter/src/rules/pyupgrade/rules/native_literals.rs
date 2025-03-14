use std::fmt;
use std::str::FromStr;

use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr, Int, LiteralExpressionRef, OperatorPrecedence, UnaryOp};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum LiteralType {
    Str,
    Bytes,
    Int,
    Float,
    Bool,
}

impl FromStr for LiteralType {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "str" => Ok(LiteralType::Str),
            "bytes" => Ok(LiteralType::Bytes),
            "int" => Ok(LiteralType::Int),
            "float" => Ok(LiteralType::Float),
            "bool" => Ok(LiteralType::Bool),
            _ => Err(()),
        }
    }
}

impl LiteralType {
    fn as_zero_value_expr(self, checker: &Checker) -> Expr {
        match self {
            LiteralType::Str => ast::StringLiteral {
                value: Box::default(),
                range: TextRange::default(),
                flags: checker.default_string_flags(),
            }
            .into(),
            LiteralType::Bytes => ast::BytesLiteral {
                value: Box::default(),
                range: TextRange::default(),
                flags: checker.default_bytes_flags(),
            }
            .into(),
            LiteralType::Int => ast::ExprNumberLiteral {
                value: ast::Number::Int(Int::from(0u8)),
                range: TextRange::default(),
            }
            .into(),
            LiteralType::Float => ast::ExprNumberLiteral {
                value: ast::Number::Float(0.0),
                range: TextRange::default(),
            }
            .into(),
            LiteralType::Bool => ast::ExprBooleanLiteral::default().into(),
        }
    }
}

impl TryFrom<LiteralExpressionRef<'_>> for LiteralType {
    type Error = ();

    fn try_from(literal_expr: LiteralExpressionRef<'_>) -> Result<Self, Self::Error> {
        match literal_expr {
            LiteralExpressionRef::StringLiteral(_) => Ok(LiteralType::Str),
            LiteralExpressionRef::BytesLiteral(_) => Ok(LiteralType::Bytes),
            LiteralExpressionRef::NumberLiteral(ast::ExprNumberLiteral { value, .. }) => {
                match value {
                    ast::Number::Int(_) => Ok(LiteralType::Int),
                    ast::Number::Float(_) => Ok(LiteralType::Float),
                    ast::Number::Complex { .. } => Err(()),
                }
            }
            LiteralExpressionRef::BooleanLiteral(_) => Ok(LiteralType::Bool),
            LiteralExpressionRef::NoneLiteral(_) | LiteralExpressionRef::EllipsisLiteral(_) => {
                Err(())
            }
        }
    }
}

impl fmt::Display for LiteralType {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LiteralType::Str => fmt.write_str("str"),
            LiteralType::Bytes => fmt.write_str("bytes"),
            LiteralType::Int => fmt.write_str("int"),
            LiteralType::Float => fmt.write_str("float"),
            LiteralType::Bool => fmt.write_str("bool"),
        }
    }
}

/// ## What it does
/// Checks for unnecessary calls to `str`, `bytes`, `int`, `float`, and `bool`.
///
/// ## Why is this bad?
/// The mentioned constructors can be replaced with their respective literal
/// forms, which are more readable and idiomatic.
///
/// ## Example
/// ```python
/// str("foo")
/// ```
///
/// Use instead:
/// ```python
/// "foo"
/// ```
///
/// ## Fix safety
/// The fix is marked as unsafe if it might remove comments.
///
/// ## References
/// - [Python documentation: `str`](https://docs.python.org/3/library/stdtypes.html#str)
/// - [Python documentation: `bytes`](https://docs.python.org/3/library/stdtypes.html#bytes)
/// - [Python documentation: `int`](https://docs.python.org/3/library/functions.html#int)
/// - [Python documentation: `float`](https://docs.python.org/3/library/functions.html#float)
/// - [Python documentation: `bool`](https://docs.python.org/3/library/functions.html#bool)
#[derive(ViolationMetadata)]
pub(crate) struct NativeLiterals {
    literal_type: LiteralType,
}

impl AlwaysFixableViolation for NativeLiterals {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NativeLiterals { literal_type } = self;
        format!("Unnecessary `{literal_type}` call (rewrite as a literal)")
    }

    fn fix_title(&self) -> String {
        let NativeLiterals { literal_type } = self;
        match literal_type {
            LiteralType::Str => "Replace with string literal".to_string(),
            LiteralType::Bytes => "Replace with bytes literal".to_string(),
            LiteralType::Int => "Replace with integer literal".to_string(),
            LiteralType::Float => "Replace with float literal".to_string(),
            LiteralType::Bool => "Replace with boolean literal".to_string(),
        }
    }
}

/// UP018
pub(crate) fn native_literals(
    checker: &Checker,
    call: &ast::ExprCall,
    parent_expr: Option<&ast::Expr>,
) {
    let ast::ExprCall {
        func,
        arguments:
            ast::Arguments {
                args,
                keywords,
                range: _,
            },
        range: _,
    } = call;

    if !keywords.is_empty() || args.len() > 1 {
        return;
    }

    let semantic = checker.semantic();

    let Some(builtin) = semantic.resolve_builtin_symbol(func) else {
        return;
    };

    let Ok(literal_type) = LiteralType::from_str(builtin) else {
        return;
    };

    // There's no way to rewrite, e.g., `f"{f'{str()}'}"` within a nested f-string.
    if semantic.in_f_string() {
        if semantic
            .current_expressions()
            .filter(|expr| expr.is_f_string_expr())
            .count()
            > 1
        {
            return;
        }
    }

    match args.first() {
        None => {
            let mut diagnostic = Diagnostic::new(NativeLiterals { literal_type }, call.range());

            // Do not suggest fix for attribute access on an int like `int().attribute`
            // Ex) `int().denominator` is valid but `0.denominator` is not
            if literal_type == LiteralType::Int && matches!(parent_expr, Some(Expr::Attribute(_))) {
                return;
            }

            let expr = literal_type.as_zero_value_expr(checker);
            let content = checker.generator().expr(&expr);
            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                content,
                call.range(),
            )));
            checker.report_diagnostic(diagnostic);
        }
        Some(arg) => {
            let (has_unary_op, literal_expr) = if let Some(literal_expr) = arg.as_literal_expr() {
                // Skip implicit concatenated strings.
                if literal_expr.is_implicit_concatenated() {
                    return;
                }
                (false, literal_expr)
            } else if let Expr::UnaryOp(ast::ExprUnaryOp {
                op: UnaryOp::UAdd | UnaryOp::USub,
                operand,
                ..
            }) = arg
            {
                if let Some(literal_expr) = operand
                    .as_literal_expr()
                    .filter(|expr| matches!(expr, LiteralExpressionRef::NumberLiteral(_)))
                {
                    (true, literal_expr)
                } else {
                    // Only allow unary operators for numbers.
                    return;
                }
            } else {
                return;
            };

            let Ok(arg_literal_type) = LiteralType::try_from(literal_expr) else {
                return;
            };

            if arg_literal_type != literal_type {
                return;
            }

            let arg_code = checker.locator().slice(arg);

            let content = match (parent_expr, literal_type, has_unary_op) {
                // Attribute access on an integer requires the integer to be parenthesized to disambiguate from a float
                // Ex) `(7).denominator` is valid but `7.denominator` is not
                // Note that floats do not have this problem
                // Ex) `(1.0).real` is valid and `1.0.real` is too
                (Some(Expr::Attribute(_)), LiteralType::Int, _) => format!("({arg_code})"),

                (Some(parent), _, _) => {
                    if OperatorPrecedence::from(parent) > OperatorPrecedence::from(arg) {
                        format!("({arg_code})")
                    } else {
                        arg_code.to_string()
                    }
                }

                _ => arg_code.to_string(),
            };

            let applicability = if checker.comment_ranges().intersects(call.range) {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            };
            let edit = Edit::range_replacement(content, call.range());
            let fix = Fix::applicable_edit(edit, applicability);

            let diagnostic = Diagnostic::new(NativeLiterals { literal_type }, call.range());
            checker.report_diagnostic(diagnostic.with_fix(fix));
        }
    }
}
