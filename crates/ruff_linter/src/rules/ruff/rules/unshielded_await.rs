use std::ops::Deref;
use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::AwaitVisitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, visitor, Comprehension, ExceptHandler, Expr, ExprAttribute, ExprCall, ExprName, Stmt};
use ruff_python_ast::Expr::Name;
use ruff_python_ast::name::{QualifiedName, QualifiedNameBuilder};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

/// ## What it does
/// TODO
///
///
/// ## Why is this bad?
/// TODO
///
/// ## Example
/// TODO
///
/// ## References
/// TODO
#[violation]
pub struct UnshieldedAwait {
    s: String,
}

impl Violation for UnshieldedAwait {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("shield it! {}", self.s)
    }
}

/// RUF102
pub(crate) fn unshielded_await(
    checker: &mut Checker,
    type_: Option<&Expr>,
    _name: Option<&str>,
    body: &[Stmt],
) {
    let Some(type_) = type_ else {
        return;
    };

    // If there are no awaits then there is nothing to shield
    let mut visitor = AwaitVisitor { seen_await: false };
    visitor.visit_body(body);
    if !visitor.seen_await {
        return;
    }

    // checker.diagnostics.push(Diagnostic::new(
    //     UnshieldedAwait {
    //         s: format!("{type_:?}"),
    //     },
    //     type_.range(),
    // ));
}

// struct AwaitVisitor<'a> {
//     seen: bool,
// }
//
// impl<'a> AwaitVisitor<'a> {
//     fn new(name: Option<&'a str>) -> Self {
//         Self { seen: false }
//     }
//
//     /// Returns `true` if the exception was re-raised.
//     fn seen(&self) -> bool {
//         self.seen
//     }
// }
//
// impl<'a> AwaitVisitor<'a> for crate::rules::flake8_blind_except::rules::blind_except::ReraiseVisitor<'a> {
//     fn visit_stmt(&mut self, stmt: &'a Stmt) {
//         match stmt {
//             Stmt::Raise(ast::StmtRaise { exc, cause, .. }) => {
//                 if let Some(cause) = cause {
//                     if let Expr::Name(ast::ExprName { id, .. }) = cause.as_ref() {
//                         if self.name.is_some_and(|name| id == name) {
//                             self.seen = true;
//                         }
//                     }
//                 } else {
//                     if let Some(exc) = exc {
//                         if let Expr::Name(ast::ExprName { id, .. }) = exc.as_ref() {
//                             if self.name.is_some_and(|name| id == name) {
//                                 self.seen = true;
//                             }
//                         }
//                     } else {
//                         self.seen = true;
//                     }
//                 }
//             }
//             Stmt::Try(_) | Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
//             _ => walk_stmt(self, stmt),
//         }
//     }
// }
//
// /// A visitor to detect whether the exception was logged.

fn flattened_tuple<'a>(t: &'a Expr, semantic: &'a SemanticModel<'a>) -> Vec<QualifiedName<'a>> {
    let mut f = vec![];

    match t {
        Expr::Tuple(t) => {
            for e in t {
                f.append(&mut flattened_tuple(e, semantic))
            }
        }
        Expr::Name( .. ) | Expr::Attribute( .. ) => {
            if let Some(name) = semantic.resolve_qualified_name(t) {
                f.push(name);
            // } else if let Some(name2) = semantic.resolve_builtin_symbol(t) {
            //     let mut builder = QualifiedNameBuilder::default();
            //     builder.push(name2);
            //     let exception = builder.build();
            //     f.push(exception);
            } else {
                panic!("inside unable to handle {:?}", t);
            };
        },
        // Expr::Attribute( .. ) => {
        //     let Some(qualified_name) = semantic.resolve_qualified_name(t.cloned()) else {
        //         panic!("inside unable to handle {:?}", t);
        //     };
        //     // print!("{qualified_name:?}")
        //     f.push(qualified_name);
        // },
        _ => panic!("outside unable to handle {:?}", t),
    }

    f
}

/// RUF102
pub(crate) fn unshielded_await_for_try(
    checker: &mut Checker,
    handlers: &Vec<ExceptHandler>,
    body: &Vec<Stmt>,
    finalbody: &Vec<Stmt>,
) {
    // If there are no awaits then there is nothing to shield
    // let mut visitor = AwaitVisitor{seen_await: false};
    // visitor.visit_body(body);
    // if ! visitor.seen_await {
    //     return;
    // }

    for handler in handlers {
        let ExceptHandler::ExceptHandler(handler) = handler;

        let Some(t) = handler.type_.as_deref() else {
            todo!()
        };

        let mut builder = QualifiedNameBuilder::default();
        builder.push("asyncio");
        builder.push("CancelledError");
        let asyncio_cancelled_error = builder.build();

        let mut builder = QualifiedNameBuilder::default();
        builder.push("Exception");
        let exception = builder.build();

        let types = flattened_tuple(t, checker.semantic());
        // for tt in types.iter() {
        //     let e = *tt == exception;
        //     let c = *tt == asyncio_cancelled_error;
        //     let s = format!("{tt}").as_str();
        //     print!("");
        // }
        // TODO i challenge you to make it worse than this
        if types.iter().find(|tt| format!("{tt}").as_str() == format!("{exception}").as_str() || format!("{tt}").as_str() == format!("{asyncio_cancelled_error}").as_str()).is_none() {
            continue;
        }

        // If there are no awaits then there is nothing to shield
        let mut visitor = PrunedAwaitVisitor { seen_await: false };
        visitor.visit_body(&handler.body);
        if visitor.seen_await {
            checker.diagnostics.push(Diagnostic::new(
                UnshieldedAwait {
                    s: format!("{types:?}"),
                },
                handler.range,
            ));
        }
    }

    // If there are no awaits then there is nothing to shield
    let mut visitor = PrunedAwaitVisitor { seen_await: false };
    visitor.visit_body(&finalbody);
    if visitor.seen_await {
        checker.diagnostics.push(Diagnostic::new(
            UnshieldedAwait {
                s: format!(""),
            },
            // TODO yeah not sure where to get the finally range itself
            finalbody[0].range(),
        ));
    }
}


/// A [`Visitor`] that detects the presence of `await` expressions in the current scope.
#[derive(Debug, Default)]
pub struct PrunedAwaitVisitor {
    pub seen_await: bool,
}

impl Visitor<'_> for PrunedAwaitVisitor {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => (),
            Stmt::With(ast::StmtWith { is_async: true, .. }) => {
                self.seen_await = true;
            }
            Stmt::With(ast::StmtWith { is_async: false, items, .. }) => {
                for item in items {
                    // TODO resolved name...  what about x = y(); with y:?
                    if let Expr::Call(ExprCall{ref func, ..}) = item.context_expr {
                        match func.deref() {
                            Expr::Attribute(ExprAttribute{ attr, .. }) => if attr.id.as_str() == "shield" { return},
                            // Expr::Name(ExprName{ .. }) => ,
                            _ => {},
                        }
                    }
                }
                visitor::walk_stmt(self, stmt);
            }
            Stmt::For(ast::StmtFor { is_async: true, .. }) => {
                self.seen_await = true;
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        if let Expr::Await(ast::ExprAwait { .. }) = expr {
            self.seen_await = true;
        } else {
            visitor::walk_expr(self, expr);
        }
    }

    fn visit_comprehension(&mut self, comprehension: &'_ Comprehension) {
        if comprehension.is_async {
            self.seen_await = true;
        } else {
            visitor::walk_comprehension(self, comprehension);
        }
    }
}
