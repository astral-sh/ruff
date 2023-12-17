use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};

use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::pylint::helpers::is_known_dunder_method;
use crate::settings::types::PythonVersion;

/// ## What it does
/// Checks for explicit use of dunder methods.
///
/// ## Why is this bad?
/// Dunder names are not meant to be called explicitly.
///
/// ## Example
/// ```python
/// three = (3.0).__str__()
/// twelve = "1".__add__("2")
///
///
/// def is_bigger_than_two(x: int) -> bool:
///     return x.__gt__(2)
/// ```
///
/// Use instead:
/// ```python
/// three = str(3.0)
/// twelve = "1" + "2"
///
///
/// def is_bigger_than_two(x: int) -> bool:
///     return x > 2
/// ```
///
#[violation]
pub struct UnnecessaryDunderCall {
    call: String,
    replacement: Option<String>,
}

impl Violation for UnnecessaryDunderCall {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryDunderCall { call, replacement } = self;

        if let Some(replacement) = replacement {
            format!("Unnecessary dunder call `{call}`. {replacement}",)
        } else {
            format!("Unnecessary dunder call `{call}`")
        }
    }

    fn fix_title(&self) -> Option<String> {
        let UnnecessaryDunderCall {
            replacement: title, ..
        } = self;
        title.clone()
    }
}

enum DunderMethod {
    Operator,
    ROperator,
    Builtin,
    MessageOnly, // has no replacements implemented
}

/// PLC2801
pub(crate) fn unnecessary_dunder_call(checker: &mut Checker, expr: &Expr) {
    let Expr::Call(ast::ExprCall {
        func, arguments, ..
    }) = expr
    else {
        return;
    };

    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() else {
        return;
    };

    if !is_known_dunder_method(attr) {
        return;
    }

    if allowed_dunder_constants(attr) {
        // if this is a dunder method that is allowed to be called explicitly, skip!
        return;
    }

    // ignore dunder methods used on "super"
    if let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() {
        if checker.semantic().is_builtin("super") {
            if let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() {
                if id == "super" {
                    return;
                }
            }
        }
    }

    if is_excusable_lambda_exception(checker, attr) {
        // if this is taking place within a lambda expression with a specific dunder method, skip!
        return;
    }

    if ignore_older_dunders(checker, attr) {
        // if this is an older dunder method, skip!
        return;
    }

    let mut fixed: Option<String> = None;
    let mut title: Option<String> = None;

    if let Some((replacement, message, dunder_type)) = dunder_constants(attr) {
        match (arguments.args.len(), dunder_type) {
            (0, DunderMethod::Builtin) => {
                if !checker.semantic().is_builtin(replacement) {
                    // duck out if the builtin was shadowed
                    return;
                }

                fixed = Some(format!(
                    "{}({})",
                    replacement,
                    checker.generator().expr(value)
                ));
                title = Some(message.to_string());
            }
            (1, DunderMethod::Operator) => {
                fixed = Some(format!(
                    "{} {} {}",
                    checker.generator().expr(value),
                    replacement,
                    checker.generator().expr(arguments.args.first().unwrap()),
                ));
                title = Some(message.to_string());
            }
            (1, DunderMethod::ROperator) => {
                fixed = Some(format!(
                    "{} {} {}",
                    checker.generator().expr(arguments.args.first().unwrap()),
                    replacement,
                    checker.generator().expr(value),
                ));
                title = Some(message.to_string());
            }
            _ => {}
        }
    } else if let Some((message, dunder_type)) = unimplemented_fix_dunder_constants(attr) {
        match dunder_type {
            DunderMethod::MessageOnly => {
                title = Some(message.to_string());
            }
            _ => {
                panic!("Dunder methods in the `unimplemented_fix_dunder_constants` list must have the `MessageOnly` enum!")
            }
        }
    }

    let mut diagnostic = Diagnostic::new(
        UnnecessaryDunderCall {
            call: checker.generator().expr(expr),
            replacement: title,
        },
        expr.range(),
    );

    if let Some(fixed) = fixed {
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(fixed, expr.range())));
    };

    checker.diagnostics.push(diagnostic);
}

fn allowed_dunder_constants(dunder_method: &str) -> bool {
    // these are dunder methods that are allowed to be called explicitly
    // please keep this list tidy when adding/removing entries!
    [
        "__aexit__",
        "__await__",
        "__class__",
        "__class_getitem__",
        "__dict__",
        "__doc__",
        "__exit__",
        "__getnewargs__",
        "__getnewargs_ex__",
        "__getstate__",
        "__index__",
        "__init_subclass__",
        "__missing__",
        "__module__",
        "__new__",
        "__post_init__",
        "__reduce__",
        "__reduce_ex__",
        "__set_name__",
        "__setstate__",
        "__sizeof__",
        "__subclasses__",
        "__subclasshook__",
        "__weakref__",
    ]
    .contains(&dunder_method)
}

fn dunder_constants(dunder_method: &str) -> Option<(&str, &str, DunderMethod)> {
    // (replacement, message, dunder_type)
    match dunder_method {
        "__add__" => Some(("+", "Use `+` operator.", DunderMethod::Operator)),
        "__and__" => Some(("&", "Use `&` operator.", DunderMethod::Operator)),
        "__contains__" => Some(("in", "Use `in` operator.", DunderMethod::Operator)),
        "__eq__" => Some(("==", "Use `==` operator.", DunderMethod::Operator)),
        "__floordiv__" => Some(("//", "Use `//` operator.", DunderMethod::Operator)),
        "__ge__" => Some((">=", "Use `>=` operator.", DunderMethod::Operator)),
        "__gt__" => Some((">", "Use `>` operator.", DunderMethod::Operator)),
        "__iadd__" => Some(("+=", "Use `+=` operator.", DunderMethod::Operator)),
        "__iand__" => Some(("&=", "Use `&=` operator.", DunderMethod::Operator)),
        "__ifloordiv__" => Some(("//=", "Use `//=` operator.", DunderMethod::Operator)),
        "__ilshift__" => Some(("<<=", "Use `<<=` operator.", DunderMethod::Operator)),
        "__imod__" => Some(("%=", "Use `%=` operator.", DunderMethod::Operator)),
        "__imul__" => Some(("*=", "Use `*=` operator.", DunderMethod::Operator)),
        "__ior__" => Some(("|=", "Use `|=` operator.", DunderMethod::Operator)),
        "__ipow__" => Some(("**=", "Use `**=` operator.", DunderMethod::Operator)),
        "__irshift__" => Some((">>=", "Use `>>=` operator.", DunderMethod::Operator)),
        "__isub__" => Some(("-=", "Use `-=` operator.", DunderMethod::Operator)),
        "__itruediv__" => Some(("/=", "Use `/=` operator.", DunderMethod::Operator)),
        "__ixor__" => Some(("^=", "Use `^=` operator.", DunderMethod::Operator)),
        "__le__" => Some(("<=", "Use `<=` operator.", DunderMethod::Operator)),
        "__lshift__" => Some(("<<", "Use `<<` operator.", DunderMethod::Operator)),
        "__lt__" => Some(("<", "Use `<` operator.", DunderMethod::Operator)),
        "__mod__" => Some(("%", "Use `%` operator.", DunderMethod::Operator)),
        "__mul__" => Some(("*", "Use `*` operator.", DunderMethod::Operator)),
        "__ne__" => Some(("!=", "Use `!=` operator.", DunderMethod::Operator)),
        "__or__" => Some(("|", "Use `|` operator.", DunderMethod::Operator)),
        "__rshift__" => Some((">>", "Use `>>` operator.", DunderMethod::Operator)),
        "__sub__" => Some(("-", "Use `-` operator.", DunderMethod::Operator)),
        "__truediv__" => Some(("/", "Use `/` operator.", DunderMethod::Operator)),
        "__xor__" => Some(("^", "Use `^` operator.", DunderMethod::Operator)),

        "__radd__" => Some(("+", "Use `+` operator.", DunderMethod::ROperator)),
        "__rand__" => Some(("&", "Use `&` operator.", DunderMethod::ROperator)),
        "__rfloordiv__" => Some(("//", "Use `//` operator.", DunderMethod::ROperator)),
        "__rlshift__" => Some(("<<", "Use `<<` operator.", DunderMethod::ROperator)),
        "__rmod__" => Some(("%", "Use `%` operator.", DunderMethod::ROperator)),
        "__rmul__" => Some(("*", "Use `*` operator.", DunderMethod::ROperator)),
        "__ror__" => Some(("|", "Use `|` operator.", DunderMethod::ROperator)),
        "__rrshift__" => Some((">>", "Use `>>` operator.", DunderMethod::ROperator)),
        "__rsub__" => Some(("-", "Use `-` operator.", DunderMethod::ROperator)),
        "__rtruediv__" => Some(("/", "Use `/` operator.", DunderMethod::ROperator)),
        "__rxor__" => Some(("^", "Use `^` operator.", DunderMethod::ROperator)),

        "__aiter__" => Some(("aiter", "Use `aiter()` builtin.", DunderMethod::Builtin)),
        "__anext__" => Some(("anext", "Use `anext()` builtin.", DunderMethod::Builtin)),
        "__abs__" => Some(("abs", "Use `abs()` builtin.", DunderMethod::Builtin)),
        "__bool__" => Some(("bool", "Use `bool()` builtin.", DunderMethod::Builtin)),
        "__bytes__" => Some(("bytes", "Use `bytes()` builtin.", DunderMethod::Builtin)),
        "__complex__" => Some(("complex", "Use `complex()` builtin.", DunderMethod::Builtin)),
        "__dir__" => Some(("dir", "Use `dir()` builtin.", DunderMethod::Builtin)),
        "__float__" => Some(("float", "Use `float()` builtin.", DunderMethod::Builtin)),
        "__hash__" => Some(("hash", "Use `hash()` builtin.", DunderMethod::Builtin)),
        "__int__" => Some(("int", "Use `int()` builtin.", DunderMethod::Builtin)),
        "__iter__" => Some(("iter", "Use `iter()` builtin.", DunderMethod::Builtin)),
        "__len__" => Some(("len", "Use `len()` builtin.", DunderMethod::Builtin)),
        "__next__" => Some(("next", "Use `next()` builtin.", DunderMethod::Builtin)),
        "__repr__" => Some(("repr", "Use `repr()` builtin.", DunderMethod::Builtin)),
        "__reversed__" => Some((
            "reversed",
            "Use `reversed()` builtin.",
            DunderMethod::Builtin,
        )),
        "__round__" => Some(("round", "Use `round()` builtin.", DunderMethod::Builtin)),
        "__str__" => Some(("str", "Use `str()` builtin.", DunderMethod::Builtin)),
        "__subclasscheck__" => Some((
            "issubclass",
            "Use `issubclass()` builtin.",
            DunderMethod::Builtin,
        )),

        _ => None,
    }
}

fn unimplemented_fix_dunder_constants(dunder_method: &str) -> Option<(&str, DunderMethod)> {
    // (replacement, dunder_type)
    // these are dunder methods that have no replacements implemented
    // please keep this list tidy when adding/removing entries!
    match dunder_method {
        "__aenter__" => Some(("Use `aenter()` builtin.", DunderMethod::MessageOnly)),
        "__ceil__" => Some(("Use `math.ceil()` function.", DunderMethod::MessageOnly)),
        "__copy__" => Some(("Use `copy.copy()` function.", DunderMethod::MessageOnly)),
        "__deepcopy__" => Some(("Use `copy.deepcopy()` function.", DunderMethod::MessageOnly)),
        "__del__" => Some(("Use `del` statement.", DunderMethod::MessageOnly)),
        "__delattr__" => Some(("Use `del` statement.", DunderMethod::MessageOnly)),
        "__delete__" => Some(("Use `del` statement.", DunderMethod::MessageOnly)),
        "__delitem__" => Some(("Use `del` statement.", DunderMethod::MessageOnly)),
        "__divmod__" => Some(("Use `divmod()` builtin.", DunderMethod::MessageOnly)),
        "__format__" => Some((
            "Use `format` builtin, format string method, or f-string.",
            DunderMethod::MessageOnly,
        )),
        "__fspath__" => Some(("Use `os.fspath` function.", DunderMethod::MessageOnly)),
        "__get__" => Some(("Use `get` method.", DunderMethod::MessageOnly)),
        "__getattr__" => Some((
            "Access attribute directly or use getattr built-in function.",
            DunderMethod::MessageOnly,
        )),
        "__getattribute__" => Some((
            "Access attribute directly or use getattr built-in function.",
            DunderMethod::MessageOnly,
        )),
        "__getitem__" => Some(("Access item via subscript.", DunderMethod::MessageOnly)),
        "__init__" => Some(("Instantiate class directly.", DunderMethod::MessageOnly)),
        "__instancecheck__" => Some(("Use `isinstance()` builtin.", DunderMethod::MessageOnly)),
        "__invert__" => Some(("Use `~` operator.", DunderMethod::MessageOnly)),
        "__neg__" => Some(("Multiply by -1 instead.", DunderMethod::MessageOnly)),
        "__pos__" => Some(("Multiply by +1 instead.", DunderMethod::MessageOnly)),
        "__pow__" => Some((
            "Use ** operator or `pow()` builtin.",
            DunderMethod::MessageOnly,
        )),
        "__rdivmod__" => Some(("Use `divmod()` builtin.", DunderMethod::MessageOnly)),
        "__rpow__" => Some((
            "Use ** operator or `pow()` builtin.",
            DunderMethod::MessageOnly,
        )),
        "__set__" => Some(("Use subscript assignment.", DunderMethod::MessageOnly)),
        "__setattr__" => Some((
            "Mutate attribute directly or use setattr built-in function.",
            DunderMethod::MessageOnly,
        )),
        "__setitem__" => Some(("Use subscript assignment.", DunderMethod::MessageOnly)),
        "__truncate__" => Some(("Use `math.trunc()` function.", DunderMethod::MessageOnly)),
        _ => None,
    }
}

fn is_excusable_lambda_exception(checker: &mut Checker, dunder_name: &str) -> bool {
    // if this is taking place within a lambda expression with a specific dunder method, return true!
    // some dunder method replacements are unrepresentable in lambdas.
    let is_parent_lambda = checker.semantic().current_scope().kind.is_lambda();

    if !is_parent_lambda {
        return false;
    }

    let unnecessary_dunder_call_lambda_exceptions = [
        "__init__",
        "__del__",
        "__delattr__",
        "__set__",
        "__delete__",
        "__setitem__",
        "__delitem__",
        "__iadd__",
        "__isub__",
        "__imul__",
        "__imatmul__",
        "__itruediv__",
        "__ifloordiv__",
        "__imod__",
        "__ipow__",
        "__ilshift__",
        "__irshift__",
        "__iand__",
        "__ixor__",
        "__ior__",
    ];

    unnecessary_dunder_call_lambda_exceptions.contains(&dunder_name)
}

fn ignore_older_dunders(checker: &mut Checker, dunder_name: &str) -> bool {
    if checker.settings.target_version < PythonVersion::Py310 {
        if ["__aiter__", "__anext__"].contains(&dunder_name) {
            return true;
        }
    }

    false
}
