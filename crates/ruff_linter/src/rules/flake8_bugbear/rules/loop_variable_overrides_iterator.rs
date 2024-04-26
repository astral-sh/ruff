use ruff_python_ast::{self as ast, Expr, ParameterWithDefault};
use rustc_hash::FxHashMap;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for loop control variables that override the loop iterable.
///
/// ## Why is this bad?
/// Loop control variables should not override the loop iterable, as this can
/// lead to confusing behavior.
///
/// Instead, use a distinct variable name for any loop control variables.
///
/// ## Example
/// ```python
/// items = [1, 2, 3]
///
/// for items in items:
///     print(items)
/// ```
///
/// Use instead:
/// ```python
/// items = [1, 2, 3]
///
/// for item in items:
///     print(item)
/// ```
///
/// ## References
/// - [Python documentation: The `for` statement](https://docs.python.org/3/reference/compound_stmts.html#the-for-statement)
#[violation]
pub struct LoopVariableOverridesIterator {
    name: String,
}

impl Violation for LoopVariableOverridesIterator {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LoopVariableOverridesIterator { name } = self;
        format!("Loop control variable `{name}` overrides iterable it iterates")
    }
}

/// B020
pub(crate) fn loop_variable_overrides_iterator(checker: &mut Checker, target: &Expr, iter: &Expr) {
    let target_names = {
        let mut target_finder = NameFinder::default();
        target_finder.visit_expr(target);
        target_finder.names
    };
    let iter_names = {
        let mut iter_finder = NameFinder::default();
        iter_finder.visit_expr(iter);
        iter_finder.names
    };

    for (name, expr) in target_names {
        if iter_names.contains_key(name) {
            checker.diagnostics.push(Diagnostic::new(
                LoopVariableOverridesIterator {
                    name: name.to_string(),
                },
                expr.range(),
            ));
        }
    }
}

#[derive(Default)]
struct NameFinder<'a> {
    names: FxHashMap<&'a str, &'a Expr>,
}

impl<'a> Visitor<'a> for NameFinder<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(ast::ExprName { id, .. }) => {
                self.names.insert(id, expr);
            }
            Expr::ListComp(ast::ExprListComp { generators, .. })
            | Expr::DictComp(ast::ExprDictComp { generators, .. })
            | Expr::SetComp(ast::ExprSetComp { generators, .. })
            | Expr::Generator(ast::ExprGenerator { generators, .. }) => {
                for comp in generators {
                    self.visit_expr(&comp.iter);
                }
            }
            Expr::Lambda(ast::ExprLambda {
                parameters,
                body,
                range: _,
            }) => {
                visitor::walk_expr(self, body);

                if let Some(parameters) = parameters {
                    for ParameterWithDefault {
                        parameter,
                        default: _,
                        range: _,
                    } in parameters.iter_non_variadic_params()
                    {
                        self.names.remove(parameter.name.as_str());
                    }
                }
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}
