use crate::rules::ssort::bindings::{expr_bindings, stmt_bindings};
use crate::rules::ssort::builtins::CLASS_BUILTINS;
use ruff_python_ast::prelude::*;
use ruff_python_ast::visitor::{walk_expr, walk_stmt, Visitor};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct Requirement<'a> {
    name: &'a str,
    deferred: bool,
    scope: RequirementScope,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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

#[derive(Default)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rustpython_parser::lexer::lex;
    use rustpython_parser::{parse_tokens, Mode};
    use unindent::unindent;

    #[test]
    fn function_def() {
        let stmt = parse(
            r#"
                @a
                def b(c: d = e, /, f: g = h, *i: j, k: l = m, **n: o) -> p:
                    q = r
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(
            requirements,
            [
                Requirement {
                    name: "a",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "e",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "h",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "m",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "d",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "g",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "j",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "l",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "o",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "p",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "r",
                    deferred: true,
                    scope: RequirementScope::Local
                },
            ]
        );
    }

    #[test]
    fn function_def_with_walrus() {
        let stmt = parse(
            r#"
                @(a := b)
                def c(
                    d: (e := f) = (g := h),
                    /,
                    i: (j := k) = (l := m),
                    *n: (o := p),
                    q: (r := s) = (t := u),
                    **v: (w := x)
                ) -> (y := z):
                    aa = ab
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(
            requirements,
            [
                Requirement {
                    name: "b",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "h",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "m",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "u",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "f",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "k",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "p",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "s",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "x",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "z",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "ab",
                    deferred: true,
                    scope: RequirementScope::Local
                },
            ]
        );
    }

    #[test]
    fn async_function_def() {
        let stmt = parse(
            r#"
                @a
                async def b(c: d = e, /, f: g = h, *i: j, k: l = m, **n: o) -> p:
                    q = r
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(
            requirements,
            [
                Requirement {
                    name: "a",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "e",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "h",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "m",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "d",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "g",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "j",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "l",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "o",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "p",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "r",
                    deferred: true,
                    scope: RequirementScope::Local
                },
            ]
        );
    }

    #[test]
    fn async_function_def_with_walrus() {
        let stmt = parse(
            r#"
                @(a := b)
                async def c(
                    d: (e := f) = (g := h),
                    /,
                    i: (j := k) = (l := m),
                    *n: (o := p),
                    q: (r := s) = (t := u),
                    **v: (w := x)
                ) -> (y := z):
                    aa = ab
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(
            requirements,
            [
                Requirement {
                    name: "b",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "h",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "m",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "u",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "f",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "k",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "p",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "s",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "x",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "z",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "ab",
                    deferred: true,
                    scope: RequirementScope::Local
                },
            ]
        );
    }

    #[test]
    fn class_def() {
        let stmt = parse(
            r#"
                @a
                class b(c, d=f):
                    g = h
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(
            requirements,
            [
                Requirement {
                    name: "a",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "c",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "f",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "h",
                    deferred: false,
                    scope: RequirementScope::Local
                }
            ]
        );
    }

    #[test]
    fn class_def_with_walrus() {
        let stmt = parse(
            r#"
                @(a := b)
                class c((d := e), f=(g := h)):
                    i = j
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(
            requirements,
            [
                Requirement {
                    name: "b",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "e",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "h",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "j",
                    deferred: false,
                    scope: RequirementScope::Local
                }
            ]
        );
    }

    #[test]
    fn return_() {
        let stmt = parse(r#"return a"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(
            requirements,
            [Requirement {
                name: "a",
                deferred: false,
                scope: RequirementScope::Local
            }]
        );
    }

    #[test]
    fn return_with_walrus() {
        let stmt = parse(r#"return (a := b)"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(
            requirements,
            [Requirement {
                name: "b",
                deferred: false,
                scope: RequirementScope::Local
            }]
        );
    }

    #[test]
    fn delete() {
        let stmt = parse(r#"del a, b.c, d[e:f:g]"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(
            requirements,
            [
                Requirement {
                    name: "a",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "b",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "d",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "e",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "f",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "g",
                    deferred: false,
                    scope: RequirementScope::Local
                }
            ]
        );
    }

    #[test]
    fn delete_with_walrus() {
        let stmt = parse(r#"del a, (b := c).d, (e := f)[(g := h) : (i := j) : (k := l)]"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(
            requirements,
            [
                Requirement {
                    name: "a",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "c",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "f",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "h",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "j",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "l",
                    deferred: false,
                    scope: RequirementScope::Local
                }
            ]
        );
    }

    #[test]
    fn assign() {
        let stmt = parse(r#"a = b, c.d, [e, *f], *g = h"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(
            requirements,
            [
                Requirement {
                    name: "h",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "c",
                    deferred: false,
                    scope: RequirementScope::Local
                }
            ]
        );
    }

    #[test]
    fn assign_with_walrus() {
        let stmt = parse(r#"a = b, (c := d).e, [f, *g], *h = (i := j)"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(
            requirements,
            [
                Requirement {
                    name: "j",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "d",
                    deferred: false,
                    scope: RequirementScope::Local
                }
            ]
        );
    }

    #[test]
    fn aug_assign() {
        let stmt = parse(r#"a += b"#);
        println!("{:?}", stmt);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(
            requirements,
            [
                Requirement {
                    name: "b",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "a",
                    deferred: false,
                    scope: RequirementScope::Local
                }
            ]
        );
    }

    #[test]
    fn aug_assign_with_walrus() {
        let stmt = parse(r#"a += (b := c)"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(
            requirements,
            [
                Requirement {
                    name: "c",
                    deferred: false,
                    scope: RequirementScope::Local
                },
                Requirement {
                    name: "a",
                    deferred: false,
                    scope: RequirementScope::Local
                }
            ]
        );
    }

    #[test]
    fn ann_assign() {
        let stmt = parse(r#"a: b = c"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn ann_assign_with_walrus() {
        let stmt = parse(r#"a: (b := c) = (d := e)"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn for_() {
        let stmt = parse(
            r#"
                for a in b:
                    c = d
                else:
                    e = f
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn for_with_walrus() {
        let stmt = parse(
            r#"
                for a in (b := c):
                    d = e
                else:
                    f = g
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn async_for() {
        let stmt = parse(
            r#"
                async for a in b:
                    c = d
                else:
                    e = f
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn async_for_with_walrus() {
        let stmt = parse(
            r#"
                async for a in (b := c):
                    d = e
                else:
                    f = g
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn while_() {
        let stmt = parse(
            r#"
                while a:
                    b = c
                else:
                    d = e
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn while_with_walrus() {
        let stmt = parse(
            r#"
                while (a := b):
                    c = d
                else:
                    e = f
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn if_() {
        let stmt = parse(
            r#"
                if a:
                    b = c
                elif d:
                    e = f
                else:
                    g = h
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn if_with_walrus() {
        let stmt = parse(
            r#"
                if (a := b):
                    c = d
                elif (e := f):
                    g = h
                else:
                    i = j
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn with() {
        let stmt = parse(
            r#"
                with a as b, c as (d, e):
                    f = g
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn with_with_walrus() {
        let stmt = parse(
            r#"
                with (a := b) as c, (d := e) as (f, g):
                    h = i
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn async_with() {
        let stmt = parse(
            r#"
                async with a as b, c as (d, e):
                    f = g
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn async_with_with_walrus() {
        let stmt = parse(
            r#"
                async with (a := b) as c, (d := e) as (f, g):
                    h = i
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn raise() {
        let stmt = parse(r#"raise a from b"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn raise_with_walrus() {
        let stmt = parse(r#"raise (a := b) from (c := d)"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn try_() {
        let stmt = parse(
            r#"
                try:
                    a = b
                except c as d:
                    e = f
                else:
                    g = h
                finally:
                    i = j
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn try_with_walrus() {
        let stmt = parse(
            r#"
                try:
                    a = b
                except (c := d) as e:
                    f = g
                else:
                    h = i
                finally:
                    j = k
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn try_star() {
        let stmt = parse(
            r#"
                try:
                    a = b
                except* c as d:
                    e = f
                else:
                    g = h
                finally:
                    i = j
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn try_star_with_walrus() {
        let stmt = parse(
            r#"
                try:
                    a = b
                except* (c := d) as e:
                    f = g
                else:
                    h = i
                finally:
                    j = k
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn assert() {
        let stmt = parse(r#"assert a, b"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn assert_with_walrus() {
        let stmt = parse(r#"assert (a := b), (c := d)"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn import() {
        let stmt = parse(r#"import a"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn import_with_submodule() {
        let stmt = parse(r#"import a.b.c"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn import_with_alias() {
        let stmt = parse(r#"import a as b"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn import_from() {
        let stmt = parse(r#"from a import b, c"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn import_from_with_alias() {
        let stmt = parse(r#"from a import b as c, d as e"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn global() {
        let stmt = parse(r#"global a, b"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn nonlocal() {
        let stmt = parse(r#"nonlocal a, b"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn pass() {
        let stmt = parse(r#"pass"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn break_() {
        let stmt = parse(r#"break"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn continue_() {
        let stmt = parse(r#"continue"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn bool_op() {
        let stmt = parse(r#"a and b"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn bool_op_with_walrus() {
        let stmt = parse(r#"(a := b) and (c := d)"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn named_expr() {
        let stmt = parse(r#"(a := b)"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn bin_op() {
        let stmt = parse(r#"a + b"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn bin_op_with_walrus() {
        let stmt = parse(r#"(a := b) + (c := d)"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn unary_op() {
        let stmt = parse(r#"-a"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn unary_op_with_walrus() {
        let stmt = parse(r#"-(a := b)"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn lambda() {
        let stmt = parse(r#"lambda a = b, /, c = d, *e, f = g, **h: i"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn lambda_with_walrus() {
        let stmt =
            parse(r#"lambda a = (b := c), /, d = (e := f), *g, h = (i := j), **k: (l := m)"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn if_exp() {
        let stmt = parse(r#"a if b else c"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn if_exp_with_walrus() {
        let stmt = parse(r#"(a := b) if (c := d) else (e := f)"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn dict() {
        let stmt = parse(r#"{a: b, **c}"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn dict_with_walrus() {
        let stmt = parse(r#"{(a := b): (c := d), **(e := f)}"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn set() {
        let stmt = parse(r#"{a, *b}"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn set_with_walrus() {
        let stmt = parse(r#"{(a := b), *(c := d)}"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn list_comp() {
        let stmt = parse(r#"[a for b in c if d]"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn list_comp_with_walrus() {
        let stmt = parse(r#"[(a := b) for c in (d := f) if (g := h)]"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn set_comp() {
        let stmt = parse(r#"{a for b in c if d}"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn set_comp_with_walrus() {
        let stmt = parse(r#"{(a := b) for c in (d := f) if (g := h)}"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn dict_comp() {
        let stmt = parse(r#"{a: b for c in d if e}"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn dict_comp_with_walrus() {
        let stmt = parse(r#"{(a := b): (c := d) for e in (f := g) if (h := i)}"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn generator_exp() {
        let stmt = parse(r#"(a for b in c if d)"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn generator_exp_with_walrus() {
        let stmt = parse(r#"((a := b) for c in (d := f) if (g := h))"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn await_() {
        let stmt = parse(r#"await a"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn await_with_walrus() {
        let stmt = parse(r#"await (a := b)"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn yield_() {
        let stmt = parse(r#"yield a"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn yield_with_walrus() {
        let stmt = parse(r#"yield (a := b)"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn yield_from() {
        let stmt = parse(r#"yield from a"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn yield_from_with_walrus() {
        let stmt = parse(r#"yield from (a := b)"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn compare() {
        let stmt = parse(r#"a < b < c"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn compare_with_walrus() {
        let stmt = parse(r#"(a := b) < (c := d) < (e := f)"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn call() {
        let stmt = parse(r#"a(b, *c, d=e, **f)"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn call_with_walrus() {
        let stmt = parse(r#"(a := b)((c := d), *(e := f), g=(h := i), **(j := k))"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn formatted_value() {
        let stmt = parse(r#"f"{a:b}""#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn formatted_value_with_walrus() {
        let stmt = parse(r#"f"{(a := b):{(c := d)}}""#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn joined_str() {
        let stmt = parse(r#"f"{a:b} {c:d}""#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn joined_str_with_walrus() {
        let stmt = parse(r#"f"{(a := b):{(c := d)}} {(e := f):{(g := h)}}""#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn constant() {
        let stmt = parse(r#"1"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn attribute() {
        let stmt = parse(r#"a.b.c"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn attribute_with_walrus() {
        let stmt = parse(r#"(a := b).c.d"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn subscript() {
        let stmt = parse(r#"a[b:c:d]"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn subscript_with_walrus() {
        let stmt = parse(r#"(a := b)[(c := d):(e := f):(g := h)]"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn starred() {
        let stmt = parse(r#"*a"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn starred_with_walrus() {
        let stmt = parse(r#"*(a := b)"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn name() {
        let stmt = parse(r#"a"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn list() {
        let stmt = parse(r#"[a, b, *c]"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn list_with_walrus() {
        let stmt = parse(r#"[(a := b), (c := d), *(e := f)]"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn tuple() {
        let stmt = parse(r#"(a, b, *c)"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn tuple_with_walrus() {
        let stmt = parse(r#"((a := b), (c := d), *(e := f))"#);
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn match_value() {
        let stmt = parse(
            r#"
                match a:
                    case "" as b if c:
                        d = e
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn match_singleton() {
        let stmt = parse(
            r#"
                match a:
                    case None as b if c:
                        d = e
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn match_sequence() {
        let stmt = parse(
            r#"
                match a:
                    case [b, *c, _] as e if f:
                        g = h
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn match_mapping() {
        let stmt = parse(
            r#"
                match a:
                    case {"b": c, "d": _, **e} as f if g:
                        h = i
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn match_class() {
        let stmt = parse(
            r#"
                match a:
                    case b(c, d=e, f=_):
                        g=h
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn match_star() {
        let stmt = parse(
            r#"
                match a:
                    case [*_] as b:
                        c = d
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    #[test]
    fn match_or() {
        let stmt = parse(
            r#"
                match a:
                    case [b] | (c) as d if e:
                        f = g
            "#,
        );
        let requirements = stmt_requirements(&stmt);
        assert_eq!(requirements, [] as [Requirement; 0]);
    }

    fn parse(source: &str) -> Stmt {
        let source = unindent(source);
        let tokens = lex(&source, Mode::Module);
        let parsed = parse_tokens(tokens, Mode::Module, "test.py").unwrap();
        match parsed {
            Mod::Module(ModModule { body, .. }) => body.into_iter().next().unwrap(),
            _ => panic!("Unsupported module type"),
        }
    }
}
