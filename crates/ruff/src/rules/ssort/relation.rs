use crate::rules::ssort::builtins::CLASS_BUILTINS;
use indexmap::IndexSet;
use ruff_python_ast::visitor::{walk_expr, walk_pattern, walk_stmt, Visitor};
use rustpython_parser::ast::*;
use std::collections::HashSet;

pub(super) fn stmt_relation(stmt: &Stmt) -> Relation {
    let mut visitor = RelationVisitor::default();
    visitor.visit_stmt(stmt);
    visitor.relation
}

#[derive(Default)]
pub(super) struct Relation<'a> {
    pub requirements: IndexSet<Requirement<'a>>,
    pub bindings: IndexSet<&'a str>,
    pub unbindings: IndexSet<&'a str>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct Requirement<'a> {
    name: &'a str,
    is_deferred: bool,
    context: RequirementContext,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum RequirementContext {
    Local,
    NonLocal,
    Global,
}

#[derive(Default)]
struct RelationVisitor<'a> {
    relation: Relation<'a>,
    is_expr_context_inverted: bool,
    is_deferred: bool,
}

impl<'a> RelationVisitor<'a> {
    fn insert_requirement(&mut self, requirement: Requirement<'a>) {
        self.relation.requirements.insert(requirement);
    }

    fn insert_binding(&mut self, binding: &'a str) {
        self.relation.bindings.insert(binding);
        self.relation.unbindings.shift_remove(binding);
    }

    fn insert_unbinding(&mut self, binding: &'a str) {
        self.relation.bindings.shift_remove(binding);
        self.relation.unbindings.insert(binding);
    }

    fn insert_deferred_requirements(
        &mut self,
        requirements: IndexSet<Requirement<'a>>,
        bindings: IndexSet<&'a str>,
        unbindings: IndexSet<&'a str>,
    ) {
        for mut requirement in requirements {
            match requirement.context {
                RequirementContext::Global => {
                    self.insert_requirement(requirement);
                }
                RequirementContext::NonLocal => {
                    requirement.context = RequirementContext::Local;
                    self.insert_requirement(requirement);
                }
                RequirementContext::Local => {
                    if !self.relation.bindings.contains(requirement.name)
                        && !bindings.contains(requirement.name)
                        && !unbindings.contains(requirement.name)
                    {
                        self.insert_requirement(requirement);
                    }
                }
            };
        }
    }
}

impl<'a> Visitor<'a> for RelationVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(StmtFunctionDef {
                name,
                args,
                body,
                decorator_list,
                returns,
                ..
            })
            | Stmt::AsyncFunctionDef(StmtAsyncFunctionDef {
                name,
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

                self.insert_binding(name);

                let requirements = std::mem::take(&mut self.relation.requirements);
                let bindings = std::mem::take(&mut self.relation.bindings);
                let unbindings = std::mem::take(&mut self.relation.unbindings);
                let is_deferred = std::mem::replace(&mut self.is_deferred, true);

                add_arguments_to_bindings(&mut self.relation.bindings, args);

                self.visit_body(body);

                let requirements = std::mem::replace(&mut self.relation.requirements, requirements);
                let bindings = std::mem::replace(&mut self.relation.bindings, bindings);
                let unbindings = std::mem::replace(&mut self.relation.unbindings, unbindings);
                self.is_deferred = is_deferred;

                self.insert_deferred_requirements(requirements, bindings, unbindings);
            }
            Stmt::ClassDef(StmtClassDef {
                name,
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

                self.insert_binding(name);

                let mut cumulative_bindings = HashSet::new();

                for stmt in body {
                    let requirements = std::mem::take(&mut self.relation.requirements);
                    let bindings = std::mem::take(&mut self.relation.bindings);
                    let unbindings = std::mem::take(&mut self.relation.unbindings);

                    self.visit_stmt(stmt);

                    let requirements =
                        std::mem::replace(&mut self.relation.requirements, requirements);
                    let bindings = std::mem::replace(&mut self.relation.bindings, bindings);
                    let unbindings = std::mem::replace(&mut self.relation.unbindings, unbindings);

                    if !unbindings.is_empty() {
                        for unbinding in unbindings {
                            cumulative_bindings.remove(unbinding);
                        }
                    }

                    for requirement in requirements {
                        if requirement.is_deferred
                            || (!self.relation.bindings.contains(requirement.name)
                                && !cumulative_bindings.contains(requirement.name)
                                && !CLASS_BUILTINS.contains(&requirement.name))
                        {
                            self.insert_requirement(requirement);
                        }
                    }

                    cumulative_bindings.extend(bindings);
                }
            }
            Stmt::AugAssign(StmtAugAssign {
                target, op, value, ..
            }) => {
                let is_expr_context_inverted =
                    std::mem::replace(&mut self.is_expr_context_inverted, true);
                self.visit_expr(target);
                self.is_expr_context_inverted = is_expr_context_inverted;
                self.visit_expr(value);
                self.visit_operator(op);
                self.visit_expr(target);
            }
            Stmt::If(StmtIf {
                test, body, orelse, ..
            }) => {
                self.visit_expr(test);

                let requirements = std::mem::take(&mut self.relation.requirements);
                let bindings = std::mem::take(&mut self.relation.bindings);

                self.visit_body(body);

                let requirements = std::mem::replace(&mut self.relation.requirements, requirements);
                let body_bindings = std::mem::replace(&mut self.relation.bindings, bindings);

                for requirement in requirements {
                    if !self.relation.bindings.contains(requirement.name) {
                        self.insert_requirement(requirement);
                    }
                }

                let requirements = std::mem::take(&mut self.relation.requirements);
                let bindings = std::mem::take(&mut self.relation.bindings);

                self.visit_body(orelse);

                let requirements = std::mem::replace(&mut self.relation.requirements, requirements);
                let orelse_bindings = std::mem::replace(&mut self.relation.bindings, bindings);

                for requirement in requirements {
                    if !self.relation.bindings.contains(requirement.name) {
                        self.insert_requirement(requirement);
                    }
                }

                self.relation.bindings.extend(body_bindings);
                self.relation.bindings.extend(orelse_bindings);
            }
            Stmt::Try(StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            })
            | Stmt::TryStar(StmtTryStar {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
                self.visit_body(body);

                let mut except_handler_bindings = vec![];
                for except_handler in handlers {
                    match except_handler {
                        ExceptHandler::ExceptHandler(ExceptHandlerExceptHandler {
                            type_,
                            name,
                            body,
                            ..
                        }) => {
                            if let Some(expr) = type_ {
                                self.visit_expr(expr);
                            }

                            let requirements = std::mem::take(&mut self.relation.requirements);
                            let bindings = std::mem::take(&mut self.relation.bindings);

                            self.visit_body(body);

                            let requirements =
                                std::mem::replace(&mut self.relation.requirements, requirements);
                            let mut bindings =
                                std::mem::replace(&mut self.relation.bindings, bindings);

                            if let Some(name) = name {
                                for requirement in requirements {
                                    if requirement.name != name.as_str()
                                        && !self.relation.bindings.contains(requirement.name)
                                    {
                                        self.insert_requirement(requirement);
                                    }
                                }
                                bindings.shift_remove(name.as_str());
                            } else {
                                for requirement in requirements {
                                    if !self.relation.bindings.contains(requirement.name) {
                                        self.insert_requirement(requirement);
                                    }
                                }
                            }

                            except_handler_bindings.push(bindings);
                        }
                    }
                }

                let requirements = std::mem::take(&mut self.relation.requirements);
                let bindings = std::mem::take(&mut self.relation.bindings);

                self.visit_body(orelse);

                let requirements = std::mem::replace(&mut self.relation.requirements, requirements);
                let orelse_bindings = std::mem::replace(&mut self.relation.bindings, bindings);

                for requirement in requirements {
                    if !self.relation.bindings.contains(requirement.name) {
                        self.insert_requirement(requirement);
                    }
                }

                for bindings in except_handler_bindings {
                    self.relation.bindings.extend(bindings);
                }
                self.relation.bindings.extend(orelse_bindings);

                self.visit_body(finalbody);
            }
            Stmt::Global(StmtGlobal { names, .. }) => {
                for name in names {
                    self.insert_requirement(Requirement {
                        name,
                        is_deferred: self.is_deferred,
                        context: RequirementContext::Global,
                    });
                    self.insert_binding(name);
                }
            }
            Stmt::Nonlocal(StmtNonlocal { names, .. }) => {
                for name in names {
                    self.insert_requirement(Requirement {
                        name,
                        is_deferred: self.is_deferred,
                        context: RequirementContext::NonLocal,
                    });
                    self.insert_binding(name);
                }
            }
            stmt => walk_stmt(self, stmt),
        };
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Lambda(ExprLambda { args, body, .. }) => {
                self.visit_arguments(args);

                let requirements = std::mem::take(&mut self.relation.requirements);
                let bindings = std::mem::take(&mut self.relation.bindings);
                let unbindings = std::mem::take(&mut self.relation.unbindings);
                let is_deferred = std::mem::replace(&mut self.is_deferred, true);

                add_arguments_to_bindings(&mut self.relation.bindings, args);

                self.visit_expr(body);

                let requirements = std::mem::replace(&mut self.relation.requirements, requirements);
                let bindings = std::mem::replace(&mut self.relation.bindings, bindings);
                let unbindings = std::mem::replace(&mut self.relation.unbindings, unbindings);
                self.is_deferred = is_deferred;

                self.insert_deferred_requirements(requirements, bindings, unbindings);
            }
            Expr::IfExp(ExprIfExp {
                test, body, orelse, ..
            }) => {
                self.visit_expr(test);

                let requirements = std::mem::take(&mut self.relation.requirements);
                let bindings = std::mem::take(&mut self.relation.bindings);

                self.visit_expr(body);

                let requirements = std::mem::replace(&mut self.relation.requirements, requirements);
                let body_bindings = std::mem::replace(&mut self.relation.bindings, bindings);

                for requirement in requirements {
                    if !self.relation.bindings.contains(requirement.name) {
                        self.insert_requirement(requirement);
                    }
                }

                let requirements = std::mem::take(&mut self.relation.requirements);
                let bindings = std::mem::take(&mut self.relation.bindings);

                self.visit_expr(orelse);

                let requirements = std::mem::replace(&mut self.relation.requirements, requirements);
                let orelse_bindings = std::mem::replace(&mut self.relation.bindings, bindings);

                for requirement in requirements {
                    if !self.relation.bindings.contains(requirement.name) {
                        self.insert_requirement(requirement);
                    }
                }

                self.relation.bindings.extend(body_bindings);
                self.relation.bindings.extend(orelse_bindings);
            }
            Expr::ListComp(ExprListComp {
                elt, generators, ..
            })
            | Expr::SetComp(ExprSetComp {
                elt, generators, ..
            })
            | Expr::GeneratorExp(ExprGeneratorExp {
                elt, generators, ..
            }) => {
                for comprehension in generators {
                    self.visit_expr(&comprehension.iter);
                }

                let requirements = std::mem::take(&mut self.relation.requirements);
                let mut bindings = IndexSet::new();

                for comprehension in generators {
                    bindings = std::mem::replace(&mut self.relation.bindings, bindings);
                    self.visit_expr(&comprehension.target);
                    bindings = std::mem::replace(&mut self.relation.bindings, bindings);

                    for expr in &comprehension.ifs {
                        self.visit_expr(expr);
                    }
                }

                self.visit_expr(elt);

                let requirements = std::mem::replace(&mut self.relation.requirements, requirements);

                for requirement in requirements {
                    if !self.relation.bindings.contains(requirement.name)
                        && !bindings.contains(requirement.name)
                    {
                        self.insert_requirement(requirement);
                    }
                }
            }
            Expr::DictComp(ExprDictComp {
                key,
                value,
                generators,
                ..
            }) => {
                for comprehension in generators {
                    self.visit_expr(&comprehension.iter);
                }

                let requirements = std::mem::take(&mut self.relation.requirements);
                let mut bindings = IndexSet::new();

                for comprehension in generators {
                    bindings = std::mem::replace(&mut self.relation.bindings, bindings);
                    self.visit_expr(&comprehension.target);
                    bindings = std::mem::replace(&mut self.relation.bindings, bindings);

                    for expr in &comprehension.ifs {
                        self.visit_expr(expr);
                    }
                }

                self.visit_expr(key);
                self.visit_expr(value);

                let requirements = std::mem::replace(&mut self.relation.requirements, requirements);

                for requirement in requirements {
                    if !self.relation.bindings.contains(requirement.name)
                        && !bindings.contains(requirement.name)
                    {
                        self.insert_requirement(requirement);
                    }
                }
            }
            Expr::Name(ExprName { id, ctx, .. }) => {
                let ctx = if self.is_expr_context_inverted {
                    match ctx {
                        ExprContext::Load => ExprContext::Store,
                        ExprContext::Store => ExprContext::Load,
                        ExprContext::Del => ExprContext::Del,
                    }
                } else {
                    *ctx
                };

                match ctx {
                    ExprContext::Load => {
                        if !self.relation.bindings.contains(id.as_str()) {
                            self.insert_requirement(Requirement {
                                name: id,
                                is_deferred: self.is_deferred,
                                context: RequirementContext::Local,
                            });
                        }
                    }
                    ExprContext::Store => {
                        self.insert_binding(id);
                    }
                    ExprContext::Del => {
                        if !self.relation.bindings.contains(id.as_str()) {
                            self.insert_requirement(Requirement {
                                name: id,
                                is_deferred: self.is_deferred,
                                context: RequirementContext::Local,
                            });
                        }
                        self.insert_unbinding(id.as_str());
                    }
                }
            }
            expr => walk_expr(self, expr),
        };
    }

    fn visit_alias(&mut self, alias: &'a Alias) {
        match &alias.asname {
            Some(asname) => self.insert_binding(asname),
            None => match alias.name.split_once('.') {
                Some((prefix, _)) => self.insert_binding(prefix),
                _ => self.insert_binding(&alias.name),
            },
        };
    }

    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        match pattern {
            Pattern::MatchStar(PatternMatchStar { name, .. }) => {
                if let Some(name) = name {
                    self.insert_binding(name);
                }
            }
            Pattern::MatchMapping(PatternMatchMapping {
                keys,
                patterns,
                rest,
                ..
            }) => {
                for (key, pattern) in keys.iter().zip(patterns) {
                    self.visit_expr(key);
                    self.visit_pattern(pattern);
                }
                if let Some(rest) = rest {
                    self.insert_binding(rest);
                }
            }
            Pattern::MatchAs(PatternMatchAs { pattern, name, .. }) => {
                if let Some(pattern) = pattern {
                    self.visit_pattern(pattern);
                }
                if let Some(name) = name {
                    self.insert_binding(name);
                }
            }
            pattern => walk_pattern(self, pattern),
        };
    }
}

fn add_arguments_to_bindings<'a>(bindings: &mut IndexSet<&'a str>, arguments: &'a Arguments) {
    bindings.extend(arguments.posonlyargs.iter().map(|arg| arg.def.arg.as_str()));
    bindings.extend(arguments.args.iter().map(|arg| arg.def.arg.as_str()));
    bindings.extend(arguments.vararg.iter().map(|arg| arg.arg.as_str()));
    bindings.extend(arguments.kwonlyargs.iter().map(|arg| arg.def.arg.as_str()));
    bindings.extend(arguments.kwarg.iter().map(|arg| arg.arg.as_str()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rustpython_parser::lexer::lex;
    use rustpython_parser::{parse_tokens, Mode};
    use unindent::unindent;

    fn parse(source: &str) -> Stmt {
        let source = unindent(source);
        let tokens = lex(&source, Mode::Module);
        let parsed = parse_tokens(tokens, Mode::Module, "test.py").unwrap();
        let ast = match parsed {
            Mod::Module(ModModule { body, .. }) => body.into_iter().next().unwrap(),
            _ => panic!("Unsupported module type"),
        };
        ast
    }

    #[test]
    fn function_def() {
        let stmt = parse(
            r#"
                @a
                def b(c: d = e, /, f: g = h, *i: j, k: l = m, **n: o) -> p:
                    q = r
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "m",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "g",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "j",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "l",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "o",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "p",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "r",
                    is_deferred: true,
                    context: RequirementContext::Local
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
        let relation = stmt_relation(&stmt);
        assert_eq!(
            Vec::from_iter(relation.bindings),
            ["a", "g", "l", "t", "e", "j", "o", "r", "w", "y", "c"]
        );
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "m",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "u",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "k",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "p",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "s",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "x",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "z",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "ab",
                    is_deferred: true,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn function_def_reference_bindings() {
        let stmt = parse(
            r#"
                @(a := b)
                def c(
                    d: (e := f)[b][e][g][l][t] = (g := h)[b][g],
                    /,
                    i: (j := k)[b][e][g][j][l][t] = (l := m)[b][g][l],
                    *n: (o := p)[b][e][g][j][l][o][t],
                    q: (r := s)[b][e][g][j][l][o][r][t] = (t := u)[b][g][l][t],
                    **v: (w := x)[b][e][g][j][l][o][r][t][w]
                ) -> (y := z)[b][e][g][j][l][o][r][t][w][y]:
                    aa = a, c, d, e, g, i, j, l, n, o, q, r, t, v, w, y
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(
            Vec::from_iter(relation.bindings),
            ["a", "g", "l", "t", "e", "j", "o", "r", "w", "y", "c"]
        );
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "m",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "u",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "k",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "p",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "s",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "x",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "z",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn function_def_del_in_body() {
        let stmt = parse(
            r#"
                def a():
                    b = 1
                    del b
                    return b
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.requirements), []);
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
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "m",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "g",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "j",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "l",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "o",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "p",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "r",
                    is_deferred: true,
                    context: RequirementContext::Local
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
        let relation = stmt_relation(&stmt);
        assert_eq!(
            Vec::from_iter(relation.bindings),
            ["a", "g", "l", "t", "e", "j", "o", "r", "w", "y", "c"]
        );
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "m",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "u",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "k",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "p",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "s",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "x",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "z",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "ab",
                    is_deferred: true,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn async_function_def_reference_bindings() {
        let stmt = parse(
            r#"
                @(a := b)
                async def c(
                    d: (e := f)[b][e][g][l][t] = (g := h)[b][g],
                    /,
                    i: (j := k)[b][e][g][j][l][t] = (l := m)[b][g][l],
                    *n: (o := p)[b][e][g][j][l][o][t],
                    q: (r := s)[b][e][g][j][l][o][r][t] = (t := u)[b][g][l][t],
                    **v: (w := x)[b][e][g][j][l][o][r][t][w]
                ) -> (y := z)[b][e][g][j][l][o][r][t][w][y]:
                    aa = a, c, d, e, g, i, j, l, n, o, q, r, t, v, w, y
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(
            Vec::from_iter(relation.bindings),
            ["a", "g", "l", "t", "e", "j", "o", "r", "w", "y", "c"]
        );
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "m",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "u",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "k",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "p",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "s",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "x",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "z",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn async_function_def_del_in_body() {
        let stmt = parse(
            r#"
                async def a():
                    b = 1
                    del b
                    return b
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.requirements), []);
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
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
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
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "d", "g", "c"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "j",
                    is_deferred: false,
                    context: RequirementContext::Local
                }
            ]
        );
    }

    #[test]
    fn class_def_reference_bindings() {
        let stmt = parse(
            r#"
                @(a := b)
                class c((d := e), f=(g := h)):
                    _ = a, c, d, g
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "d", "g", "c"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn class_def_del_in_body() {
        let stmt = parse(
            r#"
                class a:
                    b = c
                    del b
                    d = b
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn class_def_builtins() {
        let stmt = parse(
            r#"
                class a:
                    b = __module__
                    c = __qualname__
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [] as [Requirement; 0]
        );
    }

    #[test]
    fn return_() {
        let stmt = parse(r#"return a"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "a",
                is_deferred: false,
                context: RequirementContext::Local
            }]
        );
    }

    #[test]
    fn return_with_walrus() {
        let stmt = parse(r#"return (a := b)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "b",
                is_deferred: false,
                context: RequirementContext::Local
            }]
        );
    }

    #[test]
    fn return_reference_bindings() {
        let stmt = parse(r#"return (a := b), a"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "b",
                is_deferred: false,
                context: RequirementContext::Local
            }]
        );
    }

    #[test]
    fn delete() {
        let stmt = parse(r#"del a, b.c, d[e:f:g]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), ["a"]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "g",
                    is_deferred: false,
                    context: RequirementContext::Local
                }
            ]
        );
    }

    #[test]
    fn delete_with_walrus() {
        let stmt = parse(r#"del a, (b := c).d, (e := f)[(g := h) : (i := j) : (k := l)]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "e", "g", "i", "k"]);
        assert_eq!(Vec::from_iter(relation.unbindings), ["a"]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "j",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "l",
                    is_deferred: false,
                    context: RequirementContext::Local
                }
            ]
        );
    }

    #[test]
    fn delete_reference_bindings() {
        let stmt =
            parse(r#"del a, (b := c).d, (e := f)[(g := h) : (i := j) : (k := l)], b, e, g, i, k"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.unbindings),
            ["a", "b", "e", "g", "i", "k"]
        );
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "j",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "l",
                    is_deferred: false,
                    context: RequirementContext::Local
                }
            ]
        );
    }

    #[test]
    fn assign() {
        let stmt = parse(r#"a = b, c.d, [e, *f], *g = h"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "b", "e", "f", "g"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                }
            ]
        );
    }

    #[test]
    fn assign_with_walrus() {
        let stmt = parse(r#"a = b, (c := d).e, [f, *g], *h = (i := j)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(
            Vec::from_iter(relation.bindings),
            ["i", "a", "b", "c", "f", "g", "h"]
        );
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "j",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                }
            ]
        );
    }

    #[test]
    fn assign_reference_bindings() {
        let stmt = parse(
            r#"a = b, (c := d).e, [f, *g], *h = a.a = b.a = c.a = f.a = g.a = h.a = (i := j), i"#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(
            Vec::from_iter(relation.bindings),
            ["i", "a", "b", "c", "f", "g", "h"]
        );
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "j",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                }
            ]
        );
    }

    #[test]
    fn aug_assign() {
        let stmt = parse(r#"a += b"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                }
            ]
        );
    }

    #[test]
    fn aug_assign_with_walrus() {
        let stmt = parse(r#"a += (b := c)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                }
            ]
        );
    }

    #[test]
    fn aug_assign_references_bindings() {
        let stmt = parse(r#"a += (b := c)[a][b]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                }
            ]
        );
    }

    #[test]
    fn aug_assign_binds_target() {
        let stmt = parse(r#"a += (a := b)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                }
            ]
        );
    }

    #[test]
    fn ann_assign() {
        let stmt = parse(r#"a: b = c"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn ann_assign_with_walrus() {
        let stmt = parse(r#"a: (b := c) = (d := e)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["d", "b", "a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn ann_assign_references_bindings() {
        let stmt = parse(r#"a: (b := c)[b] = (d := e)[d]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["d", "b", "a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
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
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "e"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
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
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "a", "d", "f"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "g",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn for_references_bindings() {
        let stmt = parse(
            r#"
                for a in (b := c)[b]:
                    d = a, b
                else:
                    e = a, b
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "a", "d", "e"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "c",
                is_deferred: false,
                context: RequirementContext::Local
            }]
        );
    }

    #[test]
    fn for_references_bindings_in_conditional() {
        let stmt = parse(
            r#"
                for a in (b := c)[b]:
                    d = a, b, e
                else:
                    e = a, b, d
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "a", "d", "e"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                }
            ]
        );
    }

    #[test]
    fn async_for_() {
        let stmt = parse(
            r#"
                async for a in b:
                    c = d
                else:
                    e = f
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "e"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
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
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "a", "d", "f"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "g",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn async_for_references_bindings() {
        let stmt = parse(
            r#"
                async for a in (b := c)[b]:
                    d = a, b
                else:
                    e = a, b
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "a", "d", "e"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "c",
                is_deferred: false,
                context: RequirementContext::Local
            }]
        );
    }

    #[test]
    fn async_for_references_bindings_in_conditional() {
        let stmt = parse(
            r#"
                async for a in (b := c)[b]:
                    d = a, b, e
                else:
                    e = a, b, d
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "a", "d", "e"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                }
            ]
        );
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
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "d"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
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
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "e"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn while_references_bindings() {
        let stmt = parse(
            r#"
                while (a := b)[a]:
                    c = a
                else:
                    d = a
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "d"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "b",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn while_references_bindings_in_conditional() {
        let stmt = parse(
            r#"
                while (a := b)[a]:
                    c = a, d
                else:
                    d = a, c
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "d"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
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
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "e", "g"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
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
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "e", "g", "i"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "j",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn if_references_bindings() {
        let stmt = parse(
            r#"
                if (a := b)[a]:
                    c = a
                elif (e := f)[a][e]:
                    g = a, e
                else:
                    h = a, e
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "e", "g", "h"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn if_references_bindings_in_conditional() {
        let stmt = parse(
            r#"
                if a:
                    b = a
                    c = e
                    d = g
                elif b:
                    e = c
                    f = h
                else:
                    g = d
                    h = f
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(
            Vec::from_iter(relation.bindings),
            ["b", "c", "d", "e", "f", "g", "h"]
        );
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "g",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn with() {
        let stmt = parse(
            r#"
                with a as b, c as (d, e):
                    f = g
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "d", "e", "f"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "g",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn with_with_walrus() {
        let stmt = parse(
            r#"
                with (a := b) as c, (d := e) as (f, g):
                    h = i
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(
            Vec::from_iter(relation.bindings),
            ["a", "c", "d", "f", "g", "h"]
        );
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "i",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn with_references_bindings() {
        let stmt = parse(
            r#"
                with (a := b)[a] as c, (d := e)[a][c][d] as (f, g):
                    h = a, c, d, f, g
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(
            Vec::from_iter(relation.bindings),
            ["a", "c", "d", "f", "g", "h"]
        );
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn async_with() {
        let stmt = parse(
            r#"
                async with a as b, c as (d, e):
                    f = g
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "d", "e", "f"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "g",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn async_with_with_walrus() {
        let stmt = parse(
            r#"
                async with (a := b) as c, (d := e) as (f, g):
                    h = i
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(
            Vec::from_iter(relation.bindings),
            ["a", "c", "d", "f", "g", "h"]
        );
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "i",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn async_with_references_bindings() {
        let stmt = parse(
            r#"
                async with (a := b)[a] as c, (d := e)[a][c][d] as (f, g):
                    h = a, c, d, f, g
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(
            Vec::from_iter(relation.bindings),
            ["a", "c", "d", "f", "g", "h"]
        );
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn raise() {
        let stmt = parse(r#"raise a from b"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn raise_with_walrus() {
        let stmt = parse(r#"raise (a := b) from (c := d)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn raise_references_bindings() {
        let stmt = parse(r#"raise (a := b)[a] from (c := d)[a][c]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
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
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "e", "g", "i"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "j",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
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
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "f", "h", "j"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "g",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "i",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "k",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn try_references_bindings() {
        let stmt = parse(
            r#"
                try:
                    a = b
                except (c := d)[a][c] as e:
                    f = a, c, e
                else:
                    g = a, c
                finally:
                    h = a, c
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "f", "g", "h"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn try_references_bindings_in_conditional() {
        let stmt = parse(
            r#"
                try:
                    a = b
                    c = d
                    e = f
                except g as h:
                    i = a
                    j = h
                else:
                    k = c, i, h
                finally:
                    l = e, h, j, k
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(
            Vec::from_iter(relation.bindings),
            ["a", "c", "e", "i", "j", "k", "l"]
        );
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "g",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "i",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
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
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "e", "g", "i"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "j",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
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
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "f", "h", "j"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "g",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "i",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "k",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn try_star_references_bindings() {
        let stmt = parse(
            r#"
                try:
                    a = b
                except* (c := d)[a][c] as e:
                    f = a, c, e
                else:
                    g = a, c
                finally:
                    h = a, c
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "f", "g", "h"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn try_star_references_bindings_in_conditional() {
        let stmt = parse(
            r#"
                try:
                    a = b
                    c = d
                    e = f
                except* g as h:
                    i = a
                    j = h
                else:
                    k = c, i, h
                finally:
                    l = e, h, j, k
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(
            Vec::from_iter(relation.bindings),
            ["a", "c", "e", "i", "j", "k", "l"]
        );
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "g",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "i",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn assert() {
        let stmt = parse(r#"assert a, b"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn assert_with_walrus() {
        let stmt = parse(r#"assert (a := b), (c := d)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn assert_references_bindings() {
        let stmt = parse(r#"assert (a := b)[a], (c := d)[a][c]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn import() {
        let stmt = parse(r#"import a"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [] as [Requirement; 0]
        );
    }

    #[test]
    fn import_submodule() {
        let stmt = parse(r#"import a.b.c"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [] as [Requirement; 0]
        );
    }

    #[test]
    fn import_with_alias() {
        let stmt = parse(r#"import a as b"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [] as [Requirement; 0]
        );
    }

    #[test]
    fn import_from() {
        let stmt = parse(r#"from a import b, c"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "c"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [] as [Requirement; 0]
        );
    }

    #[test]
    fn import_from_with_alias() {
        let stmt = parse(r#"from a import b as c, d as e"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["c", "e"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [] as [Requirement; 0]
        );
    }

    #[test]
    fn global() {
        let stmt = parse(r#"global a, b"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "b"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Global
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Global
                },
            ]
        );
    }

    #[test]
    fn nonlocal() {
        let stmt = parse(r#"nonlocal a, b"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "b"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::NonLocal
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::NonLocal
                },
            ]
        );
    }

    #[test]
    fn pass() {
        let stmt = parse(r#"pass"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [] as [Requirement; 0]
        );
    }

    #[test]
    fn break_() {
        let stmt = parse(r#"break"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [] as [Requirement; 0]
        );
    }

    #[test]
    fn continue_() {
        let stmt = parse(r#"continue"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [] as [Requirement; 0]
        );
    }

    #[test]
    fn bool_op() {
        let stmt = parse(r#"a and b"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn bool_op_with_walrus() {
        let stmt = parse(r#"(a := b) and (c := d)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn bool_op_references_bindings() {
        let stmt = parse(r#"(a := b)[a] and (c := d)[a][c]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn named_expr() {
        let stmt = parse(r#"(a := b)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "b",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn named_expr_references_bindings() {
        let stmt = parse(r#"(a := b)[a]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "b",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn bin_op() {
        let stmt = parse(r#"a + b"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn bin_op_with_walrus() {
        let stmt = parse(r#"(a := b) + (c := d)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn bin_op_references_bindings() {
        let stmt = parse(r#"(a := b)[a] + (c := d)[a][c]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn unary_op() {
        let stmt = parse(r#"-a"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "a",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn unary_op_with_walrus() {
        let stmt = parse(r#"-(a := b)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "b",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn unary_op_references_bindings() {
        let stmt = parse(r#"-(a := b)[a]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "b",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn lambda() {
        let stmt = parse(r#"lambda a = b, /, c = d, *e, f = g, **h: i"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "g",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "i",
                    is_deferred: true,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn lambda_with_walrus() {
        let stmt =
            parse(r#"lambda a = (b := c), /, d = (e := f), *g, h = (i := j), **k: (l := m)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "e", "i"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "j",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "m",
                    is_deferred: true,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn lambda_references_bindings() {
        let stmt = parse(
            r#"
                lambda \
                    a = (b := c)[b], \
                    /, \
                    d = (e := f)[b][e], \
                    *g, \
                    h = (i := j)[b][e][i], \
                    **k \
                : \
                    (a, b, d, e, g, h, i, k)"#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "e", "i"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "j",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn if_exp() {
        let stmt = parse(r#"a if b else c"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn if_exp_with_walrus() {
        let stmt = parse(r#"(a := b) if (c := d) else (e := f)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["c", "a", "e"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn if_exp_references_bindings() {
        let stmt = parse(r#"(a := b)[a][c] if (c := d)[c] else (e := f)[c][e]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["c", "a", "e"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn if_exp_references_bindings_in_conditional() {
        let stmt = parse(r#"(a := b)[a][c][e] if (c := d)[c] else (e := f)[a][c][e]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["c", "a", "e"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn dict() {
        let stmt = parse(r#"{a: b, **c}"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn dict_with_walrus() {
        let stmt = parse(r#"{(a := b): (c := d), **(e := f)}"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "e"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn dict_references_bindings() {
        let stmt = parse(r#"{(a := b)[a]: (c := d)[a][c], **(e := f)[a][c][e]}"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "e"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn set() {
        let stmt = parse(r#"{a, *b}"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn set_with_walrus() {
        let stmt = parse(r#"{(a := b), *(c := d)}"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn set_references_bindings() {
        let stmt = parse(r#"{(a := b)[a], *(c := d)[a][c]}"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn list_comp() {
        let stmt = parse(r#"[a for b in c if d]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn list_comp_with_walrus() {
        let stmt = parse(r#"[(a := b) for c in (d := f) if (g := h)]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["d", "g", "a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn list_comp_references_bindings() {
        let stmt = parse(r#"[(a := b)[a][c][d][g] for c in (d := f)[d] if (g := h)[c][d][g]]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["d", "g", "a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn list_comp_shadows_iter() {
        let stmt = parse(r#"[a for a in a if a]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "a",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn set_comp() {
        let stmt = parse(r#"{a for b in c if d}"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn set_comp_with_walrus() {
        let stmt = parse(r#"{(a := b) for c in (d := f) if (g := h)}"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["d", "g", "a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn set_comp_references_bindings() {
        let stmt = parse(r#"{(a := b)[a][c][d][g] for c in (d := f)[d] if (g := h)[c][d][g]}"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["d", "g", "a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn set_comp_shadows_iter() {
        let stmt = parse(r#"{a for a in a if a}"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "a",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn dict_comp() {
        let stmt = parse(r#"{a: b for c in d if e}"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn dict_comp_with_walrus() {
        let stmt = parse(r#"{(a := b): (c := d) for e in (f := g) if (h := i)}"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["f", "h", "a", "c"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "g",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "i",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn dict_comp_references_bindings() {
        let stmt = parse(
            r#"
                {
                    (a := b)[a][e][f][h]: (c := d)[a][c][e][f][h]
                    for e in (f := g)[f] if (h := i)[e][f][h]
                }
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["f", "h", "a", "c"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "g",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "i",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn dict_comp_shadows_iter() {
        let stmt = parse(r#"{a: a for a in a if a}"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "a",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn generator_exp() {
        let stmt = parse(r#"(a for b in c if d)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn generator_exp_with_walrus() {
        let stmt = parse(r#"((a := b) for c in (d := f) if (g := h))"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["d", "g", "a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn generator_exp_references_bindings() {
        let stmt = parse(r#"((a := b)[a][c][d][g] for c in (d := f)[d] if (g := h)[c][d][g])"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["d", "g", "a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn generator_exp_shadows_iter() {
        let stmt = parse(r#"(a for a in a if a)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "a",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn await_() {
        let stmt = parse(r#"await a"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "a",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn await_with_walrus() {
        let stmt = parse(r#"await (a := b)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "b",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn await_references_bindings() {
        let stmt = parse(r#"await (a := b)[a]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "b",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn yield_() {
        let stmt = parse(r#"yield a"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "a",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn yield_with_walrus() {
        let stmt = parse(r#"yield (a := b)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "b",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn yield_references_bindings() {
        let stmt = parse(r#"yield (a := b)[a]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "b",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn yield_from() {
        let stmt = parse(r#"yield from a"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "a",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn yield_from_with_walrus() {
        let stmt = parse(r#"yield from (a := b)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "b",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn yield_from_references_bindings() {
        let stmt = parse(r#"yield from (a := b)[a]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "b",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn compare() {
        let stmt = parse(r#"a < b < c"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn compare_with_walrus() {
        let stmt = parse(r#"(a := b) < (c := d) < (e := f)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "e"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn compare_references_bindings() {
        let stmt = parse(r#"(a := b)[a] < (c := d)[a][c] < (e := f)[a][c][e]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "e"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn call() {
        let stmt = parse(r#"a(b, *c, d=e, **f)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn call_with_walrus() {
        let stmt = parse(r#"(a := b)((c := d), *(e := f), g=(h := i), **(j := k))"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "e", "h", "j"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "i",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "k",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn call_references_bindings() {
        let stmt = parse(
            r#"
                (a := b)[a](
                    (c := d)[a][c],
                    *(e := f)[a][c][f],
                    g=(h := i)[a][c][f][h],
                    **(j := k)[a][c][f][h][j]
                )
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "e", "h", "j"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "i",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "k",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn formatted_value() {
        let stmt = parse(r#"f"{a:{b}}""#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn formatted_value_with_walrus() {
        let stmt = parse(r#"f"{(a := b):{(c := d)}}""#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn formatted_value_references_bindings() {
        let stmt = parse(r#"f"{(a := b)[a]:{(c := d)[a][c]}}""#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn joined_str() {
        let stmt = parse(r#"f"{a:{b}} {c:{d}}""#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn joined_str_with_walrus() {
        let stmt = parse(r#"f"{(a := b):{(c := d)}} {(e := f):{(g := h)}}""#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "e", "g"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn joined_str_references_bindings() {
        let stmt = parse(
            r#"f"{(a := b)[a]:{(c := d)[a][c]}} {(e := f)[a][c][e]:{(g := h)[a][c][e][g]}}""#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "e", "g"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn constant() {
        let stmt = parse(r#"1"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [] as [Requirement; 0]
        );
    }

    #[test]
    fn attribute() {
        let stmt = parse(r#"a.b.c"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "a",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn attribute_with_walrus() {
        let stmt = parse(r#"(a := b).c.d"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "b",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn attribute_references_bindings() {
        let stmt = parse(r#"(a := b).c.d[a]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "b",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn subscript() {
        let stmt = parse(r#"a[b:c:d]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn subscript_with_walrus() {
        let stmt = parse(r#"(a := b)[(c := d):(e := f):(g := h)]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "e", "g"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn subscript_references_bindings() {
        let stmt = parse(r#"(a := b)[a][(c := d)[a][c]:(e := f)[a][c][e]:(g := h)[a][c][e][g]]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "e", "g"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn starred() {
        let stmt = parse(r#"*a"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "a",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn starred_with_walrus() {
        let stmt = parse(r#"*(a := b)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "b",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn starred_references_bindings() {
        let stmt = parse(r#"*(a := b)[a]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "b",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn name() {
        let stmt = parse(r#"a"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [Requirement {
                name: "a",
                is_deferred: false,
                context: RequirementContext::Local
            },]
        );
    }

    #[test]
    fn list() {
        let stmt = parse(r#"[a, b, *c]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn list_with_walrus() {
        let stmt = parse(r#"[(a := b), (c := d), *(e := f)]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "e"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn list_references_bindings() {
        let stmt = parse(r#"[(a := b)[a], (c := d)[a][c], *(e := f)[a][c][e]]"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "e"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn tuple() {
        let stmt = parse(r#"(a, b, *c)"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), [] as [&str; 0]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn tuple_with_walrus() {
        let stmt = parse(r#"((a := b), (c := d), *(e := f))"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "e"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn tuple_references_bindings() {
        let stmt = parse(r#"((a := b)[a], (c := d)[a][c], *(e := f)[a][c][e])"#);
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["a", "c", "e"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "d",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
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
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "d"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
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
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "d"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
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
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "c", "e", "g"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "f",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
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
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["c", "e", "f", "h"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "g",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "i",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn match_class() {
        let stmt = parse(
            r#"
                match a:
                    case b(c, d=e, f=_) as g if h:
                        i=j
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["c", "e", "g", "i"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "b",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "h",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "j",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }

    #[test]
    fn match_star() {
        let stmt = parse(
            r#"
                match a:
                    case [*_] as b if c:
                        d = e
            "#,
        );
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "d"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "c",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
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
        let relation = stmt_relation(&stmt);
        assert_eq!(Vec::from_iter(relation.bindings), ["b", "c", "d", "f"]);
        assert_eq!(Vec::from_iter(relation.unbindings), [] as [&str; 0]);
        assert_eq!(
            Vec::from_iter(relation.requirements),
            [
                Requirement {
                    name: "a",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "e",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
                Requirement {
                    name: "g",
                    is_deferred: false,
                    context: RequirementContext::Local
                },
            ]
        );
    }
}
