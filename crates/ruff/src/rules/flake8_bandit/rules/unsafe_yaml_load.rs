use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of the `yaml.load` function.
///
/// ## Why is this bad?
/// Running the `yaml.load` function over untrusted YAML files is insecure, as
/// `yaml.load` allows for the creation of arbitrary Python objects, which can
/// then be used to execute arbitrary code.
///
/// Instead, consider using `yaml.safe_load`, which allows for the creation of
/// simple Python objects like integers and lists, but prohibits the creation of
/// more complex objects like functions and classes.
///
/// ## Example
/// ```python
/// import yaml
///
/// yaml.load(untrusted_yaml)
/// ```
///
/// Use instead:
/// ```python
/// import yaml
///
/// yaml.safe_load(untrusted_yaml)
/// ```
///
/// ## References
/// - [PyYAML documentation: Loading YAML](https://pyyaml.org/wiki/PyYAMLDocumentation)
/// - [Common Weakness Enumeration: CWE-20](https://cwe.mitre.org/data/definitions/20.html)
#[violation]
pub struct UnsafeYAMLLoad {
    pub loader: Option<String>,
}

impl Violation for UnsafeYAMLLoad {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnsafeYAMLLoad { loader } = self;
        match loader {
            Some(name) => {
                format!(
                    "Probable use of unsafe loader `{name}` with `yaml.load`. Allows \
                     instantiation of arbitrary objects. Consider `yaml.safe_load`."
                )
            }
            None => format!(
                "Probable use of unsafe `yaml.load`. Allows instantiation of arbitrary objects. \
                 Consider `yaml.safe_load`."
            ),
        }
    }
}

/// S506
pub(crate) fn unsafe_yaml_load(checker: &mut Checker, call: &ast::ExprCall) {
    if checker
        .semantic()
        .resolve_call_path(&call.func)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["yaml", "load"]))
    {
        if let Some(loader_arg) = call.arguments.find_argument("Loader", 1) {
            if !checker
                .semantic()
                .resolve_call_path(loader_arg)
                .is_some_and(|call_path| {
                    matches!(call_path.as_slice(), ["yaml", "SafeLoader" | "CSafeLoader"])
                })
            {
                let loader = match loader_arg {
                    Expr::Attribute(ast::ExprAttribute { attr, .. }) => Some(attr.to_string()),
                    Expr::Name(ast::ExprName { id, .. }) => Some(id.to_string()),
                    _ => None,
                };
                checker.diagnostics.push(Diagnostic::new(
                    UnsafeYAMLLoad { loader },
                    loader_arg.range(),
                ));
            }
        } else {
            checker.diagnostics.push(Diagnostic::new(
                UnsafeYAMLLoad { loader: None },
                call.func.range(),
            ));
        }
    }
}
