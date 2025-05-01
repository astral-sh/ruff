use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr, OperatorPrecedence, Stmt};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::pylint::helpers::is_known_dunder_method;
use ruff_python_ast::PythonVersion;

/// ## What it does
/// Checks for explicit use of dunder methods, like `__str__` and `__add__`.
///
/// ## Why is this bad?
/// Dunder names are not meant to be called explicitly and, in most cases, can
/// be replaced with builtins or operators.
///
/// ## Example
/// ```python
/// three = (3.0).__str__()
/// twelve = "1".__add__("2")
///
///
/// def is_greater_than_two(x: int) -> bool:
///     return x.__gt__(2)
/// ```
///
/// Use instead:
/// ```python
/// three = str(3.0)
/// twelve = "1" + "2"
///
///
/// def is_greater_than_two(x: int) -> bool:
///     return x > 2
/// ```
///
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryDunderCall {
    method: String,
    replacement: Option<String>,
}

impl Violation for UnnecessaryDunderCall {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryDunderCall {
            method,
            replacement,
        } = self;

        if let Some(replacement) = replacement {
            format!("Unnecessary dunder call to `{method}`. {replacement}.")
        } else {
            format!("Unnecessary dunder call to `{method}`")
        }
    }

    fn fix_title(&self) -> Option<String> {
        let UnnecessaryDunderCall { replacement, .. } = self;
        replacement.clone()
    }
}

/// PLC2801
pub(crate) fn unnecessary_dunder_call(checker: &Checker, call: &ast::ExprCall) {
    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = call.func.as_ref() else {
        return;
    };

    // If this isn't a known dunder method, abort.
    if !is_known_dunder_method(attr) {
        return;
    }

    // If this is an allowed dunder method, abort.
    if allowed_dunder_constants(attr, checker.target_version()) {
        return;
    }

    // Ignore certain dunder method calls in lambda expressions. These methods would require
    // rewriting as a statement, which is not possible in a lambda expression.
    if allow_nested_expression(attr, checker.semantic()) {
        return;
    }

    // Ignore dunder method calls within dunder methods definitions.
    if in_dunder_method_definition(checker.semantic()) {
        return;
    }

    // Ignore dunder methods used on `super`.
    if let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() {
        if checker.semantic().has_builtin_binding("super") {
            if let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() {
                if id == "super" {
                    return;
                }
            }
        }
    }

    // If the call has keywords, abort.
    if !call.arguments.keywords.is_empty() {
        return;
    }

    // If a fix is available, we'll store the text of the fixed expression here
    // along with the precedence of the resulting expression.
    let mut fixed: Option<(String, OperatorPrecedence)> = None;
    let mut title: Option<String> = None;

    if let Some(dunder) = DunderReplacement::from_method(attr) {
        match (&*call.arguments.args, dunder) {
            ([], DunderReplacement::Builtin(replacement, message)) => {
                if !checker.semantic().has_builtin_binding(replacement) {
                    return;
                }
                fixed = Some((
                    format!(
                        "{}({})",
                        replacement,
                        checker.locator().slice(value.as_ref()),
                    ),
                    OperatorPrecedence::CallAttribute,
                ));
                title = Some(message.to_string());
            }
            ([arg], DunderReplacement::Operator(replacement, message, precedence)) => {
                let value_slice = checker.locator().slice(value.as_ref());
                let arg_slice = checker.locator().slice(arg);

                if OperatorPrecedence::from_expr(arg) > precedence {
                    // if it's something that can reasonably be removed from parentheses,
                    // we'll do that.
                    fixed = Some((
                        format!("{value_slice} {replacement} {arg_slice}"),
                        precedence,
                    ));
                } else {
                    fixed = Some((
                        format!("{value_slice} {replacement} ({arg_slice})"),
                        precedence,
                    ));
                }

                title = Some(message.to_string());
            }
            ([arg], DunderReplacement::ROperator(replacement, message, precedence)) => {
                let value_slice = checker.locator().slice(value.as_ref());
                let arg_slice = checker.locator().slice(arg);

                if OperatorPrecedence::from_expr(arg) > precedence {
                    // if it's something that can reasonably be removed from parentheses,
                    // we'll do that.
                    fixed = Some((
                        format!("{arg_slice} {replacement} {value_slice}"),
                        precedence,
                    ));
                } else {
                    fixed = Some((
                        format!("({arg_slice}) {replacement} {value_slice}"),
                        precedence,
                    ));
                }
                title = Some(message.to_string());
            }
            (_, DunderReplacement::MessageOnly(message)) => {
                title = Some(message.to_string());
            }
            _ => {}
        }
    }

    let mut diagnostic = Diagnostic::new(
        UnnecessaryDunderCall {
            method: attr.to_string(),
            replacement: title,
        },
        call.range(),
    );

    if let Some((mut fixed, precedence)) = fixed {
        let dunder = DunderReplacement::from_method(attr);

        // We never need to wrap builtin functions in extra parens
        // since function calls have high precedence
        let wrap_in_paren = (!matches!(dunder, Some(DunderReplacement::Builtin(_,_))))
        // If parent expression has higher precedence then the new replacement,
        // it would associate with either the left operand (e.g. naive change from `a * b.__add__(c)`
        // becomes `a * b + c` which is incorrect) or the right operand (e.g. naive change from
        // `a.__add__(b).attr` becomes `a + b.attr` which is also incorrect).
        // This rule doesn't apply to function calls despite them having higher
        // precedence than any of our replacement, since they already wrap around
        // our expression e.g. `print(a.__add__(3))` -> `print(a + 3)`
            && checker
                .semantic()
                .current_expression_parent()
                .is_some_and(|parent| !parent.is_call_expr() && OperatorPrecedence::from_expr(parent) > precedence);

        if wrap_in_paren {
            fixed = format!("({fixed})");
        }

        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            fixed,
            call.range(),
        )));
    }

    checker.report_diagnostic(diagnostic);
}

/// Return `true` if this is a dunder method that is allowed to be called explicitly.
fn allowed_dunder_constants(dunder_method: &str, target_version: PythonVersion) -> bool {
    if matches!(
        dunder_method,
        "__aexit__"
            | "__await__"
            | "__class__"
            | "__class_getitem__"
            | "__delete__"
            | "__dict__"
            | "__doc__"
            | "__exit__"
            | "__get__"
            | "__getnewargs__"
            | "__getnewargs_ex__"
            | "__getstate__"
            | "__index__"
            | "__init_subclass__"
            | "__missing__"
            | "__module__"
            | "__new__"
            | "__post_init__"
            | "__reduce__"
            | "__reduce_ex__"
            | "__set__"
            | "__set_name__"
            | "__setstate__"
            | "__sizeof__"
            | "__subclasses__"
            | "__subclasshook__"
            | "__weakref__"
    ) {
        return true;
    }

    if target_version < PythonVersion::PY310 && matches!(dunder_method, "__aiter__" | "__anext__") {
        return true;
    }

    false
}

#[derive(Debug, Copy, Clone)]
enum DunderReplacement {
    /// A dunder method that is an operator.
    Operator(&'static str, &'static str, OperatorPrecedence),
    /// A dunder method that is a right-side operator.
    ROperator(&'static str, &'static str, OperatorPrecedence),
    /// A dunder method that is a builtin.
    Builtin(&'static str, &'static str),
    /// A dunder method that is a message only.
    MessageOnly(&'static str),
}

impl DunderReplacement {
    fn from_method(dunder_method: &str) -> Option<Self> {
        match dunder_method {
            "__add__" => Some(Self::Operator(
                "+",
                "Use `+` operator",
                OperatorPrecedence::AddSub,
            )),
            "__and__" => Some(Self::Operator(
                "&",
                "Use `&` operator",
                OperatorPrecedence::BitAnd,
            )),
            "__contains__" => Some(Self::ROperator(
                "in",
                "Use `in` operator",
                OperatorPrecedence::ComparisonsMembershipIdentity,
            )),
            "__eq__" => Some(Self::Operator(
                "==",
                "Use `==` operator",
                OperatorPrecedence::ComparisonsMembershipIdentity,
            )),
            "__floordiv__" => Some(Self::Operator(
                "//",
                "Use `//` operator",
                OperatorPrecedence::MulDivRemain,
            )),
            "__ge__" => Some(Self::Operator(
                ">=",
                "Use `>=` operator",
                OperatorPrecedence::ComparisonsMembershipIdentity,
            )),
            "__gt__" => Some(Self::Operator(
                ">",
                "Use `>` operator",
                OperatorPrecedence::ComparisonsMembershipIdentity,
            )),
            "__iadd__" => Some(Self::Operator(
                "+=",
                "Use `+=` operator",
                OperatorPrecedence::Assign,
            )),
            "__iand__" => Some(Self::Operator(
                "&=",
                "Use `&=` operator",
                OperatorPrecedence::Assign,
            )),
            "__ifloordiv__" => Some(Self::Operator(
                "//=",
                "Use `//=` operator",
                OperatorPrecedence::Assign,
            )),
            "__ilshift__" => Some(Self::Operator(
                "<<=",
                "Use `<<=` operator",
                OperatorPrecedence::Assign,
            )),
            "__imod__" => Some(Self::Operator(
                "%=",
                "Use `%=` operator",
                OperatorPrecedence::Assign,
            )),
            "__imul__" => Some(Self::Operator(
                "*=",
                "Use `*=` operator",
                OperatorPrecedence::Assign,
            )),
            "__ior__" => Some(Self::Operator(
                "|=",
                "Use `|=` operator",
                OperatorPrecedence::Assign,
            )),
            "__ipow__" => Some(Self::Operator(
                "**=",
                "Use `**=` operator",
                OperatorPrecedence::Assign,
            )),
            "__irshift__" => Some(Self::Operator(
                ">>=",
                "Use `>>=` operator",
                OperatorPrecedence::Assign,
            )),
            "__isub__" => Some(Self::Operator(
                "-=",
                "Use `-=` operator",
                OperatorPrecedence::Assign,
            )),
            "__itruediv__" => Some(Self::Operator(
                "/=",
                "Use `/=` operator",
                OperatorPrecedence::Assign,
            )),
            "__ixor__" => Some(Self::Operator(
                "^=",
                "Use `^=` operator",
                OperatorPrecedence::Assign,
            )),
            "__le__" => Some(Self::Operator(
                "<=",
                "Use `<=` operator",
                OperatorPrecedence::ComparisonsMembershipIdentity,
            )),
            "__lshift__" => Some(Self::Operator(
                "<<",
                "Use `<<` operator",
                OperatorPrecedence::LeftRightShift,
            )),
            "__lt__" => Some(Self::Operator(
                "<",
                "Use `<` operator",
                OperatorPrecedence::ComparisonsMembershipIdentity,
            )),
            "__mod__" => Some(Self::Operator(
                "%",
                "Use `%` operator",
                OperatorPrecedence::MulDivRemain,
            )),
            "__mul__" => Some(Self::Operator(
                "*",
                "Use `*` operator",
                OperatorPrecedence::MulDivRemain,
            )),
            "__ne__" => Some(Self::Operator(
                "!=",
                "Use `!=` operator",
                OperatorPrecedence::ComparisonsMembershipIdentity,
            )),
            "__or__" => Some(Self::Operator(
                "|",
                "Use `|` operator",
                OperatorPrecedence::BitOr,
            )),
            "__rshift__" => Some(Self::Operator(
                ">>",
                "Use `>>` operator",
                OperatorPrecedence::LeftRightShift,
            )),
            "__sub__" => Some(Self::Operator(
                "-",
                "Use `-` operator",
                OperatorPrecedence::AddSub,
            )),
            "__truediv__" => Some(Self::Operator(
                "/",
                "Use `/` operator",
                OperatorPrecedence::MulDivRemain,
            )),
            "__xor__" => Some(Self::Operator(
                "^",
                "Use `^` operator",
                OperatorPrecedence::BitXor,
            )),

            "__radd__" => Some(Self::ROperator(
                "+",
                "Use `+` operator",
                OperatorPrecedence::AddSub,
            )),
            "__rand__" => Some(Self::ROperator(
                "&",
                "Use `&` operator",
                OperatorPrecedence::BitAnd,
            )),
            "__rfloordiv__" => Some(Self::ROperator(
                "//",
                "Use `//` operator",
                OperatorPrecedence::MulDivRemain,
            )),
            "__rlshift__" => Some(Self::ROperator(
                "<<",
                "Use `<<` operator",
                OperatorPrecedence::LeftRightShift,
            )),
            "__rmod__" => Some(Self::ROperator(
                "%",
                "Use `%` operator",
                OperatorPrecedence::MulDivRemain,
            )),
            "__rmul__" => Some(Self::ROperator(
                "*",
                "Use `*` operator",
                OperatorPrecedence::MulDivRemain,
            )),
            "__ror__" => Some(Self::ROperator(
                "|",
                "Use `|` operator",
                OperatorPrecedence::BitOr,
            )),
            "__rrshift__" => Some(Self::ROperator(
                ">>",
                "Use `>>` operator",
                OperatorPrecedence::LeftRightShift,
            )),
            "__rsub__" => Some(Self::ROperator(
                "-",
                "Use `-` operator",
                OperatorPrecedence::AddSub,
            )),
            "__rtruediv__" => Some(Self::ROperator(
                "/",
                "Use `/` operator",
                OperatorPrecedence::MulDivRemain,
            )),
            "__rxor__" => Some(Self::ROperator(
                "^",
                "Use `^` operator",
                OperatorPrecedence::BitXor,
            )),

            "__aiter__" => Some(Self::Builtin("aiter", "Use `aiter()` builtin")),
            "__anext__" => Some(Self::Builtin("anext", "Use `anext()` builtin")),
            "__abs__" => Some(Self::Builtin("abs", "Use `abs()` builtin")),
            "__bool__" => Some(Self::Builtin("bool", "Use `bool()` builtin")),
            "__bytes__" => Some(Self::Builtin("bytes", "Use `bytes()` builtin")),
            "__complex__" => Some(Self::Builtin("complex", "Use `complex()` builtin")),
            "__dir__" => Some(Self::Builtin("dir", "Use `dir()` builtin")),
            "__float__" => Some(Self::Builtin("float", "Use `float()` builtin")),
            "__hash__" => Some(Self::Builtin("hash", "Use `hash()` builtin")),
            "__int__" => Some(Self::Builtin("int", "Use `int()` builtin")),
            "__iter__" => Some(Self::Builtin("iter", "Use `iter()` builtin")),
            "__len__" => Some(Self::Builtin("len", "Use `len()` builtin")),
            "__next__" => Some(Self::Builtin("next", "Use `next()` builtin")),
            "__repr__" => Some(Self::Builtin("repr", "Use `repr()` builtin")),
            "__reversed__" => Some(Self::Builtin("reversed", "Use `reversed()` builtin")),
            "__round__" => Some(Self::Builtin("round", "Use `round()` builtin")),
            "__str__" => Some(Self::Builtin("str", "Use `str()` builtin")),
            "__subclasscheck__" => Some(Self::Builtin("issubclass", "Use `issubclass()` builtin")),

            "__aenter__" => Some(Self::MessageOnly("Invoke context manager directly")),
            "__ceil__" => Some(Self::MessageOnly("Use `math.ceil()` function")),
            "__copy__" => Some(Self::MessageOnly("Use `copy.copy()` function")),
            "__deepcopy__" => Some(Self::MessageOnly("Use `copy.deepcopy()` function")),
            "__del__" => Some(Self::MessageOnly("Use `del` statement")),
            "__delattr__" => Some(Self::MessageOnly("Use `del` statement")),
            "__delitem__" => Some(Self::MessageOnly("Use `del` statement")),
            "__divmod__" => Some(Self::MessageOnly("Use `divmod()` builtin")),
            "__format__" => Some(Self::MessageOnly(
                "Use `format` builtin, format string method, or f-string",
            )),
            "__fspath__" => Some(Self::MessageOnly("Use `os.fspath` function")),
            "__getattr__" => Some(Self::MessageOnly(
                "Access attribute directly or use getattr built-in function",
            )),
            "__getattribute__" => Some(Self::MessageOnly(
                "Access attribute directly or use getattr built-in function",
            )),
            "__getitem__" => Some(Self::MessageOnly("Access item via subscript")),
            "__init__" => Some(Self::MessageOnly("Instantiate class directly")),
            "__instancecheck__" => Some(Self::MessageOnly("Use `isinstance()` builtin")),
            "__invert__" => Some(Self::MessageOnly("Use `~` operator")),
            "__neg__" => Some(Self::MessageOnly("Multiply by -1 instead")),
            "__pos__" => Some(Self::MessageOnly("Multiply by +1 instead")),
            "__pow__" => Some(Self::MessageOnly("Use ** operator or `pow()` builtin")),
            "__rdivmod__" => Some(Self::MessageOnly("Use `divmod()` builtin")),
            "__rpow__" => Some(Self::MessageOnly("Use ** operator or `pow()` builtin")),
            "__setattr__" => Some(Self::MessageOnly(
                "Mutate attribute directly or use setattr built-in function",
            )),
            "__setitem__" => Some(Self::MessageOnly("Use subscript assignment")),
            "__truncate__" => Some(Self::MessageOnly("Use `math.trunc()` function")),

            _ => None,
        }
    }
}

/// Returns `true` if this is a dunder method that is excusable in a nested expression. Some
/// methods are otherwise unusable in lambda expressions and elsewhere, as they can only be
/// represented as
/// statements.
fn allow_nested_expression(dunder_name: &str, semantic: &SemanticModel) -> bool {
    semantic.current_expression_parent().is_some()
        && matches!(
            dunder_name,
            "__init__"
                | "__del__"
                | "__delattr__"
                | "__setitem__"
                | "__delitem__"
                | "__iadd__"
                | "__isub__"
                | "__imul__"
                | "__imatmul__"
                | "__itruediv__"
                | "__ifloordiv__"
                | "__imod__"
                | "__ipow__"
                | "__ilshift__"
                | "__irshift__"
                | "__iand__"
                | "__ixor__"
                | "__ior__"
        )
}

/// Returns `true` if the [`SemanticModel`] is currently in a dunder method definition.
fn in_dunder_method_definition(semantic: &SemanticModel) -> bool {
    semantic.current_statements().any(|statement| {
        let Stmt::FunctionDef(func_def) = statement else {
            return false;
        };
        func_def.name.starts_with("__") && func_def.name.ends_with("__")
    })
}
