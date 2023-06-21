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
        let stmt = parse(r#"def function(): pass"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["function"]);
    }

    #[test]
    fn function_def_with_walrus_in_defaults() {
        let stmt = parse(
            r#"
                def function(
                    posonly = (posonly_default := None),
                    /,
                    arg = (arg_default := None),
                    *args,
                    kwarg = (kwarg_default := None),
                    **kwargs
                ):
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(
            bindings,
            [
                "posonly_default",
                "arg_default",
                "kwarg_default",
                "function",
            ]
        );
    }

    #[test]
    fn function_def_with_walrus_in_annotations() {
        let stmt = parse(
            r#"
                def function(
                    posonly: (posonly_type := int),
                    / ,
                    arg: (arg_type := int),
                    *args: (args_type := int),
                    kwarg: (kwarg_type := int),
                    **kwargs: (kwargs_type := int)
                ) -> (return_type := int):
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(
            bindings,
            [
                "posonly_type",
                "arg_type",
                "args_type",
                "kwarg_type",
                "kwargs_type",
                "return_type",
                "function",
            ],
        );
    }

    #[test]
    fn function_def_with_walrus_in_decorator() {
        let stmt = parse(
            r#"
                @(decorator := my_decorator)
                def function(x):
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["decorator", "function"],);
    }

    #[test]
    fn async_function_def() {
        let stmt = parse(r#"async def function(): pass"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["function"]);
    }

    #[test]
    fn async_function_def_with_walrus_in_defaults() {
        let stmt = parse(
            r#"
                async def function(
                    posonly = (posonly_default := None),
                    /,
                    arg = (arg_default := None),
                    *args,
                    kwarg = (kwarg_default := None),
                    **kwargs
                ):
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(
            bindings,
            [
                "posonly_default",
                "arg_default",
                "kwarg_default",
                "function",
            ]
        );
    }

    #[test]
    fn async_function_def_with_walrus_in_annotations() {
        let stmt = parse(
            r#"
                async def function(
                    posonly: (posonly_type := int),
                    / ,
                    arg: (arg_type := int),
                    *args: (args_type := int),
                    kwarg: (kwarg_type := int),
                    **kwargs: (kwargs_type := int)
                ) -> (return_type := int):
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(
            bindings,
            [
                "posonly_type",
                "arg_type",
                "args_type",
                "kwarg_type",
                "kwargs_type",
                "return_type",
                "function",
            ]
        );
    }

    #[test]
    fn async_function_def_with_walrus_in_decorator() {
        let stmt = parse(
            r#"
                @(decorator := my_decorator)
                def function(x):
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["decorator", "function"]);
    }

    #[test]
    fn class_def() {
        let stmt = parse(
            r#"
                class ClassName:
                    a = 1
                    def b(self):
                        pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["ClassName"]);
    }

    #[test]
    fn class_def_with_walrus_in_decorator() {
        let stmt = parse(
            r#"
                @(decorator := my_decorator)
                class ClassName:
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["decorator", "ClassName"]);
    }

    #[test]
    fn class_def_with_walrus_in_base_class() {
        let stmt = parse(
            r#"
                class ClassName(BaseClass, (OtherBaseClass := MyBaseClass)):
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["OtherBaseClass", "ClassName"]);
    }

    #[test]
    fn class_def_with_walrus_in_metaclass() {
        let stmt = parse(
            r#"
                class ClassName(metaclass = (MetaClass := MyMetaClass)):
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["MetaClass", "ClassName"]);
    }

    #[test]
    fn class_def_with_walrus_in_body() {
        let stmt = parse(
            r#"
                class ClassName:
                    a = (x := 2)
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["ClassName"]);
    }

    #[test]
    fn return_with_variable() {
        let stmt = parse(r#"return x"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn return_with_walrus() {
        let stmt = parse(r#"return (x := 1)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["x"]);
    }

    #[test]
    fn delete_with_single_variable() {
        let stmt = parse(r#"del a"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn delete_with_multiple_variables() {
        let stmt = parse(r#"del a, b"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn delete_with_subscript() {
        let stmt = parse(r#"del a[b:c]"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn delete_with_attribute() {
        let stmt = parse(r#"del a.b"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn assign_with_variable() {
        let stmt = parse(r#"a = b"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a"]);
    }

    #[test]
    fn assign_with_unpacked_variable() {
        let stmt = parse(r#"a, *b = c"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "b"]);
    }

    #[test]
    fn assign_with_attribute() {
        let stmt = parse(r#"a.b = c"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn assign_with_destructured_list() {
        let stmt = parse(r#"[a, [b, c]] = d"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "b", "c"]);
    }

    #[test]
    fn assign_with_destructed_list_and_unpacked_variable() {
        let stmt = parse(r#"[a, *b] = c"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "b"]);
    }

    #[test]
    fn assign_with_walrus() {
        let stmt = parse(r#"a = (b := c)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "a"]);
    }

    #[test]
    fn aug_assign_with_variable() {
        let stmt = parse(r#"a += b"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a"]);
    }

    #[test]
    fn aug_assign_with_attribute() {
        let stmt = parse(r#"a.b /= c"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn aug_assign_with_walrus() {
        let stmt = parse(r#"a ^= (b := c)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "b"]);
    }

    #[test]
    fn ann_assign_with_variable() {
        let stmt = parse(r#"a: int = b"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a"]);
    }

    #[test]
    fn ann_assign_with_no_value() {
        let stmt = parse(r#"a: int"#);
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
    fn for_with_else() {
        let stmt = parse(
            r#"
                for i in range(10):
                    a += i
                else:
                    b = 1
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["i", "a", "b"]);
    }

    #[test]
    fn for_with_walrus() {
        let stmt = parse(
            r#"
                for i in (r := range(10)):
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["r", "i"]);
    }

    #[test]
    fn async_for_with_else() {
        let stmt = parse(
            r#"
                async for i in range(10):
                    a += i
                else:
                    b = 1
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["i", "a", "b"]);
    }

    #[test]
    fn async_for_with_walrus() {
        let stmt = parse(
            r#"
                async for i in (r := range(10)):
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["r", "i"]);
    }

    #[test]
    fn while_with_else() {
        let stmt = parse(
            r#"
                while x:
                    a = 1
                else:
                    b = 1
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "b"]);
    }

    #[test]
    fn while_with_walrus() {
        let stmt = parse(
            r#"
                while (x := y):
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["x"]);
    }

    #[test]
    fn if_with_elif_and_else() {
        let stmt = parse(
            r#"
                if x:
                    a = 1
                elif y:
                    b = 2
                else:
                    c = 3
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "b", "c"]);
    }

    #[test]
    fn if_with_walrus() {
        let stmt = parse(
            r#"
                if (x := y):
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["x"]);
    }

    #[test]
    fn with() {
        let stmt = parse(
            r#"
                with a:
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn with_with_single_alias() {
        let stmt = parse(
            r#"
                with a as b:
                    c = 1
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "c"]);
    }

    #[test]
    fn with_with_multiple_aliases() {
        let stmt = parse(
            r#"
                with a as b, c as d:
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "d"]);
    }

    #[test]
    fn with_with_tuple_alias() {
        let stmt = parse(
            r#"
                with a as (b, c):
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "c"]);
    }

    #[test]
    fn with_with_walrus() {
        let stmt = parse(
            r#"
                with (a := b) as c:
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c"]);
    }

    #[test]
    fn async_with() {
        let stmt = parse(
            r#"
                async with a:
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn async_with_with_single_alias() {
        let stmt = parse(
            r#"
                async with a as b:
                    c = 1
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "c"]);
    }

    #[test]
    fn async_with_with_multiple_aliases() {
        let stmt = parse(
            r#"
                async with a as b, c as d:
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "d"]);
    }

    #[test]
    fn async_with_with_tuple_alias() {
        let stmt = parse(
            r#"
                async with a as (b, c):
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "c"]);
    }

    #[test]
    fn async_with_with_walrus() {
        let stmt = parse(
            r#"
                async with (a := b) as c:
                    pass
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "c"]);
    }

    #[test]
    fn raise() {
        let stmt = parse(r#"raise"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn raise_with_exception() {
        let stmt = parse(r#"raise x"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn raise_with_cause() {
        let stmt = parse(r#"raise x from y"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn raise_with_walrus_in_exception() {
        let stmt = parse(r#"raise (x := y)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["x"]);
    }

    #[test]
    fn raise_with_walrus_in_cause() {
        let stmt = parse(r#"raise x from (y := z)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["y"]);
    }

    #[test]
    fn try_with_else_and_finally() {
        let stmt = parse(
            r#"
                try:
                    a = b
                except x as y:
                    c = d
                else:
                    e = f
                finally:
                    g = h
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "y", "c", "e", "g"]);
    }

    #[test]
    fn try_with_walrus() {
        let stmt = parse(
            r#"
                try:
                    a = b
                except (x := y):
                    c = d
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "x", "c"]);
    }

    #[test]
    fn try_star_with_else_and_finally() {
        let stmt = parse(
            r#"
                try:
                    a = b
                except* x as y:
                    c = d
                else:
                    e = f
                finally:
                    g = h
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "y", "c", "e", "g"]);
    }

    #[test]
    fn try_star_with_walrus() {
        let stmt = parse(
            r#"
                try:
                    a = b
                except* (x := y):
                    c = d
            "#,
        );
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["a", "x", "c"]);
    }

    #[test]
    fn assert() {
        let stmt = parse(r#"assert x"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn assert_with_message() {
        let stmt = parse(r#"assert x, y"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn assert_with_walrus_in_test() {
        let stmt = parse(r#"assert (x := y)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["x"]);
    }

    #[test]
    fn assert_with_walrus_in_message() {
        let stmt = parse(r#"assert x, (y := z)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["y"]);
    }

    #[test]
    fn import() {
        let stmt = parse(r#"import x"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["x"]);
    }

    #[test]
    fn import_with_alias() {
        let stmt = parse(r#"import x as y"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["y"]);
    }

    #[test]
    fn import_with_submodule() {
        let stmt = parse(r#"import x.y.z"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["x"]);
    }

    #[test]
    fn import_from() {
        let stmt = parse(r#"from x import y, z"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["y", "z"]);
    }

    #[test]
    fn import_from_with_alias() {
        let stmt = parse(r#"from x import y as z"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["z"]);
    }

    #[test]
    fn global_with_single_name() {
        let stmt = parse(r#"global x"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["x"]);
    }

    #[test]
    fn global_with_multiple_names() {
        let stmt = parse(r#"global x, y"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["x", "y"]);
    }

    #[test]
    fn nonlocal_with_single_name() {
        let stmt = parse(r#"nonlocal x"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["x"]);
    }

    #[test]
    fn nonlocal_with_multiple_names() {
        let stmt = parse(r#"nonlocal x, y"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["x", "y"]);
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
        let stmt = parse(r#"break"#);
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
        let stmt = parse(r#"lambda x: x"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn lambda_with_walrus() {
        let stmt = parse(r#"lambda a = (b := c), *, d = (e := f): (g := h)"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, ["b", "e"]);
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
        let stmt = parse(r#"await x"#);
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
        let stmt = parse(r#"yield"#);
        let bindings = bindings(&stmt);
        assert_eq!(bindings, [] as [&str; 0]);
    }

    #[test]
    fn yield_with_value() {
        let stmt = parse(r#"yield x"#);
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
        let stmt = parse(r#"yield from x"#);
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
