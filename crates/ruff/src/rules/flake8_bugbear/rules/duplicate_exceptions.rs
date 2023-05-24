use itertools::Itertools;
use ruff_text_size::TextRange;
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_parser::ast::{self, Excepthandler, Expr, ExprContext, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path;
use ruff_python_ast::call_path::CallPath;

use crate::checkers::ast::Checker;
use crate::registry::{AsRule, Rule};

#[violation]
pub struct DuplicateTryBlockException {
    name: String,
}

impl Violation for DuplicateTryBlockException {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateTryBlockException { name } = self;
        format!("try-except block with duplicate exception `{name}`")
    }
}
#[violation]
pub struct DuplicateHandlerException {
    pub names: Vec<String>,
}

impl AlwaysAutofixableViolation for DuplicateHandlerException {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateHandlerException { names } = self;
        if names.len() == 1 {
            let name = &names[0];
            format!("Exception handler with duplicate exception: `{name}`")
        } else {
            let names = names.iter().map(|name| format!("`{name}`")).join(", ");
            format!("Exception handler with duplicate exceptions: {names}")
        }
    }

    fn autofix_title(&self) -> String {
        "De-duplicate exceptions".to_string()
    }
}

fn type_pattern(elts: Vec<&Expr>) -> Expr {
    ast::ExprTuple {
        elts: elts.into_iter().cloned().collect(),
        ctx: ExprContext::Load,
        range: TextRange::default(),
    }
    .into()
}

fn duplicate_handler_exceptions<'a>(
    checker: &mut Checker,
    expr: &'a Expr,
    elts: &'a [Expr],
) -> FxHashMap<CallPath<'a>, &'a Expr> {
    let mut seen: FxHashMap<CallPath, &Expr> = FxHashMap::default();
    let mut duplicates: FxHashSet<CallPath> = FxHashSet::default();
    let mut unique_elts: Vec<&Expr> = Vec::default();
    for type_ in elts {
        if let Some(call_path) = call_path::collect_call_path(type_) {
            if seen.contains_key(&call_path) {
                duplicates.insert(call_path);
            } else {
                seen.entry(call_path).or_insert(type_);
                unique_elts.push(type_);
            }
        }
    }

    if checker.enabled(Rule::DuplicateHandlerException) {
        // TODO(charlie): Handle "BaseException" and redundant exception aliases.
        if !duplicates.is_empty() {
            let mut diagnostic = Diagnostic::new(
                DuplicateHandlerException {
                    names: duplicates
                        .into_iter()
                        .map(|call_path| call_path.join("."))
                        .sorted()
                        .collect::<Vec<String>>(),
                },
                expr.range(),
            );
            if checker.patch(diagnostic.kind.rule()) {
                #[allow(deprecated)]
                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                    if unique_elts.len() == 1 {
                        checker.generator().expr(unique_elts[0])
                    } else {
                        checker.generator().expr(&type_pattern(unique_elts))
                    },
                    expr.range(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }

    seen
}

pub(crate) fn duplicate_exceptions(checker: &mut Checker, handlers: &[Excepthandler]) {
    let mut seen: FxHashSet<CallPath> = FxHashSet::default();
    let mut duplicates: FxHashMap<CallPath, Vec<&Expr>> = FxHashMap::default();
    for handler in handlers {
        let Excepthandler::ExceptHandler(ast::ExcepthandlerExceptHandler { type_: Some(type_), .. }) = handler else {
            continue;
        };
        match type_.as_ref() {
            Expr::Attribute(_) | Expr::Name(_) => {
                if let Some(call_path) = call_path::collect_call_path(type_) {
                    if seen.contains(&call_path) {
                        duplicates.entry(call_path).or_default().push(type_);
                    } else {
                        seen.insert(call_path);
                    }
                }
            }
            Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                for (name, expr) in duplicate_handler_exceptions(checker, type_, elts) {
                    if seen.contains(&name) {
                        duplicates.entry(name).or_default().push(expr);
                    } else {
                        seen.insert(name);
                    }
                }
            }
            _ => {}
        }
    }

    if checker.enabled(Rule::DuplicateTryBlockException) {
        for (name, exprs) in duplicates {
            for expr in exprs {
                checker.diagnostics.push(Diagnostic::new(
                    DuplicateTryBlockException {
                        name: name.join("."),
                    },
                    expr.range(),
                ));
            }
        }
    }
}
