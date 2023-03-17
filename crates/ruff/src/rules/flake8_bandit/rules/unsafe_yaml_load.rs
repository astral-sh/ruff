use rustpython_parser::ast::{Expr, ExprKind, Keyword};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::SimpleCallArgs;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

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
pub fn unsafe_yaml_load(checker: &mut Checker, func: &Expr, args: &[Expr], keywords: &[Keyword]) {
    if checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| call_path.as_slice() == ["yaml", "load"])
    {
        let call_args = SimpleCallArgs::new(args, keywords);
        if let Some(loader_arg) = call_args.argument("Loader", 1) {
            if !checker
                .ctx
                .resolve_call_path(loader_arg)
                .map_or(false, |call_path| {
                    call_path.as_slice() == ["yaml", "SafeLoader"]
                        || call_path.as_slice() == ["yaml", "CSafeLoader"]
                })
            {
                let loader = match &loader_arg.node {
                    ExprKind::Attribute { attr, .. } => Some(attr.to_string()),
                    ExprKind::Name { id, .. } => Some(id.to_string()),
                    _ => None,
                };
                checker.diagnostics.push(Diagnostic::new(
                    UnsafeYAMLLoad { loader },
                    Range::from(loader_arg),
                ));
            }
        } else {
            checker.diagnostics.push(Diagnostic::new(
                UnsafeYAMLLoad { loader: None },
                Range::from(func),
            ));
        }
    }
}
