use crate::rules::ssort::builtins::CLASS_BUILTINS;
use indexmap::IndexSet;
use ruff_python_ast::prelude::*;
use ruff_python_ast::visitor::{walk_expr, walk_pattern, walk_stmt, Visitor};

pub(super) fn stmt_relations(stmt: &Stmt) -> Relations {
    let mut visitor = RelationsVisitor::default();
    visitor.visit_stmt(stmt);
    visitor.relations
}

#[derive(Default)]
pub(super) struct Relations<'a> {
    pub requirements: IndexSet<Requirement<'a>>,
    pub bindings: IndexSet<&'a str>,
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
struct RelationsVisitor<'a> {
    relations: Relations<'a>,
    is_store_requirement: bool,
    is_deferred: bool,
}

impl<'a> Visitor<'a> for RelationsVisitor<'a> {
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

                self.relations.bindings.insert(name);

                let requirements = std::mem::take(&mut self.relations.requirements);
                let bindings = std::mem::take(&mut self.relations.bindings);
                let is_deferred = std::mem::replace(&mut self.is_deferred, true);

                add_arguments_to_bindings(&mut self.relations.bindings, args);

                for stmt in body {
                    self.visit_stmt(stmt);
                }

                let requirements =
                    std::mem::replace(&mut self.relations.requirements, requirements);
                let bindings = std::mem::replace(&mut self.relations.bindings, bindings);
                self.is_deferred = is_deferred;

                for mut requirement in requirements {
                    match requirement.context {
                        RequirementContext::Global => {
                            self.relations.requirements.insert(requirement);
                        }
                        RequirementContext::NonLocal => {
                            requirement.context = RequirementContext::Local;
                            self.relations.requirements.insert(requirement);
                        }
                        RequirementContext::Local => {
                            if !self.relations.bindings.contains(requirement.name)
                                && !bindings.contains(requirement.name)
                            {
                                self.relations.requirements.insert(requirement);
                            }
                        }
                    };
                }
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

                self.relations.bindings.insert(name);

                let mut cumulative_bindings = IndexSet::new();

                for stmt in body {
                    let requirements = std::mem::take(&mut self.relations.requirements);
                    let bindings = std::mem::take(&mut self.relations.bindings);

                    self.visit_stmt(stmt);

                    let requirements =
                        std::mem::replace(&mut self.relations.requirements, requirements);
                    let bindings = std::mem::replace(&mut self.relations.bindings, bindings);

                    for requirement in requirements {
                        if requirement.is_deferred
                            || (!self.relations.bindings.contains(requirement.name)
                                && !cumulative_bindings.contains(requirement.name))
                        {
                            self.relations.requirements.insert(requirement);
                        }
                    }
                    cumulative_bindings.extend(bindings);
                }
            }
            Stmt::AugAssign(StmtAugAssign {
                target, op, value, ..
            }) => {
                self.visit_expr(value);
                self.visit_operator(op);
                let is_store_requirement = std::mem::replace(&mut self.is_store_requirement, true);
                self.visit_expr(target);
                self.is_store_requirement = is_store_requirement;
            }
            Stmt::Global(StmtGlobal { names, .. }) => {
                for name in names {
                    self.relations.requirements.insert(Requirement {
                        name,
                        is_deferred: self.is_deferred,
                        context: RequirementContext::Global,
                    });
                    self.relations.bindings.insert(name);
                }
            }
            Stmt::Nonlocal(StmtNonlocal { names, .. }) => {
                for name in names {
                    self.relations.requirements.insert(Requirement {
                        name,
                        is_deferred: self.is_deferred,
                        context: RequirementContext::NonLocal,
                    });
                    self.relations.bindings.insert(name);
                }
            }
            stmt => walk_stmt(self, stmt),
        };
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Lambda(ExprLambda { args, body, .. }) => {
                self.visit_arguments(args);

                let requirements = std::mem::take(&mut self.relations.requirements);
                let bindings = std::mem::take(&mut self.relations.bindings);
                let is_deferred = std::mem::replace(&mut self.is_deferred, true);

                add_arguments_to_bindings(&mut self.relations.bindings, args);

                self.visit_expr(body);

                let requirements =
                    std::mem::replace(&mut self.relations.requirements, requirements);
                let bindings = std::mem::replace(&mut self.relations.bindings, bindings);
                self.is_deferred = is_deferred;

                for mut requirement in requirements {
                    match requirement.context {
                        RequirementContext::Global => {
                            self.relations.requirements.insert(requirement);
                        }
                        RequirementContext::NonLocal => {
                            requirement.context = RequirementContext::Local;
                            self.relations.requirements.insert(requirement);
                        }
                        RequirementContext::Local => {
                            if !self.relations.bindings.contains(requirement.name)
                                && !bindings.contains(requirement.name)
                            {
                                self.relations.requirements.insert(requirement);
                            }
                        }
                    };
                }
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
                let requirements = std::mem::take(&mut self.relations.requirements);
                let mut bindings = IndexSet::new();

                for comprehension in generators {
                    self.visit_expr(&comprehension.iter);

                    bindings = std::mem::replace(&mut self.relations.bindings, bindings);
                    self.visit_expr(&comprehension.target);
                    bindings = std::mem::replace(&mut self.relations.bindings, bindings);

                    for expr in &comprehension.ifs {
                        self.visit_expr(expr);
                    }
                }

                self.visit_expr(elt);

                let requirements =
                    std::mem::replace(&mut self.relations.requirements, requirements);

                for requirement in requirements {
                    if !self.relations.bindings.contains(requirement.name)
                        && !bindings.contains(requirement.name)
                    {
                        self.relations.requirements.insert(requirement);
                    }
                }
            }
            Expr::DictComp(ExprDictComp {
                key,
                value,
                generators,
                ..
            }) => {
                let requirements = std::mem::take(&mut self.relations.requirements);
                let mut bindings = IndexSet::new();

                for comprehension in generators {
                    self.visit_expr(&comprehension.iter);

                    bindings = std::mem::replace(&mut self.relations.bindings, bindings);
                    self.visit_expr(&comprehension.target);
                    bindings = std::mem::replace(&mut self.relations.bindings, bindings);

                    for expr in &comprehension.ifs {
                        self.visit_expr(expr);
                    }
                }

                self.visit_expr(key);
                self.visit_expr(value);

                let requirements =
                    std::mem::replace(&mut self.relations.requirements, requirements);

                for requirement in requirements {
                    if !self.relations.bindings.contains(requirement.name)
                        && !bindings.contains(requirement.name)
                    {
                        self.relations.requirements.insert(requirement);
                    }
                }
            }
            Expr::Name(ExprName { id, ctx, .. }) => {
                if self.is_store_requirement || ctx != &ExprContext::Store {
                    self.relations.requirements.insert(Requirement {
                        name: id,
                        is_deferred: self.is_deferred,
                        context: RequirementContext::Local,
                    });
                }
                if ctx == &ExprContext::Store {
                    self.relations.bindings.insert(id);
                }
            }
            expr => walk_expr(self, expr),
        };
    }

    fn visit_except_handler(&mut self, except_handler: &'a ExceptHandler) {
        match except_handler {
            ExceptHandler::ExceptHandler(ExceptHandlerExceptHandler {
                type_, name, body, ..
            }) => {
                if let Some(expr) = type_ {
                    self.visit_expr(expr);
                }
                if let Some(name) = name {
                    self.relations.bindings.insert(name);
                }
                self.visit_body(body);
            }
        };
    }

    fn visit_alias(&mut self, alias: &'a Alias) {
        match &alias.asname {
            Some(asname) => self.relations.bindings.insert(asname),
            None => match alias.name.split_once('.') {
                Some((prefix, _)) => self.relations.bindings.insert(prefix),
                _ => self.relations.bindings.insert(&alias.name),
            },
        };
    }

    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        match pattern {
            Pattern::MatchStar(PatternMatchStar { name, .. }) => {
                if let Some(name) = name {
                    self.relations.bindings.insert(name);
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
                    self.relations.bindings.insert(rest);
                }
            }
            Pattern::MatchAs(PatternMatchAs { pattern, name, .. }) => {
                if let Some(pattern) = pattern {
                    self.visit_pattern(pattern);
                }
                if let Some(name) = name {
                    self.relations.bindings.insert(name);
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
    use rustpython_parser::lexer::lex;
    use rustpython_parser::{parse_tokens, Mode};
    use unindent::unindent;

    fn parse(source: &str) -> Stmt {
        let source = unindent(source);
        let tokens = lex(&source, Mode::Module);
        let parsed = parse_tokens(tokens, Mode::Module, "test.py").unwrap();
        match parsed {
            Mod::Module(ModModule { body, .. }) => body.into_iter().next().unwrap(),
            _ => panic!("Unsupported module type"),
        }
    }

    mod bindings {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn function_def() {
            let stmt = parse(
                r#"
                @a
                def b(c: d = e, /, f: g = h, *i: j, k: l = m, **n: o) -> p:
                    q = r
            "#,
            );
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["b"]);
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.bindings),
                ["a", "g", "l", "t", "e", "j", "o", "r", "w", "y", "c"]
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
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["b"]);
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.bindings),
                ["a", "g", "l", "t", "e", "j", "o", "r", "w", "y", "c"]
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
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["b"]);
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
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a", "d", "g", "c"]);
        }

        #[test]
        fn return_() {
            let stmt = parse(r#"return a"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn return_with_walrus() {
            let stmt = parse(r#"return (a := b)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a"]);
        }

        #[test]
        fn delete() {
            let stmt = parse(r#"del a, b.c, d[e:f:g]"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn delete_with_walrus() {
            let stmt = parse(r#"del a, (b := c).d, (e := f)[(g := h) : (i := j) : (k := l)]"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.bindings),
                ["b", "e", "g", "i", "k"]
            );
        }

        #[test]
        fn assign() {
            let stmt = parse(r#"a = b, c.d, [e, *f], *g = h"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.bindings),
                ["a", "b", "e", "f", "g"]
            );
        }

        #[test]
        fn assign_with_walrus() {
            let stmt = parse(r#"a = b, (c := d).e, [f, *g], *h = (i := j)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.bindings),
                ["i", "a", "b", "c", "f", "g", "h"]
            );
        }

        #[test]
        fn aug_assign() {
            let stmt = parse(r#"a += b"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a"]);
        }

        #[test]
        fn aug_assign_with_walrus() {
            let stmt = parse(r#"a += (b := c)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["b", "a"]);
        }

        #[test]
        fn ann_assign() {
            let stmt = parse(r#"a: b = c"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a"]);
        }

        #[test]
        fn ann_assign_with_walrus() {
            let stmt = parse(r#"a: (b := c) = (d := e)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["d", "b", "a"]);
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
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a", "c", "e"]);
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
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["b", "a", "d", "f"]);
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
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a", "c", "e"]);
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
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["b", "a", "d", "f"]);
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
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["b", "d"]);
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
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a", "c", "e"]);
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
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["b", "e", "g"]);
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.bindings),
                ["a", "c", "e", "g", "i"]
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
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["b", "d", "e", "f"]);
        }

        #[test]
        fn with_with_walrus() {
            let stmt = parse(
                r#"
                with (a := b) as c, (d := e) as (f, g):
                    h = i
            "#,
            );
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.bindings),
                ["a", "c", "d", "f", "g", "h"]
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
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["b", "d", "e", "f"]);
        }

        #[test]
        fn async_with_with_walrus() {
            let stmt = parse(
                r#"
                async with (a := b) as c, (d := e) as (f, g):
                    h = i
            "#,
            );
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.bindings),
                ["a", "c", "d", "f", "g", "h"]
            );
        }

        #[test]
        fn raise() {
            let stmt = parse(r#"raise a from b"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn raise_with_walrus() {
            let stmt = parse(r#"raise (a := b) from (c := d)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a", "c"]);
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.bindings),
                ["a", "d", "e", "g", "i"]
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.bindings),
                ["a", "c", "e", "f", "h", "j"]
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.bindings),
                ["a", "d", "e", "g", "i"]
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.bindings),
                ["a", "c", "e", "f", "h", "j"]
            );
        }

        #[test]
        fn assert() {
            let stmt = parse(r#"assert a, b"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn assert_with_walrus() {
            let stmt = parse(r#"assert (a := b), (c := d)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a", "c"]);
        }

        #[test]
        fn import() {
            let stmt = parse(r#"import a"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a"]);
        }

        #[test]
        fn import_with_submodule() {
            let stmt = parse(r#"import a.b.c"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a"]);
        }

        #[test]
        fn import_with_alias() {
            let stmt = parse(r#"import a as b"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["b"]);
        }

        #[test]
        fn import_from() {
            let stmt = parse(r#"from a import b, c"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["b", "c"]);
        }

        #[test]
        fn import_from_with_alias() {
            let stmt = parse(r#"from a import b as c, d as e"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["c", "e"]);
        }

        #[test]
        fn global() {
            let stmt = parse(r#"global a, b"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a", "b"]);
        }

        #[test]
        fn nonlocal() {
            let stmt = parse(r#"nonlocal a, b"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a", "b"]);
        }

        #[test]
        fn pass() {
            let stmt = parse(r#"pass"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn break_() {
            let stmt = parse(r#"break"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn continue_() {
            let stmt = parse(r#"continue"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn bool_op() {
            let stmt = parse(r#"a and b"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn bool_op_with_walrus() {
            let stmt = parse(r#"(a := b) and (c := d)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a", "c"]);
        }

        #[test]
        fn named_expr() {
            let stmt = parse(r#"(a := b)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a"]);
        }

        #[test]
        fn bin_op() {
            let stmt = parse(r#"a + b"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn bin_op_with_walrus() {
            let stmt = parse(r#"(a := b) + (c := d)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a", "c"]);
        }

        #[test]
        fn unary_op() {
            let stmt = parse(r#"-a"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn unary_op_with_walrus() {
            let stmt = parse(r#"-(a := b)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a"]);
        }

        #[test]
        fn lambda() {
            let stmt = parse(r#"lambda a = b, /, c = d, *e, f = g, **h: i"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn lambda_with_walrus() {
            let stmt =
                parse(r#"lambda a = (b := c), /, d = (e := f), *g, h = (i := j), **k: (l := m)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["b", "e", "i"]);
        }

        #[test]
        fn if_exp() {
            let stmt = parse(r#"a if b else c"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn if_exp_with_walrus() {
            let stmt = parse(r#"(a := b) if (c := d) else (e := f)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["c", "a", "e"]);
        }

        #[test]
        fn dict() {
            let stmt = parse(r#"{a: b, **c}"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn dict_with_walrus() {
            let stmt = parse(r#"{(a := b): (c := d), **(e := f)}"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a", "c", "e"]);
        }

        #[test]
        fn set() {
            let stmt = parse(r#"{a, *b}"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn set_with_walrus() {
            let stmt = parse(r#"{(a := b), *(c := d)}"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a", "c"]);
        }

        #[test]
        fn list_comp() {
            let stmt = parse(r#"[a for b in c if d]"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn list_comp_with_walrus() {
            let stmt = parse(r#"[(a := b) for c in (d := f) if (g := h)]"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["d", "g", "a"]);
        }

        #[test]
        fn set_comp() {
            let stmt = parse(r#"{a for b in c if d}"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn set_comp_with_walrus() {
            let stmt = parse(r#"{(a := b) for c in (d := f) if (g := h)}"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["d", "g", "a"]);
        }

        #[test]
        fn dict_comp() {
            let stmt = parse(r#"{a: b for c in d if e}"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn dict_comp_with_walrus() {
            let stmt = parse(r#"{(a := b): (c := d) for e in (f := g) if (h := i)}"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["f", "h", "a", "c"]);
        }

        #[test]
        fn generator_exp() {
            let stmt = parse(r#"(a for b in c if d)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn generator_exp_with_walrus() {
            let stmt = parse(r#"((a := b) for c in (d := f) if (g := h))"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["d", "g", "a"]);
        }

        #[test]
        fn await_() {
            let stmt = parse(r#"await a"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn await_with_walrus() {
            let stmt = parse(r#"await (a := b)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a"]);
        }

        #[test]
        fn yield_() {
            let stmt = parse(r#"yield a"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn yield_with_walrus() {
            let stmt = parse(r#"yield (a := b)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a"]);
        }

        #[test]
        fn yield_from() {
            let stmt = parse(r#"yield from a"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn yield_from_with_walrus() {
            let stmt = parse(r#"yield from (a := b)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a"]);
        }

        #[test]
        fn compare() {
            let stmt = parse(r#"a < b < c"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn compare_with_walrus() {
            let stmt = parse(r#"(a := b) < (c := d) < (e := f)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a", "c", "e"]);
        }

        #[test]
        fn call() {
            let stmt = parse(r#"a(b, *c, d=e, **f)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn call_with_walrus() {
            let stmt = parse(r#"(a := b)((c := d), *(e := f), g=(h := i), **(j := k))"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.bindings),
                ["a", "c", "e", "h", "j"]
            );
        }

        #[test]
        fn formatted_value() {
            let stmt = parse(r#"f"{a:{b}}""#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn formatted_value_with_walrus() {
            let stmt = parse(r#"f"{(a := b):{(c := d)}}""#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a", "c"]);
        }

        #[test]
        fn joined_str() {
            let stmt = parse(r#"f"{a:{b}} {c:{d}}""#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn joined_str_with_walrus() {
            let stmt = parse(r#"f"{(a := b):{(c := d)}} {(e := f):{(g := h)}}""#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a", "c", "e", "g"]);
        }

        #[test]
        fn constant() {
            let stmt = parse(r#"1"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn attribute() {
            let stmt = parse(r#"a.b.c"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn attribute_with_walrus() {
            let stmt = parse(r#"(a := b).c.d"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a"]);
        }

        #[test]
        fn subscript() {
            let stmt = parse(r#"a[b:c:d]"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn subscript_with_walrus() {
            let stmt = parse(r#"(a := b)[(c := d):(e := f):(g := h)]"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a", "c", "e", "g"]);
        }

        #[test]
        fn starred() {
            let stmt = parse(r#"*a"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn starred_with_walrus() {
            let stmt = parse(r#"*(a := b)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a"]);
        }

        #[test]
        fn name() {
            let stmt = parse(r#"a"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn list() {
            let stmt = parse(r#"[a, b, *c]"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn list_with_walrus() {
            let stmt = parse(r#"[(a := b), (c := d), *(e := f)]"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a", "c", "e"]);
        }

        #[test]
        fn tuple() {
            let stmt = parse(r#"(a, b, *c)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), [] as [&str; 0]);
        }

        #[test]
        fn tuple_with_walrus() {
            let stmt = parse(r#"((a := b), (c := d), *(e := f))"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["a", "c", "e"]);
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
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["b", "d"]);
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
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["b", "d"]);
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
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["b", "c", "e", "g"]);
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
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["c", "e", "f", "h"]);
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
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["c", "e", "g", "i"]);
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
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["b", "d"]);
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
            let relations = stmt_relations(&stmt);
            assert_eq!(Vec::from_iter(relations.bindings), ["b", "c", "d", "f"]);
        }
    }

    mod requirements {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn function_def() {
            let stmt = parse(
                r#"
                @a
                def b(c: d = e, /, f: g = h, *i: j, k: l = m, **n: o) -> p:
                    q = r
            "#,
            );
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
        fn function_def_with_bindings() {
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
                    _ = a, c, d, e, g, i, j, l, n, o, q, r, t, v, w, y
            "#,
            );
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
        fn async_function_def() {
            let stmt = parse(
                r#"
                @a
                async def b(c: d = e, /, f: g = h, *i: j, k: l = m, **n: o) -> p:
                    q = r
            "#,
            );
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
        fn async_function_def_with_bindings() {
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
                    _ = a, c, d, e, g, i, j, l, n, o, q, r, t, v, w, y
            "#,
            );
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
        fn class_def() {
            let stmt = parse(
                r#"
                @a
                class b(c, d=f):
                    g = h
            "#,
            );
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
        fn class_def_with_bindings() {
            let stmt = parse(
                r#"
                @(a := b)
                class c((d := e), f=(g := h)):
                    _ = a, c, d, g
            "#,
            );
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
        fn return_() {
            let stmt = parse(r#"return a"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
                    }
                ]
            );
        }

        #[test]
        fn aug_assign_with_walrus() {
            let stmt = parse(r#"a += (b := c)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
                [
                    Requirement {
                        name: "c",
                        is_deferred: false,
                        context: RequirementContext::Local
                    },
                    Requirement {
                        name: "a",
                        is_deferred: false,
                        context: RequirementContext::Local
                    }
                ]
            );
        }

        #[test]
        fn ann_assign() {
            let stmt = parse(r#"a: b = c"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
        fn async_for_() {
            let stmt = parse(
                r#"
                async for a in b:
                    c = d
                else:
                    e = f
            "#,
            );
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
        fn while_() {
            let stmt = parse(
                r#"
                while a:
                    b = c
                else:
                    d = e
            "#,
            );
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
        fn with() {
            let stmt = parse(
                r#"
                with a as b, c as (d, e):
                    f = g
            "#,
            );
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
        fn async_with() {
            let stmt = parse(
                r#"
                async with a as b, c as (d, e):
                    f = g
            "#,
            );
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
        fn raise() {
            let stmt = parse(r#"raise a from b"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
        fn assert() {
            let stmt = parse(r#"assert a, b"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
                [] as [Requirement; 0]
            );
        }

        #[test]
        fn import_with_submodule() {
            let stmt = parse(r#"import a.b.c"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
                [] as [Requirement; 0]
            );
        }

        #[test]
        fn import_with_alias() {
            let stmt = parse(r#"import a as b"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
                [] as [Requirement; 0]
            );
        }

        #[test]
        fn import_from() {
            let stmt = parse(r#"from a import b, c"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
                [] as [Requirement; 0]
            );
        }

        #[test]
        fn import_from_with_alias() {
            let stmt = parse(r#"from a import b as c, d as e"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
                [] as [Requirement; 0]
            );
        }

        #[test]
        fn global() {
            let stmt = parse(r#"global a, b"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
                [] as [Requirement; 0]
            );
        }

        #[test]
        fn break_() {
            let stmt = parse(r#"break"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
                [] as [Requirement; 0]
            );
        }

        #[test]
        fn continue_() {
            let stmt = parse(r#"continue"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
                [] as [Requirement; 0]
            );
        }

        #[test]
        fn bool_op() {
            let stmt = parse(r#"a and b"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
        fn if_exp() {
            let stmt = parse(r#"a if b else c"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
        fn dict() {
            let stmt = parse(r#"{a: b, **c}"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
        fn set_comp() {
            let stmt = parse(r#"{a for b in c if d}"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
        fn dict_comp() {
            let stmt = parse(r#"{a: b for c in d if e}"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
        fn generator_exp() {
            let stmt = parse(r#"(a for b in c if d)"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
        fn await_() {
            let stmt = parse(r#"await a"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
                [] as [Requirement; 0]
            );
        }

        #[test]
        fn attribute() {
            let stmt = parse(r#"a.b.c"#);
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
            let relations = stmt_relations(&stmt);
            assert_eq!(
                Vec::from_iter(relations.requirements),
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
}
