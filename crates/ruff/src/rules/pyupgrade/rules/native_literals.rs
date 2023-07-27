use std::fmt;
use std::str::FromStr;

use num_bigint::BigInt;
use ruff_python_ast::{self as ast, Constant, Expr, Keyword, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::str::is_implicit_concatenation;

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
            LiteralType::Str => Constant::Str(String::new()),
            LiteralType::Bytes => Constant::Bytes(vec![]),
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
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Expr::Name(ast::ExprName { id, .. }) = func else {
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
            .expr_ancestors()
            .filter(|expr| expr.is_joined_str_expr())
            .count()
            > 1
        {
            return;
        }
    }

    match args.get(0) {
        None => {
            let mut diagnostic = Diagnostic::new(NativeLiterals { literal_type }, expr.range());
            if checker.patch(diagnostic.kind.rule()) {
                let constant = Constant::from(literal_type);
                let content = checker.generator().constant(&constant);
                diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                    content,
                    expr.range(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
        Some(arg) => {
            let Expr::Constant(ast::ExprConstant { value, .. }) = arg else {
                return;
            };

            let Ok(arg_literal_type) = LiteralType::try_from(value) else {
                return;
            };

            if arg_literal_type != literal_type {
                return;
            }

            let arg_code = checker.locator().slice(arg.range());

            // Skip implicit string concatenations.
            if matches!(arg_literal_type, LiteralType::Str | LiteralType::Bytes)
                && is_implicit_concatenation(arg_code)
            {
                return;
            }

            let mut diagnostic = Diagnostic::new(NativeLiterals { literal_type }, expr.range());
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                    arg_code.to_string(),
                    expr.range(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
