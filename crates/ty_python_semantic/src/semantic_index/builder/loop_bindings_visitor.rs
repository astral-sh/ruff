use ruff_python_ast as ast;
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_pattern, walk_stmt};

use crate::semantic_index::place::PlaceExpr;
use crate::semantic_index::symbol::Symbol;

/// Do a pre-walk of a `while` loop to collect all the places that are bound, prior to visiting the
/// loop with `SemanticIndexBuilder`. This walk includes bindings in nested loops, but not in
/// nested scopes. (I.e. we don't descend into function bodies or class definitions.) We need this
/// pre-walk so that we can synthesize "loop header definitions" that are visible to the loop body
/// (and condition). See `LoopHeader`.
/// TODO: Handle `nonlocal` bindings from nested scopes somehow.
pub(crate) fn collect_while_loop_bindings(while_stmt: &ast::StmtWhile) -> Vec<PlaceExpr> {
    let mut collector = LoopBindingsVisitor::default();
    collector.visit_expr(&while_stmt.test);
    collector.visit_body(&while_stmt.body);
    collector.bound_places
}

/// Like `collect_while_loop_bindings` above, but for `for` loops.
pub(crate) fn collect_for_loop_bindings(for_stmt: &ast::StmtFor) -> Vec<PlaceExpr> {
    let mut collector = LoopBindingsVisitor::default();
    collector.add_place_from_target(&for_stmt.target);
    collector.visit_body(&for_stmt.body);
    collector.bound_places
}

/// The visitor that powers `collect_while_loop_bindings` and `collect_for_loop_bindings`.
///
/// This visitor doesn't walk nested function/class definitions since those are different scopes.
#[derive(Debug, Default)]
pub(crate) struct LoopBindingsVisitor {
    bound_places: Vec<PlaceExpr>,
}

impl LoopBindingsVisitor {
    pub(crate) fn add_place_from_target(&mut self, target: &ast::Expr) {
        match target {
            ast::Expr::Name(name) => {
                self.bound_places.push(PlaceExpr::from_expr_name(name));
            }
            ast::Expr::Attribute(_) | ast::Expr::Subscript(_) => {
                if let Some(place) = PlaceExpr::try_from_expr(target) {
                    self.bound_places.push(place);
                }
            }
            ast::Expr::Tuple(tuple) => {
                for elt in &tuple.elts {
                    self.add_place_from_target(elt);
                }
            }
            ast::Expr::List(list) => {
                for elt in &list.elts {
                    self.add_place_from_target(elt);
                }
            }
            ast::Expr::Starred(starred) => {
                self.add_place_from_target(&starred.value);
            }
            _ => {}
        }
    }
}

impl<'ast> Visitor<'ast> for LoopBindingsVisitor {
    fn visit_stmt(&mut self, stmt: &'ast ast::Stmt) {
        match stmt {
            ast::Stmt::Assign(node) => {
                for target in &node.targets {
                    self.add_place_from_target(target);
                }
                // Visit the value expression to find named expressions (walrus operator).
                self.visit_expr(&node.value);
            }
            ast::Stmt::AugAssign(node) => {
                self.add_place_from_target(&node.target);
                self.visit_expr(&node.value);
            }
            ast::Stmt::AnnAssign(node) => {
                if let Some(value) = &node.value {
                    self.add_place_from_target(&node.target);
                    self.visit_expr(value);
                }
            }
            ast::Stmt::For(node) => {
                self.add_place_from_target(&node.target);
                self.visit_expr(&node.iter);
                self.visit_body(&node.body);
                self.visit_body(&node.orelse);
            }
            ast::Stmt::While(node) => {
                self.visit_expr(&node.test);
                self.visit_body(&node.body);
                self.visit_body(&node.orelse);
            }
            ast::Stmt::With(node) => {
                for item in &node.items {
                    self.visit_expr(&item.context_expr);
                    if let Some(vars) = &item.optional_vars {
                        self.add_place_from_target(vars);
                    }
                }
                self.visit_body(&node.body);
            }
            ast::Stmt::Try(node) => {
                self.visit_body(&node.body);
                for handler in &node.handlers {
                    let ast::ExceptHandler::ExceptHandler(h) = handler;
                    if let Some(name) = &h.name {
                        self.bound_places
                            .push(PlaceExpr::Symbol(Symbol::new(name.id.clone())));
                    }
                    self.visit_body(&h.body);
                }
                self.visit_body(&node.orelse);
                self.visit_body(&node.finalbody);
            }
            ast::Stmt::Import(node) => {
                for alias in &node.names {
                    let name = alias.asname.as_ref().unwrap_or(&alias.name);
                    self.bound_places
                        .push(PlaceExpr::Symbol(Symbol::new(name.id.clone())));
                }
            }
            ast::Stmt::ImportFrom(node) => {
                for alias in &node.names {
                    if &*alias.name != "*" {
                        let name = alias.asname.as_ref().unwrap_or(&alias.name);
                        self.bound_places
                            .push(PlaceExpr::Symbol(Symbol::new(name.id.clone())));
                    }
                }
            }
            ast::Stmt::FunctionDef(node) => {
                self.bound_places
                    .push(PlaceExpr::Symbol(Symbol::new(node.name.id.clone())));
                // Don't descend into function bodies - they're different scopes.
            }
            ast::Stmt::ClassDef(node) => {
                self.bound_places
                    .push(PlaceExpr::Symbol(Symbol::new(node.name.id.clone())));
                // Don't descend into class bodies - they're different scopes.
            }
            ast::Stmt::Match(node) => {
                self.visit_expr(&node.subject);
                for case in &node.cases {
                    if let Some(guard) = &case.guard {
                        self.visit_expr(guard);
                    }
                    self.visit_pattern(&case.pattern);
                    self.visit_body(&case.body);
                }
            }
            ast::Stmt::Delete(node) => {
                for target in &node.targets {
                    self.add_place_from_target(target);
                }
            }
            _ => walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'ast ast::Expr) {
        // the walrus operator
        if let ast::Expr::Named(node) = expr {
            self.add_place_from_target(&node.target);
        }
        walk_expr(self, expr);
    }

    fn visit_pattern(&mut self, pattern: &'ast ast::Pattern) {
        match pattern {
            ast::Pattern::MatchAs(p) => {
                if let Some(name) = &p.name {
                    self.bound_places
                        .push(PlaceExpr::Symbol(Symbol::new(name.id.clone())));
                }
            }
            ast::Pattern::MatchStar(p) => {
                if let Some(name) = &p.name {
                    self.bound_places
                        .push(PlaceExpr::Symbol(Symbol::new(name.id.clone())));
                }
            }
            ast::Pattern::MatchMapping(p) => {
                if let Some(rest) = &p.rest {
                    self.bound_places
                        .push(PlaceExpr::Symbol(Symbol::new(rest.id.clone())));
                }
            }
            _ => {}
        }
        walk_pattern(self, pattern);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_python_parser::parse_module;
    use ruff_python_trivia::textwrap::dedent;

    // Test collecting `while` loop bindings.

    fn collect_while_loop_place_names(code: &str) -> Vec<String> {
        let parsed = parse_module(code).expect("valid Python code");
        let stmt = &parsed.suite()[0];
        let ast::Stmt::While(while_stmt) = stmt else {
            panic!("Expected a while statement");
        };
        collect_while_loop_bindings(while_stmt)
            .into_iter()
            .map(|place| match place {
                PlaceExpr::Symbol(sym) => sym.name().to_string(),
                PlaceExpr::Member(member) => member.to_string(),
            })
            .collect()
    }

    #[test]
    fn test_collect_while_loop() {
        let bindings = collect_while_loop_place_names(&dedent(
            "
            while True:
                x = 1
                y = 2
                x = 3
            else:
                z = 4
            ",
        ));
        // `z` is not collected, because it's not visible to the loopback edge.
        assert_eq!(bindings, vec!["x", "y", "x"]);
    }

    #[test]
    fn test_collect_while_loop_nested() {
        let bindings = collect_while_loop_place_names(&dedent(
            "
            while True:
                a = 1
                if some_condition:
                    b = 2
                while some_condition:
                    c = 3
                for d in e:
                    f = 4
                [g := 42 for x in [h := 99 for _ in 'hello world']]
            ",
        ));
        // Note that "x", the comprehension variable, is not included, but "g", a walrus assignment
        // within the comprehension, is included.
        assert_eq!(bindings, vec!["a", "b", "c", "d", "f", "h", "g"]);
    }

    #[test]
    fn test_collect_while_loop_walrus_in_condition() {
        let bindings = collect_while_loop_place_names(&dedent(
            "
            while (x := get_next()):
                y = x + 1
            ",
        ));
        assert_eq!(bindings, vec!["x", "y"]);
    }

    // Test collecting `for` loop bindings.

    fn collect_for_loop_place_names(code: &str) -> Vec<String> {
        let parsed = parse_module(code).expect("valid Python code");
        let stmt = &parsed.suite()[0];
        let ast::Stmt::For(for_stmt) = stmt else {
            panic!("Expected a for statement");
        };
        collect_for_loop_bindings(for_stmt)
            .into_iter()
            .map(|place| match place {
                PlaceExpr::Symbol(sym) => sym.name().to_string(),
                PlaceExpr::Member(member) => member.to_string(),
            })
            .collect()
    }

    #[test]
    fn test_collect_for_loop() {
        let bindings = collect_for_loop_place_names(&dedent(
            "
            for i in range(10):
                x = 1
                y = 2
                x = 3
            else:
                z = 4
            ",
        ));
        // `z` is not collected, because it's not visible to the loopback edge.
        assert_eq!(bindings, vec!["i", "x", "y", "x"]);
    }

    #[test]
    fn test_collect_for_loop_nested() {
        let bindings = collect_for_loop_place_names(&dedent(
            "
            for i in range(10):
                a = 1
                if some_condition:
                    b = 2
                while some_condition:
                    c = 3
                for d in e:
                    f = 4
                [g := 42 for x in [h := 99 for _ in 'hello world']]
            ",
        ));
        // Note that "x", the comprehension variable, is not included, but "g", a walrus assignment
        // within the comprehension, is included.
        assert_eq!(bindings, vec!["i", "a", "b", "c", "d", "f", "h", "g"]);
    }

    /// `LoopBindingsVisitor` has to handle a lot of different types of bindings. Exercise all of
    /// them at least once.
    #[test]
    fn test_all_different_binding_kinds() {
        enum LoopKind {
            While,
            For,
        }
        let loop_cases = [
            ("while True:", LoopKind::While),
            ("for for_loop_var in range(1_000_000):", LoopKind::For),
            ("async for for_loop_var in range(1_000_000):", LoopKind::For),
        ];
        for (loop_header, loop_kind) in loop_cases {
            let code_snippet = dedent(&format!(
                r#"
            {loop_header}
                simple_assign = 1
                tuple_unpack_a, tuple_unpack_b = (1, 2)
                [list_unpack_a, list_unpack_b] = [1, 2]
                first, *starred_rest, last = [1, 2, 3, 4]
                obj.attr_target = 1
                obj["subscript_target"] = 1
                aug_assign += 1
                ann_assign: int = 1
                for for_target in items:
                    for_body_binding = 1
                while condition:
                    while_body_binding = 1
                with ctx() as with_var:
                    with_body_binding = 1
                with ctx() as (with_tuple_a, with_tuple_b):
                    pass
                async with ctx() as async_with_var:
                    async_with_body_binding = 1
                try:
                    try_body_binding = 1
                except Exception as exc_var:
                    except_body_binding = 1
                finally:
                    finally_binding = 1
                import mod_a
                import mod_b as mod_b_alias
                from pkg import name_c
                from pkg import name_d as name_d_alias
                def func_def(): ...
                class ClassDef: ...
                (walrus_var := 42)
                assign_with_walrus = (walrus_in_assign := 1)
                aug_assign_walrus += (walrus_in_aug_assign := 1)
                ann_assign_walrus: int = (walrus_in_ann_assign := 1)
                for walrus_for_target in (walrus_in_for_iter := items):
                    walrus_for_body = 1
                with (walrus_in_with_ctx := ctx()) as walrus_with_var:
                    walrus_with_body = 1
                match (walrus_in_match_subject := value):
                    case match_as_var:
                        match_as_body = 1
                    case _ if (walrus_in_match_guard := guard()):
                        match_guard_body = 1
                    case int() as match_as_with_pattern: ...
                    case [seq_first, *match_star_rest, seq_last]: ...
                    case {{"key": mapping_val, **match_mapping_rest}}: ...
                    case Point(class_pos_x, y=class_kw_y): ...
                    case match_or_a | match_or_b: ...
                    case [seq_a, seq_b]: ...
                    case 42 | None | True: ...
                del deleted_variable
                [list_comp_iter for list_comp_iter in range(10)]
                {{set_comp_iter for set_comp_iter in range(10)}}
                (gen_comp_iter for gen_comp_iter in range(10))
                {{dk: dv for dk, dv in items}}
                [walrus_in_list_comp := 42 for _ in range(10)]
                [a for a in (walrus_in_comp_iter := range(10))]
            "#,
            ))
            .into_owned();

            let mut expected_bindings = vec![
                "simple_assign",
                "tuple_unpack_a",
                "tuple_unpack_b",
                "list_unpack_a",
                "list_unpack_b",
                "first",
                "starred_rest",
                "last",
                "obj.attr_target",
                "obj[\"subscript_target\"]",
                "aug_assign",
                "ann_assign",
                "for_target",
                "for_body_binding",
                "while_body_binding",
                "with_var",
                "with_body_binding",
                "with_tuple_a",
                "with_tuple_b",
                "async_with_var",
                "async_with_body_binding",
                "try_body_binding",
                "exc_var",
                "except_body_binding",
                "finally_binding",
                "mod_a",
                "mod_b_alias",
                "name_c",
                "name_d_alias",
                "func_def",
                "ClassDef",
                "walrus_var",
                "assign_with_walrus",
                "walrus_in_assign",
                "aug_assign_walrus",
                "walrus_in_aug_assign",
                "ann_assign_walrus",
                "walrus_in_ann_assign",
                "walrus_for_target",
                "walrus_in_for_iter",
                "walrus_for_body",
                "walrus_in_with_ctx",
                "walrus_with_var",
                "walrus_with_body",
                "walrus_in_match_subject",
                "match_as_var",
                "match_as_body",
                "walrus_in_match_guard",
                "match_guard_body",
                "match_as_with_pattern",
                "seq_first",
                "match_star_rest",
                "seq_last",
                "match_mapping_rest",
                "mapping_val",
                "class_pos_x",
                "class_kw_y",
                "match_or_a",
                "match_or_b",
                "seq_a",
                "seq_b",
                "deleted_variable",
                // Only the LHS of walrus operators gets collected from comprehensions.
                "walrus_in_list_comp",
                "walrus_in_comp_iter",
            ];
            if matches!(loop_kind, LoopKind::For) {
                expected_bindings.insert(0, "for_loop_var");
            }

            let bindings = match loop_kind {
                LoopKind::While => collect_while_loop_place_names(&code_snippet),
                LoopKind::For => collect_for_loop_place_names(&code_snippet),
            };

            assert_eq!(bindings, expected_bindings);
        }
    }
}
