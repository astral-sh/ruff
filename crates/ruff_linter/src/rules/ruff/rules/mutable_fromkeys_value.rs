use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::analyze::typing::is_mutable_expr;

use ruff_python_codegen::Generator;
use ruff_text_size::Ranged;
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for mutable objects passed as a value argument to `dict.fromkeys`.
///
/// ## Why is this bad?
/// All values in the dictionary created by the `dict.fromkeys` method
/// refer to the same instance of the provided object. If that object is
/// modified, all values are modified, which can lead to unexpected behavior.
/// For example, if the empty list (`[]`) is provided as the default value,
/// all values in the dictionary will use the same list; as such, appending to
/// any one entry will append to all entries.
///
/// Instead, use a comprehension to generate a dictionary with distinct
/// instances of the default value.
///
/// ## Example
/// ```python
/// cities = dict.fromkeys(["UK", "Poland"], [])
/// cities["UK"].append("London")
/// cities["Poland"].append("Poznan")
/// print(cities)  # {'UK': ['London', 'Poznan'], 'Poland': ['London', 'Poznan']}
/// ```
///
/// Use instead:
/// ```python
/// cities = {country: [] for country in ["UK", "Poland"]}
/// cities["UK"].append("London")
/// cities["Poland"].append("Poznan")
/// print(cities)  # {'UK': ['London'], 'Poland': ['Poznan']}
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as the edit will change the behavior of
/// the program by using a distinct object for every value in the dictionary,
/// rather than a shared mutable instance. In some cases, programs may rely on
/// the previous behavior.
///
/// ## References
/// - [Python documentation: `dict.fromkeys`](https://docs.python.org/3/library/stdtypes.html#dict.fromkeys)
#[violation]
pub struct MutableFromkeysValue;

impl Violation for MutableFromkeysValue {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not pass mutable objects as values to `dict.fromkeys`")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with comprehension".to_string())
    }
}

/// RUF024
pub(crate) fn mutable_fromkeys_value(checker: &mut Checker, call: &ast::ExprCall) {
    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = call.func.as_ref() else {
        return;
    };

    // Check that the call is to `dict.fromkeys`.
    if attr != "fromkeys" {
        return;
    }
    let semantic = checker.semantic();
    if !semantic.match_builtin_expr(value, "dict") {
        return;
    }

    // Check that the value parameter is a mutable object.
    let [keys, value] = &*call.arguments.args else {
        return;
    };
    if !is_mutable_expr(value, semantic) {
        return;
    }

    let mut diagnostic = Diagnostic::new(MutableFromkeysValue, call.range());
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        generate_dict_comprehension(keys, value, checker.generator()),
        call.range(),
    )));
    checker.diagnostics.push(diagnostic);
}

/// Format a code snippet to expression `{key: value for key in keys}`, where
/// `keys` and `value` are the parameters of `dict.fromkeys`.
fn generate_dict_comprehension(keys: &Expr, value: &Expr, generator: Generator) -> String {
    // Construct `key`.
    let key = ast::ExprName {
        id: Name::new_static("key"),
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
    };
    // Construct `key in keys`.
    let comp = ast::Comprehension {
        target: key.clone().into(),
        iter: keys.clone(),
        ifs: vec![],
        range: TextRange::default(),
        is_async: false,
    };
    // Construct the dict comprehension.
    let dict_comp = ast::ExprDictComp {
        key: Box::new(key.into()),
        value: Box::new(value.clone()),
        generators: vec![comp],
        range: TextRange::default(),
    };
    generator.expr(&dict_comp.into())
}
