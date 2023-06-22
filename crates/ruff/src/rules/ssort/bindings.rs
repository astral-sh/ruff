use ruff_python_ast::prelude::*;
use ruff_python_ast::visitor::{walk_expr, walk_pattern, walk_stmt, Visitor};

pub(super) fn bindings(stmt: &Stmt) -> Vec<&str> {
    let mut bindings = Bindings { bindings: vec![] };
    bindings.visit_stmt(stmt);
    bindings.bindings
}

struct Bindings<'a> {
    bindings: Vec<&'a str>,
}

impl<'a> Visitor<'a> for Bindings<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(StmtFunctionDef {
                name,
                args,
                decorator_list,
                returns,
                ..
            })
            | Stmt::AsyncFunctionDef(StmtAsyncFunctionDef {
                name,
                args,
                decorator_list,
                returns,
                ..
            }) => {
                // Skip visiting function body. Function body's never produce bindings.
                for decorator in decorator_list {
                    self.visit_decorator(decorator);
                }

                self.visit_arguments(args);

                for expr in returns {
                    self.visit_annotation(expr);
                }

                self.bindings.push(name.as_str());
            }
            Stmt::ClassDef(StmtClassDef {
                name,
                bases,
                keywords,
                decorator_list,
                ..
            }) => {
                // Skip visiting class body. Class body's never produce bindings.
                for decorator in decorator_list {
                    self.visit_decorator(decorator);
                }

                for expr in bases {
                    self.visit_expr(expr);
                }

                for keyword in keywords {
                    self.visit_keyword(keyword);
                }

                self.bindings.push(name.as_str());
            }
            Stmt::Global(StmtGlobal { names, .. }) | Stmt::Nonlocal(StmtNonlocal { names, .. }) => {
                self.bindings.extend(names.iter().map(Identifier::as_str));
            }
            stmt => walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Lambda(ExprLambda { args, .. }) => {
                // Skip visiting lambda body. Lambda body's never produce bindings.
                self.visit_arguments(args);
            }
            Expr::Name(ExprName { id, ctx, .. }) => {
                if ctx == &ExprContext::Store {
                    self.bindings.push(id);
                }
            }
            expr => walk_expr(self, expr),
        }
    }

    fn visit_comprehension(&mut self, comprehension: &'a Comprehension) {
        // Skip visiting comprehension target. Comprehension target's never produce bindings.
        self.visit_expr(&comprehension.iter);
        for condition in &comprehension.ifs {
            self.visit_expr(condition);
        }
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
                    self.bindings.push(name.as_str());
                }
                self.visit_body(body);
            }
        }
    }

    fn visit_alias(&mut self, alias: &'a Alias) {
        match &alias.asname {
            Some(asname) => self.bindings.push(asname.as_str()),
            None => match alias.name.split_once('.') {
                Some((prefix, _)) => self.bindings.push(prefix),
                _ => self.bindings.push(alias.name.as_str()),
            },
        }
    }

    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        match pattern {
            Pattern::MatchStar(PatternMatchStar { name, .. }) => {
                if let Some(name) = name {
                    self.bindings.push(name.as_str());
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
                    self.bindings.push(rest.as_str());
                }
            }
            Pattern::MatchAs(PatternMatchAs { pattern, name, .. }) => {
                if let Some(pattern) = pattern {
                    self.visit_pattern(pattern);
                }
                if let Some(name) = name {
                    self.bindings.push(name.as_str());
                }
            }
            pattern => walk_pattern(self, pattern),
        }
    }
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(
            bindings,
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(
            bindings,
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "d", "g", "c"]);
    }

    #[test]
    fn return_() {
        let stmt = parse(r#"return a"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn return_with_walrus() {
        let stmt = parse(r#"return (a := b)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a"]);
    }

    #[test]
    fn delete() {
        let stmt = parse(r#"del a, b.c, d[e:f:g]"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn delete_with_walrus() {
        let stmt = parse(r#"del a, (b := c).d, (e := f)[(g := h) : (i := j) : (k := l)]"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "e", "g", "i", "k"]);
    }

    #[test]
    fn assign() {
        let stmt = parse(r#"a, b.c, [d, *e], *f = g"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "d", "e", "f"]);
    }

    #[test]
    fn assign_with_walrus() {
        let stmt = parse(r#"a, (b := c).d, [e, *f], *g = (h := i)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["h", "a", "b", "e", "f", "g"]);
    }

    #[test]
    fn aug_assign() {
        let stmt = parse(r#"a += b"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a"]);
    }

    #[test]
    fn aug_assign_with_walrus() {
        let stmt = parse(r#"a += (b := c)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "b"]);
    }

    #[test]
    fn ann_assign() {
        let stmt = parse(r#"a: b = c"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a"]);
    }

    #[test]
    fn ann_assign_with_walrus() {
        let stmt = parse(r#"a: (b := c) = (d := e)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["d", "b", "a"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c", "e"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "a", "d", "f"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c", "e"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "a", "d", "f"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "d"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c", "e"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "e", "g"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c", "e", "g", "i"]);
    }

    #[test]
    fn with() {
        let stmt = parse(
            r#"
                with a as b, c as (d, e):
                    f = g
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "d", "e", "f"]);
    }

    #[test]
    fn with_with_walrus() {
        let stmt = parse(
            r#"
                with (a := b) as c, (d := e) as (f, g):
                    h = i
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c", "d", "f", "g", "h"]);
    }

    #[test]
    fn async_with() {
        let stmt = parse(
            r#"
                async with a as b, c as (d, e):
                    f = g
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "d", "e", "f"]);
    }

    #[test]
    fn async_with_with_walrus() {
        let stmt = parse(
            r#"
                async with (a := b) as c, (d := e) as (f, g):
                    h = i
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c", "d", "f", "g", "h"]);
    }

    #[test]
    fn raise() {
        let stmt = parse(r#"raise a from b"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn raise_with_walrus() {
        let stmt = parse(r#"raise (a := b) from (c := d)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "d", "e", "g", "i"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c", "e", "f", "h", "j"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "d", "e", "g", "i"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c", "e", "f", "h", "j"]);
    }

    #[test]
    fn assert() {
        let stmt = parse(r#"assert a, b"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn assert_with_walrus() {
        let stmt = parse(r#"assert (a := b), (c := d)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c"]);
    }

    #[test]
    fn import() {
        let stmt = parse(r#"import a"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a"]);
    }

    #[test]
    fn import_with_submodule() {
        let stmt = parse(r#"import a.b.c"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a"]);
    }

    #[test]
    fn import_with_alias() {
        let stmt = parse(r#"import a as b"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b"]);
    }

    #[test]
    fn import_from() {
        let stmt = parse(r#"from a import b, c"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "c"]);
    }

    #[test]
    fn import_from_with_alias() {
        let stmt = parse(r#"from a import b as c, d as e"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["c", "e"]);
    }

    #[test]
    fn global() {
        let stmt = parse(r#"global a, b"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "b"]);
    }

    #[test]
    fn nonlocal() {
        let stmt = parse(r#"nonlocal a, b"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "b"]);
    }

    #[test]
    fn pass() {
        let stmt = parse(r#"pass"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn break_() {
        let stmt = parse(r#"break"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn continue_() {
        let stmt = parse(r#"continue"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn bool_op() {
        let stmt = parse(r#"a and b"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn bool_op_with_walrus() {
        let stmt = parse(r#"(a := b) and (c := d)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c"]);
    }

    #[test]
    fn named_expr() {
        let stmt = parse(r#"(a := b)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a"]);
    }

    #[test]
    fn bin_op() {
        let stmt = parse(r#"a + b"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn bin_op_with_walrus() {
        let stmt = parse(r#"(a := b) + (c := d)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c"]);
    }

    #[test]
    fn unary_op() {
        let stmt = parse(r#"-a"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn unary_op_with_walrus() {
        let stmt = parse(r#"-(a := b)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a"]);
    }

    #[test]
    fn lambda() {
        let stmt = parse(r#"lambda a = b, /, c = d, *e, f = g, **h: i"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn lambda_with_walrus() {
        let stmt =
            parse(r#"lambda a = (b := c), /, d = (e := f), *g, h = (i := j), **k: (l := m)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "e", "i"]);
    }

    #[test]
    fn if_exp() {
        let stmt = parse(r#"a if b else c"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn if_exp_with_walrus() {
        let stmt = parse(r#"(a := b) if (c := d) else (e := f)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["c", "a", "e"]);
    }

    #[test]
    fn dict() {
        let stmt = parse(r#"{a: b, **c}"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn dict_with_walrus() {
        let stmt = parse(r#"{(a := b): (c := d), **(e := f)}"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c", "e"]);
    }

    #[test]
    fn set() {
        let stmt = parse(r#"{a, *b}"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn set_with_walrus() {
        let stmt = parse(r#"{(a := b), *(c := d)}"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c"]);
    }

    #[test]
    fn list_comp() {
        let stmt = parse(r#"[a for b in c if d]"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn list_comp_with_walrus() {
        let stmt = parse(r#"[(a := b) for c in (d := f) if (g := h)]"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["d", "g", "a"]);
    }

    #[test]
    fn set_comp() {
        let stmt = parse(r#"{a for b in c if d}"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn set_comp_with_walrus() {
        let stmt = parse(r#"{(a := b) for c in (d := f) if (g := h)}"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["d", "g", "a"]);
    }

    #[test]
    fn dict_comp() {
        let stmt = parse(r#"{a: b for c in d if e}"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn dict_comp_with_walrus() {
        let stmt = parse(r#"{(a := b): (c := d) for e in (f := g) if (h := i)}"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["f", "h", "a", "c"]);
    }

    #[test]
    fn generator_exp() {
        let stmt = parse(r#"(a for b in c if d)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn generator_exp_with_walrus() {
        let stmt = parse(r#"((a := b) for c in (d := f) if (g := h))"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["d", "g", "a"]);
    }

    #[test]
    fn await_() {
        let stmt = parse(r#"await a"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn await_with_walrus() {
        let stmt = parse(r#"await (a := b)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a"]);
    }

    #[test]
    fn yield_() {
        let stmt = parse(r#"yield a"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn yield_with_walrus() {
        let stmt = parse(r#"yield (a := b)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a"]);
    }

    #[test]
    fn yield_from() {
        let stmt = parse(r#"yield from a"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn yield_from_with_walrus() {
        let stmt = parse(r#"yield from (a := b)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a"]);
    }

    #[test]
    fn compare() {
        let stmt = parse(r#"a < b < c"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn compare_with_walrus() {
        let stmt = parse(r#"(a := b) < (c := d) < (e := f)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c", "e"]);
    }

    #[test]
    fn call() {
        let stmt = parse(r#"a(b, *c, d=e, **f)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn call_with_walrus() {
        let stmt = parse(r#"(a := b)((c := d), *(e := f), g=(h := i), **(j := k))"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c", "e", "h", "j"]);
    }

    #[test]
    fn formatted_value() {
        let stmt = parse(r#"f"{a:b}""#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn formatted_value_with_walrus() {
        let stmt = parse(r#"f"{(a := b):{(c := d)}}""#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c"]);
    }

    #[test]
    fn joined_str() {
        let stmt = parse(r#"f"{a:b} {c:d}""#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn joined_str_with_walrus() {
        let stmt = parse(r#"f"{(a := b):{(c := d)}} {(e := f):{(g := h)}}""#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c", "e", "g"]);
    }

    #[test]
    fn constant() {
        let stmt = parse(r#"1"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn attribute() {
        let stmt = parse(r#"a.b.c"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn attribute_with_walrus() {
        let stmt = parse(r#"(a := b).c.d"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a"]);
    }

    #[test]
    fn subscript() {
        let stmt = parse(r#"a[b:c:d]"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn subscript_with_walrus() {
        let stmt = parse(r#"(a := b)[(c := d):(e := f):(g := h)]"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c", "e", "g"]);
    }

    #[test]
    fn starred() {
        let stmt = parse(r#"*a"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn starred_with_walrus() {
        let stmt = parse(r#"*(a := b)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a"]);
    }

    #[test]
    fn name() {
        let stmt = parse(r#"a"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn list() {
        let stmt = parse(r#"[a, b, *c]"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn list_with_walrus() {
        let stmt = parse(r#"[(a := b), (c := d), *(e := f)]"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c", "e"]);
    }

    #[test]
    fn tuple() {
        let stmt = parse(r#"(a, b, *c)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn tuple_with_walrus() {
        let stmt = parse(r#"((a := b), (c := d), *(e := f))"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c", "e"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "d"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "d"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "c", "e", "g"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["c", "e", "f", "h"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["c", "e", "g"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "c"]);
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
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "c", "d", "f"]);
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
