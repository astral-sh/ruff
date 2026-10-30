use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat;
use ruff_python_ast::flat::fields;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::preview::is_baseloader_safe_in_yaml_load_enabled;

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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.212", category = Category::Security)]
pub(crate) struct UnsafeYAMLLoad {
    pub loader: Option<String>,
}

impl Violation for UnsafeYAMLLoad {
    #[derive_message_formats]
    fn message(&self) -> String {
        match &self.loader {
            Some(name) => {
                format!(
                    "Probable use of unsafe loader `{name}` with `yaml.load`. Allows \
                     instantiation of arbitrary objects. Consider `yaml.safe_load`."
                )
            }
            None => {
                "Probable use of unsafe `yaml.load`. Allows instantiation of arbitrary objects. \
                 Consider `yaml.safe_load`."
                    .to_string()
            }
        }
    }
}

/// S506
pub(crate) fn unsafe_yaml_load(checker: &Checker, call: &fields::ExprCall<'_>) {
    if checker
        .semantic()
        .resolve_qualified_name(call.func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["yaml", "load"]))
    {
        if let Some(loader_arg) = call.arguments.find_argument_value("Loader", 1) {
            if !checker
                .semantic()
                .resolve_qualified_name(loader_arg)
                .is_some_and(|qualified_name| {
                    matches!(
                        qualified_name.segments(),
                        ["yaml", "SafeLoader" | "CSafeLoader"]
                            | ["yaml", "loader", "SafeLoader" | "CSafeLoader"]
                            | ["yaml", "cyaml", "CSafeLoader"]
                    ) || (is_baseloader_safe_in_yaml_load_enabled(checker.settings())
                        && matches!(
                            qualified_name.segments(),
                            ["yaml", "BaseLoader" | "CBaseLoader"]
                                | ["yaml", "loader", "BaseLoader" | "CBaseLoader"]
                                | ["yaml", "cyaml", "CBaseLoader"]
                        ))
                })
            {
                let loader = match loader_arg.classify() {
                    flat::ExprKind::Attribute(expr_attribute) => {
                        let attr = expr_attribute.attr();

                        Some(attr.to_string())
                    }
                    flat::ExprKind::Name(expr_name) => {
                        let id = expr_name.id();

                        Some(id.to_string())
                    }
                    _ => None,
                };
                checker.report_diagnostic(UnsafeYAMLLoad { loader }, loader_arg.range());
            }
        } else {
            checker.report_diagnostic(UnsafeYAMLLoad { loader: None }, call.func.range());
        }
    }
}
