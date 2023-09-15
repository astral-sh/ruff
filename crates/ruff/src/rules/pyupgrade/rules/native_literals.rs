use std::fmt;
use std::str::FromStr;

use num_bigint::BigInt;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Constant, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

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

impl From<LiteralType> for Constant {
    fn from(value: LiteralType) -> Self {
        match value {
            LiteralType::Str => Constant::Str(ast::StringConstant {
                value: String::new(),
                unicode: false,
                implicit_concatenated: false,
            }),
            LiteralType::Bytes => Constant::Bytes(ast::BytesConstant {
                value: Vec::new(),
                implicit_concatenated: false,
            }),
            LiteralType::Int => Constant::Int(BigInt::from(0)),
            LiteralType::Float => Constant::Float(0.0),
            LiteralType::Bool => Constant::Bool(false),
        }
    }
}

impl TryFrom<&Constant> for LiteralType {
    type Error = ();

    fn try_from(value: &Constant) -> Result<Self, Self::Error> {
        match value {
            Constant::Str(_) => Ok(LiteralType::Str),
            Constant::Bytes(_) => Ok(LiteralType::Bytes),
            Constant::Int(_) => Ok(LiteralType::Int),
            Constant::Float(_) => Ok(LiteralType::Float),
            Constant::Bool(_) => Ok(LiteralType::Bool),
            _ => Err(()),
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
/// ## References
/// - [Python documentation: `str`](https://docs.python.org/3/library/stdtypes.html#str)
/// - [Python documentation: `bytes`](https://docs.python.org/3/library/stdtypes.html#bytes)
/// - [Python documentation: `int`](https://docs.python.org/3/library/functions.html#int)
/// - [Python documentation: `float`](https://docs.python.org/3/library/functions.html#float)
/// - [Python documentation: `bool`](https://docs.python.org/3/library/functions.html#bool)
#[violation]
pub struct NativeLiterals {
    literal_type: LiteralType,
}

impl AlwaysAutofixableViolation for NativeLiterals {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NativeLiterals { literal_type } = self;
        format!("Unnecessary `{literal_type}` call (rewrite as a literal)")
    }

    fn autofix_title(&self) -> String {
        let NativeLiterals { literal_type } = self;
        match literal_type {
            LiteralType::Str => "Replace with empty string".to_string(),
            LiteralType::Bytes => "Replace with empty bytes".to_string(),
            LiteralType::Int => "Replace with 0".to_string(),
            LiteralType::Float => "Replace with 0.0".to_string(),
            LiteralType::Bool => "Replace with `False`".to_string(),
        }
    }
}

/// UP018
pub(crate) fn native_literals(
    checker: &mut Checker,
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

    let Expr::Name(ast::ExprName { ref id, .. }) = func.as_ref() else {
        return;
    };

    if !keywords.is_empty() || args.len() > 1 {
        return;
    }

    let Ok(literal_type) = LiteralType::from_str(id.as_str()) else {
        return;
    };

    if !checker.semantic().is_builtin(id) {
        return;
    }

    // There's no way to rewrite, e.g., `f"{f'{str()}'}"` within a nested f-string.
    if checker.semantic().in_f_string() {
        if checker
            .semantic()
            .current_expressions()
            .filter(|expr| expr.is_f_string_expr())
            .count()
            > 1
        {
            return;
        }
    }

    match args.get(0) {
        None => {
            let mut diagnostic = Diagnostic::new(NativeLiterals { literal_type }, call.range());

            // Do not suggest fix for attribute access on an int like `int().attribute`
            // Ex) `int().denominator` is valid but `0.denominator` is not
            if literal_type == LiteralType::Int && matches!(parent_expr, Some(Expr::Attribute(_))) {
                return;
            }

            if checker.patch(diagnostic.kind.rule()) {
                let constant = Constant::from(literal_type);
                let content = checker.generator().constant(&constant);
                diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                    content,
                    call.range(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
        Some(arg) => {
            let Expr::Constant(ast::ExprConstant { value, .. }) = arg else {
                return;
            };

            // Skip implicit string concatenations.
            if value.is_implicit_concatenated() {
                return;
            }

            let Ok(arg_literal_type) = LiteralType::try_from(value) else {
                return;
            };

            if arg_literal_type != literal_type {
                return;
            }

            let arg_code = checker.locator().slice(arg);

            // Attribute access on an integer requires the integer to be parenthesized to disambiguate from a float
            // Ex) `(7).denominator` is valid but `7.denominator` is not
            // Note that floats do not have this problem
            // Ex) `(1.0).real` is valid and `1.0.real` is too
            let content = match (parent_expr, value) {
                (Some(Expr::Attribute(_)), Constant::Int(_)) => format!("({arg_code})"),
                _ => arg_code.to_string(),
            };

            let mut diagnostic = Diagnostic::new(NativeLiterals { literal_type }, call.range());
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                    content,
                    call.range(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
