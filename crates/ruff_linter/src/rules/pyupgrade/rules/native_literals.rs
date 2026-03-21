use std::fmt;
use std::str::FromStr;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, Int, LiteralExpressionRef, OperatorPrecedence, UnaryOp};
use ruff_source_file::find_newline;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Applicability, Edit, Fix};

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum LiteralType {
    Str,
    Bytes,
    Int,
    Float,
    Bool,
    Complex,
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
            "complex" => Ok(LiteralType::Complex),
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
                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
                flags: checker.default_string_flags(),
            }
            .into(),
            LiteralType::Bytes => ast::BytesLiteral {
                value: Box::default(),
                range: TextRange::default(),
                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
                flags: checker.default_bytes_flags(),
            }
            .into(),
            LiteralType::Int => ast::ExprNumberLiteral {
                value: ast::Number::Int(Int::from(0u8)),
                range: TextRange::default(),
                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
            }
            .into(),
            LiteralType::Float => ast::ExprNumberLiteral {
                value: ast::Number::Float(0.0),
                range: TextRange::default(),
                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
            }
            .into(),
            LiteralType::Bool => ast::ExprBooleanLiteral::default().into(),
            LiteralType::Complex => ast::ExprNumberLiteral {
                value: ast::Number::Complex {
                    real: 0.0,
                    imag: 0.0,
                },
                range: TextRange::default(),
                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
            }
            .into(),
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
                    ast::Number::Complex { .. } => Ok(LiteralType::Complex),
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
            LiteralType::Complex => fmt.write_str("complex"),
        }
    }
}

/// ## What it does
/// Checks for unnecessary calls to `str`, `bytes`, `int`, `float`, `bool`, and `complex`.
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
/// - [Python documentation: `complex`](https://docs.python.org/3/library/functions.html#complex)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.193")]
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
            LiteralType::Complex => "Replace with complex literal".to_string(),
        }
    }
}

/// Returns `true` if the keyword argument is redundant for the given builtin.
fn is_redundant_keyword(builtin: &str, keyword: &ast::Keyword) -> bool {
    let Some(arg) = keyword.arg.as_ref() else {
        return false;
    };
    match builtin {
        "str" => arg == "object",
        "complex" => arg == "real",
        _ => false,
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
                node_index: _,
            },
        range: call_range,
        node_index: _,
    } = call;

    let semantic = checker.semantic();

    let Some(builtin) = semantic.resolve_builtin_symbol(func) else {
        return;
    };

    let call_arg = match (args.as_ref(), keywords.as_ref()) {
        ([], []) => None,
        ([arg], []) => Some(arg),
        ([], [keyword]) if is_redundant_keyword(builtin, keyword) => Some(&keyword.value),
        _ => return,
    };

    let tokens = checker.tokens();

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

    match call_arg {
        None => {
            let mut diagnostic =
                checker.report_diagnostic(NativeLiterals { literal_type }, call.range());

            let expr = literal_type.as_zero_value_expr(checker);
            let mut content = checker.generator().expr(&expr);

            // Attribute access on an integer requires the integer to be parenthesized to disambiguate from a float
            // Ex) `(0).denominator` is valid but `0.denominator` is not
            if literal_type == LiteralType::Int && matches!(parent_expr, Some(Expr::Attribute(_))) {
                content = format!("({content})");
            }

            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                content,
                call.range(),
            )));
        }
        Some(arg) => {
            let (has_unary_op, literal_expr) = if let Some(literal_expr) = arg.as_literal_expr() {
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

            let mut needs_space = false;
            // Look for the `Rpar` token of the call expression and check if there is a keyword token right
            // next to it without any space separating them. Without this check, the fix for this
            // rule would create a syntax error.
            // Ex) `bool(True)and None` no space between `)` and the keyword `and`.
            //
            // Subtract 1 from the end of the range to include `Rpar` token in the slice.
            if let [paren_token, next_token, ..] = tokens.after(call_range.sub_end(1.into()).end())
            {
                needs_space = next_token.kind().is_keyword()
                    && paren_token.range().end() == next_token.range().start();
            }

            let mut content = match (parent_expr, literal_type, has_unary_op) {
                // Expressions including newlines must be parenthesised to be valid syntax
                (_, _, true) if find_newline(arg_code).is_some() => format!("({arg_code})"),

                // Implicitly concatenated strings spanning multiple lines must be parenthesized
                (_, LiteralType::Str | LiteralType::Bytes, _)
                    if literal_expr.is_implicit_concatenated()
                        && find_newline(arg_code).is_some() =>
                {
                    format!("({arg_code})")
                }

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

            if needs_space {
                content.push(' ');
            }

            let applicability = if checker.comment_ranges().intersects(call.range) {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            };
            let edit = Edit::range_replacement(content, call.range());
            let fix = Fix::applicable_edit(edit, applicability);

            checker
                .report_diagnostic(NativeLiterals { literal_type }, call.range())
                .set_fix(fix);
        }
    }
}
