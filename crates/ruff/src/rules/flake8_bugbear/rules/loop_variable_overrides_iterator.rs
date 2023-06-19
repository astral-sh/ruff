use rustc_hash::FxHashMap;
use rustpython_parser::ast::{self, ArgWithDefault, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;

use crate::checkers::ast::Checker;

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

#[derive(Default)]
struct NameFinder<'a> {
    names: FxHashMap<&'a str, &'a Expr>,
}

impl<'a, 'b> Visitor<'b> for NameFinder<'a>
where
    'b: 'a,
{
    fn visit_expr(&mut self, expr: &'b Expr) {
        match expr {
            Expr::Name(ast::ExprName { id, .. }) => {
                self.names.insert(id, expr);
            }
            Expr::ListComp(ast::ExprListComp { generators, .. })
            | Expr::DictComp(ast::ExprDictComp { generators, .. })
            | Expr::SetComp(ast::ExprSetComp { generators, .. })
            | Expr::GeneratorExp(ast::ExprGeneratorExp { generators, .. }) => {
                for comp in generators {
                    self.visit_expr(&comp.iter);
                }
            }
            Expr::Lambda(ast::ExprLambda {
                args,
                body,
                range: _,
            }) => {
                visitor::walk_expr(self, body);
                for ArgWithDefault {
                    def,
                    default: _,
                    range: _,
                } in args
                    .posonlyargs
                    .iter()
                    .chain(&args.args)
                    .chain(&args.kwonlyargs)
                {
                    self.names.remove(def.arg.as_str());
                }
            }
            _ => visitor::walk_expr(self, expr),
        }
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
