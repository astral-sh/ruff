use crate::rules::ssort::bindings::{expr_bindings, stmt_bindings};
use crate::rules::ssort::builtins::CLASS_BUILTINS;
use ruff_python_ast::prelude::*;
use ruff_python_ast::visitor::{walk_expr, walk_stmt, Visitor};
use std::collections::HashSet;

pub(super) struct Requirement<'a> {
    name: &'a str,
    deferred: bool,
    scope: RequirementScope,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum RequirementScope {
    Local,
    NonLocal,
    Global,
}

pub(super) fn stmt_requirements(stmt: &Stmt) -> Vec<Requirement> {
    let mut requirements = Requirements {
        requirements: vec![],
    };
    requirements.visit_stmt(stmt);
    requirements.requirements
}

struct Requirements<'a> {
    requirements: Vec<Requirement<'a>>,
}

impl<'a> Visitor<'a> for Requirements<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(StmtFunctionDef {
                args,
                body,
                decorator_list,
                returns,
                ..
            })
            | Stmt::AsyncFunctionDef(StmtAsyncFunctionDef {
                args,
                body,
                decorator_list,
                returns,
                ..
            }) => {
                for decorator in decorator_list {
                    self.visit_decorator(decorator);
                }

                self.visit_arguments(args);

                if let Some(expr) = returns {
                    self.visit_annotation(expr);
                }

                let mut scope = arguments_scope(args);
                let requirements = std::mem::take(&mut self.requirements);
                for stmt in body {
                    scope.extend(stmt_bindings(stmt));
                    self.visit_stmt(stmt);
                }
                let requirements = std::mem::replace(&mut self.requirements, requirements);

                for mut requirement in requirements {
                    match requirement.scope {
                        RequirementScope::Global => {}
                        RequirementScope::NonLocal => requirement.scope = RequirementScope::Local,
                        RequirementScope::Local => {
                            if scope.contains(requirement.name) {
                                continue;
                            }
                        }
                    }

                    requirement.deferred = true;
                    self.requirements.push(requirement);
                }
            }
            Stmt::ClassDef(StmtClassDef {
                bases,
                keywords,
                body,
                decorator_list,
                ..
            }) => {
                for decorator in decorator_list {
                    self.visit_decorator(decorator);
                }

                for expr in bases {
                    self.visit_expr(expr);
                }

                for keyword in keywords {
                    self.visit_keyword(keyword);
                }

                let mut scope: HashSet<&str> = CLASS_BUILTINS.iter().copied().collect();
                for stmt in body {
                    let requirements = std::mem::take(&mut self.requirements);
                    self.visit_stmt(stmt);
                    let requirements = std::mem::replace(&mut self.requirements, requirements);
                    for requirement in requirements {
                        if requirement.deferred || !scope.contains(requirement.name) {
                            self.requirements.push(requirement);
                        }
                    }
                    scope.extend(stmt_bindings(stmt));
                }
            }
            Stmt::For(StmtFor {
                target,
                iter,
                body,
                orelse,
                ..
            })
            | Stmt::AsyncFor(StmtAsyncFor {
                target,
                iter,
                body,
                orelse,
                ..
            }) => {
                self.visit_expr(iter);
                self.visit_expr(target);

                let requirements = std::mem::take(&mut self.requirements);
                self.visit_body(body);
                self.visit_body(orelse);
                let requirements = std::mem::replace(&mut self.requirements, requirements);

                let scope = stmt_bindings(stmt);
                for requirement in requirements {
                    if !scope.contains(&requirement.name) {
                        self.requirements.push(requirement);
                    }
                }
            }
            Stmt::With(StmtWith { items, body, .. })
            | Stmt::AsyncWith(StmtAsyncWith { items, body, .. }) => {
                for with_item in items {
                    self.visit_with_item(with_item);
                }

                let requirements = std::mem::take(&mut self.requirements);
                self.visit_body(body);
                let requirements = std::mem::replace(&mut self.requirements, requirements);

                let scope = stmt_bindings(stmt);
                for requirement in requirements {
                    if !scope.contains(&requirement.name) {
                        self.requirements.push(requirement);
                    }
                }
            }
            Stmt::Global(StmtGlobal { names, .. }) => {
                for name in names {
                    self.requirements.push(Requirement {
                        name: name.as_str(),
                        deferred: false,
                        scope: RequirementScope::Global,
                    });
                }
            }
            Stmt::Nonlocal(StmtNonlocal { names, .. }) => {
                for name in names {
                    self.requirements.push(Requirement {
                        name: name.as_str(),
                        deferred: false,
                        scope: RequirementScope::NonLocal,
                    });
                }
            }
            stmt => walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Lambda(ExprLambda { args, body, .. }) => {
                self.visit_arguments(args);

                let requirements = std::mem::take(&mut self.requirements);
                self.visit_expr(body);
                let requirements = std::mem::replace(&mut self.requirements, requirements);

                let mut scope = arguments_scope(args);
                scope.extend(expr_bindings(body));
                for requirement in requirements {
                    if !scope.contains(requirement.name) {
                        self.requirements.push(requirement);
                    }
                }
            }
            Expr::ListComp(ExprListComp { generators, .. })
            | Expr::SetComp(ExprSetComp { generators, .. })
            | Expr::DictComp(ExprDictComp { generators, .. })
            | Expr::GeneratorExp(ExprGeneratorExp { generators, .. }) => {
                let requirements = std::mem::take(&mut self.requirements);
                walk_expr(self, expr);
                let requirements = std::mem::replace(&mut self.requirements, requirements);

                let scope = comprehensions_scope(generators);
                for requirement in requirements {
                    if !scope.contains(requirement.name) {
                        self.requirements.push(requirement);
                    }
                }
            }
            Expr::Name(ExprName { id, ctx, .. }) => {
                if ctx == &ExprContext::Load || ctx == &ExprContext::Del {
                    self.requirements.push(Requirement {
                        name: id,
                        deferred: false,
                        scope: RequirementScope::Local,
                    });
                }
            }
            expr => walk_expr(self, expr),
        }
    }
}

fn arguments_scope(args: &Arguments) -> HashSet<&str> {
    let mut scope = HashSet::new();
    scope.extend(args.posonlyargs.iter().map(|arg| arg.def.arg.as_str()));
    scope.extend(args.args.iter().map(|arg| arg.def.arg.as_str()));
    scope.extend(args.vararg.iter().map(|arg| arg.arg.as_str()));
    scope.extend(args.kwonlyargs.iter().map(|arg| arg.def.arg.as_str()));
    scope.extend(args.kwarg.iter().map(|arg| arg.arg.as_str()));
    scope
}

fn comprehensions_scope(comprehensions: &[Comprehension]) -> HashSet<&str> {
    let mut scope = HashSet::new();
    for comprehension in comprehensions {
        scope.extend(expr_bindings(&comprehension.target));
    }
    scope
}
