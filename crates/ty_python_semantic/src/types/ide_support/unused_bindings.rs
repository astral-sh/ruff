use crate::Db;
use crate::reachability::is_reachable;
use crate::types::function::FunctionDecorators;
use crate::types::infer::function_known_decorator_flags;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use ruff_text_size::TextRange;
use rustc_hash::FxHashSet;
use ty_python_core::definition::{DefinitionKind, DefinitionState};
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::scope::{FileScopeId, ScopeKind};
use ty_python_core::{SemanticIndex, get_loop_header, semantic_index};

/// Returns `true` for definition kinds that create user-facing bindings we consider for
/// unused-binding diagnostics.
fn should_consider_definition(kind: &DefinitionKind<'_>) -> bool {
    match kind {
        DefinitionKind::NamedExpression(_)
        | DefinitionKind::Assignment(_)
        | DefinitionKind::AnnotatedAssignment(_)
        | DefinitionKind::For(_)
        | DefinitionKind::Comprehension(_)
        | DefinitionKind::Parameter(_)
        | DefinitionKind::LambdaParameter { .. }
        | DefinitionKind::WithItem(_)
        | DefinitionKind::MatchPattern(_)
        | DefinitionKind::ExceptHandler(_) => true,

        DefinitionKind::Import(_)
        | DefinitionKind::ImportFrom(_)
        | DefinitionKind::ImportFromSubmodule(_)
        | DefinitionKind::StarImport(_)
        | DefinitionKind::Function(_)
        | DefinitionKind::Class(_)
        | DefinitionKind::TypeAlias(_)
        | DefinitionKind::AugmentedAssignment(_)
        | DefinitionKind::DictKeyAssignment(_)
        | DefinitionKind::TypeVar(_)
        | DefinitionKind::ParamSpec(_)
        | DefinitionKind::TypeVarTuple(_)
        | DefinitionKind::LoopHeader(_) => false,
    }
}

fn function_scope_is_overload_declaration(
    db: &dyn Db,
    index: &SemanticIndex<'_>,
    file_scope_id: FileScopeId,
) -> bool {
    let scope = index.scope(file_scope_id);
    let Some(function) = scope.node().as_function() else {
        return false;
    };

    let definition = index.expect_single_definition(function);
    function_known_decorator_flags(db, definition).contains(FunctionDecorators::OVERLOAD)
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct UnusedBinding {
    pub range: TextRange,
    pub name: Name,
}

/// Collects unused local bindings for IDE-facing diagnostics.
///
/// This intentionally reports only function-, lambda-, and comprehension-scope bindings.
/// Module- and class-scope bindings can still be observed indirectly (for example via
/// imports or attribute access), so reporting them here would risk false positives
/// without broader reference analysis.
#[salsa::tracked(returns(ref))]
pub fn unused_bindings(db: &dyn Db, file: ruff_db::files::File) -> Vec<UnusedBinding> {
    let parsed = parsed_module(db, file).load(db);
    let is_stub_file = file.is_stub(db);
    let index = semantic_index(db, file);
    let mut unused = Vec::new();

    for scope_id in index.scope_ids() {
        let file_scope_id = scope_id.file_scope_id(db);
        let scope = index.scope(file_scope_id);
        let scope_kind = scope.kind();

        if !matches!(
            scope_kind,
            ScopeKind::Function | ScopeKind::Lambda | ScopeKind::Comprehension
        ) {
            continue;
        }

        let is_method_scope = index.class_definition_of_method(file_scope_id).is_some();
        let method_has_stub_body = is_method_scope
            && scope.node().as_function().is_some_and(|function| {
                crate::types::function::function_has_stub_body(function.node(&parsed))
            });
        let function_is_overload_declaration =
            function_scope_is_overload_declaration(db, index, file_scope_id);
        let place_table = index.place_table(file_scope_id);
        let use_def_map = index.use_def_map(file_scope_id);
        // Loop headers are synthesized before the loop body definitions they point to;
        // track used IDs as we go.
        let mut loop_header_used_definition_ids = FxHashSet::default();

        for (definition_id, state, is_used) in use_def_map.all_definitions_with_usage() {
            let DefinitionState::Defined(definition) = state else {
                continue;
            };

            if is_used {
                let DefinitionKind::LoopHeader(loop_header_definition) = definition.kind(db) else {
                    continue;
                };

                let loop_header = get_loop_header(db, loop_header_definition.loop_token());
                for live_binding in loop_header.bindings_for_place(loop_header_definition.place()) {
                    if is_reachable(db, use_def_map, live_binding.reachability_constraint()) {
                        loop_header_used_definition_ids.insert(live_binding.binding());
                    }
                }

                continue;
            }

            if loop_header_used_definition_ids.contains(&definition_id) {
                continue;
            }

            let kind = definition.kind(db);
            if !should_consider_definition(kind) {
                continue;
            }

            let is_parameter = kind.is_parameter_def();

            if is_parameter
                && (is_stub_file || function_is_overload_declaration || method_has_stub_body)
            {
                continue;
            }

            let ScopedPlaceId::Symbol(symbol_id) = definition.place(db) else {
                continue;
            };

            let symbol = place_table.symbol(symbol_id);
            let name = symbol.name().as_str();

            // Skip conventional method receiver parameters.
            if is_parameter && is_method_scope && matches!(name, "self" | "cls") {
                continue;
            }

            if name.starts_with('_') {
                continue;
            }

            // Global and nonlocal assignments target bindings from outer scopes.
            // Treat them as externally managed to avoid false positives here.
            if symbol.is_global() || symbol.is_nonlocal() {
                continue;
            }

            let range = kind.target_range(&parsed);

            unused.push(UnusedBinding {
                range,
                name: symbol.name().clone(),
            });
        }
    }

    unused.sort_unstable_by_key(|binding| (binding.range.start(), binding.range.end()));
    unused.dedup_by_key(|binding| binding.range);

    unused
}

#[cfg(test)]
mod tests {
    use super::{UnusedBinding, unused_bindings};
    use crate::db::tests::TestDbBuilder;
    use ruff_db::files::system_path_to_file;
    use ruff_python_ast::name::Name;
    use ruff_python_trivia::textwrap::dedent;
    use ruff_text_size::{TextRange, TextSize};

    fn collect_unused_bindings_in_file(
        path: &str,
        source: &str,
    ) -> anyhow::Result<Vec<UnusedBinding>> {
        let db = TestDbBuilder::new().with_file(path, source).build()?;
        let file = system_path_to_file(&db, path).unwrap();
        let mut bindings = unused_bindings(&db, file).clone();
        bindings.sort_unstable_by_key(|binding| (binding.range.start(), binding.range.end()));
        Ok(bindings)
    }

    fn collect_unused_bindings(source: &str) -> anyhow::Result<Vec<UnusedBinding>> {
        collect_unused_bindings_in_file("/src/main.py", source)
    }

    fn collect_unused_names_in_file(path: &str, source: &str) -> anyhow::Result<Vec<String>> {
        let mut names = collect_unused_bindings_in_file(path, source)?
            .iter()
            .map(|binding| binding.name.to_string())
            .collect::<Vec<_>>();
        names.sort();
        Ok(names)
    }

    fn collect_unused_names(source: &str) -> anyhow::Result<Vec<String>> {
        collect_unused_names_in_file("/src/main.py", source)
    }

    #[test]
    fn captures_safe_local_binding_kinds() -> anyhow::Result<()> {
        let source = dedent(
            "
            def f():
                used_assign, dead_assign = (1, 2)
                print(used_assign)

                for used_loop, dead_loop in [(1, 2)]:
                    print(used_loop)

                with open(\"x\") as dead_with:
                    pass

                try:
                    1 / 0
                except Exception as dead_exc:
                    pass

                if (dead_walrus := 1):
                    pass

                [1 for dead_comp in range(3)]
                [ok_comp for ok_comp, dead_comp2 in [(1, 2)]]

                match {\"x\": 1, \"y\": 2}:
                    case {\"x\": used_match, **dead_rest}:
                        print(used_match)
                    case [used_star, *dead_star] as dead_as:
                        print(used_star)
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(
            names,
            vec![
                "dead_as",
                "dead_assign",
                "dead_comp",
                "dead_comp2",
                "dead_exc",
                "dead_loop",
                "dead_rest",
                "dead_star",
                "dead_walrus",
                "dead_with",
            ]
        );
        Ok(())
    }

    #[test]
    fn skips_module_and_class_scope_bindings() -> anyhow::Result<()> {
        let source = dedent(
            "
            module_dead = 1

            class C:
                class_dead = 1

                def method(self):
                    local_dead = 1
                    return 0
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, vec!["local_dead"]);
        Ok(())
    }

    #[test]
    fn skips_placeholder_and_dunder_locals() -> anyhow::Result<()> {
        let source = dedent(
            "
            def f():
                local_dead = 1
                _ = 2
                _ignored = 3
                __dunder__ = 4
                return 0
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, vec!["local_dead"]);
        Ok(())
    }

    #[test]
    fn skips_global_and_nonlocal_assignments() -> anyhow::Result<()> {
        let source = dedent(
            "
            global_value = 0

            def mutate_global():
                global global_value
                global_value = 1
                local_dead = 1

            def outer():
                captured = 0

                def inner():
                    nonlocal captured
                    captured = 1

                inner()
                return captured
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, vec!["local_dead"]);
        Ok(())
    }

    #[test]
    fn reports_unused_parameter_for_overriding_method() -> anyhow::Result<()> {
        let source = dedent(
            "
            class Test:
                def a(self, bar):
                    print(bar)

            class Test2(Test):
                def a(self, bar):
                    return 0
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, vec!["bar"]);
        Ok(())
    }

    #[test]
    fn reports_unused_parameter_for_indirect_override() -> anyhow::Result<()> {
        let source = dedent(
            "
            class A:
                def a(self, bar):
                    print(bar)

            class B(A):
                pass

            class C(B):
                def a(self, bar):
                    return 0
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, vec!["bar"]);
        Ok(())
    }

    #[test]
    fn reports_unused_parameter_for_non_overriding_method() -> anyhow::Result<()> {
        let source = dedent(
            "
            class Base:
                def keep(self):
                    return 0

            class Child(Base):
                def new_method(self, dead):
                    return 1
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, vec!["dead"]);
        Ok(())
    }

    #[test]
    fn overriding_method_reports_unused_local_bindings() -> anyhow::Result<()> {
        let source = dedent(
            "
            class Base:
                def a(self, bar):
                    print(bar)

            class Child(Base):
                def a(self, bar):
                    local_dead = 1
                    return 0
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, vec!["bar", "local_dead"]);
        Ok(())
    }

    #[test]
    fn skips_unused_parameter_for_method_with_stub_body() -> anyhow::Result<()> {
        let source = dedent(
            "
            class Test:
                def a(self, bar):
                    ...

                def b(self, baz):
                    pass
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_unused_parameter_for_overload_stub_declarations() -> anyhow::Result<()> {
        let source = dedent(
            "
            import typing

            class Test:
                @typing.overload
                def a(self, bar: str): ...

                @typing.overload
                def a(self, bar: int) -> None:
                    ...

                def a(self, bar: str | int) -> None:
                    print(bar)
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_unused_parameter_for_module_level_overload_stub_declarations() -> anyhow::Result<()> {
        let source = dedent(
            "
            import typing

            @typing.overload
            def f(x: str) -> str: ...

            @typing.overload
            def f(x: int) -> int:
                ...

            def f(x: str | int) -> str | int:
                return x
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_unused_parameters_in_stub_files() -> anyhow::Result<()> {
        let source = dedent(
            "
            def f(x, y) -> None: ...

            class C:
                def m(self, z) -> None: ...
            ",
        );

        let names = collect_unused_names_in_file("/src/main.pyi", &source)?;
        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn captures_unused_function_and_lambda_parameters() -> anyhow::Result<()> {
        let source = dedent(
            "
            def fn(used, dead, _ignored, __dunder__):
                return used

            def fn_defaults(a, b=1, *, c=2, d):
                return a + d

            lam = lambda x, y, z=1: x + z
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, vec!["b", "c", "dead", "y"]);
        Ok(())
    }

    #[test]
    fn reports_non_parameter_self_and_cls_bindings() -> anyhow::Result<()> {
        let source = dedent(
            "
            def f(xs):
                self = 1
                [0 for cls in xs]
                return 0
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, vec!["cls", "self"]);
        Ok(())
    }

    #[test]
    fn skips_closure_captured_bindings() -> anyhow::Result<()> {
        let source = dedent(
            "
            def outer(flag: bool):
                captured = 1
                dead = 2

                def inner():
                    return captured

                if flag:
                    captured = 3

                return inner
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, vec!["dead"]);
        Ok(())
    }

    #[test]
    fn closure_uses_nearest_shadowed_binding() -> anyhow::Result<()> {
        let source = dedent(
            "
            def outer():
                x = 0

                def mid():
                    x = 1

                    def inner():
                        return x

                    return inner

                return mid
            ",
        );

        let bindings = collect_unused_bindings(&source)?;
        let outer_x_start = TextSize::try_from(source.find("x = 0").unwrap()).unwrap();
        assert_eq!(
            bindings,
            vec![UnusedBinding {
                range: TextRange::new(outer_x_start, outer_x_start + TextSize::new(1)),
                name: Name::new("x"),
            }]
        );
        Ok(())
    }

    #[test]
    fn nonlocal_proxy_scope_still_marks_outer_binding_used() -> anyhow::Result<()> {
        let source = dedent(
            "
            def outer():
                x = 1

                def mid():
                    nonlocal x
                    x = 2

                    def inner():
                        return x

                    return inner

                return mid
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn nested_local_same_name_does_not_mark_outer_used() -> anyhow::Result<()> {
        let source = dedent(
            "
            def outer():
                x = 1

                def inner():
                    x = 2
                    return x

                return inner
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, vec!["x"]);
        Ok(())
    }

    #[test]
    fn comprehension_binding_captured_by_nested_lambda_is_used() -> anyhow::Result<()> {
        let source = dedent(
            "
            def f():
                funcs = [lambda: x for x in range(3)]
                return funcs
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn reports_unused_binding_on_syntax_error() -> anyhow::Result<()> {
        let source = dedent(
            "
            def f(
                x = 1
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, vec!["x"]);
        Ok(())
    }

    #[test]
    fn does_not_report_used_parameter_on_syntax_error() -> anyhow::Result<()> {
        let source = dedent(
            "
            def f(x
                return x
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_loop_carried_rebinding() -> anyhow::Result<()> {
        let source = dedent(
            "
            def buy_sell_once(prices: list[float]) -> float:
                assert len(prices) > 1
                best_buy, best_so_far = prices[0], 0.0
                for i in range(1, len(prices)):
                    best_so_far = max(best_so_far, prices[i] - best_buy)
                    best_buy = min(best_buy, prices[i])
                return best_so_far
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_unreachable_loop_carried_rebinding() -> anyhow::Result<()> {
        let source = dedent(
            "
            def f():
                value = 0
                for _ in range(3):
                    print(value)
                    if False:
                        value = 1
            ",
        );

        let bindings = collect_unused_bindings(&source)?;
        let value_start = TextSize::try_from(source.rfind("value = 1").unwrap()).unwrap();
        assert_eq!(
            bindings,
            vec![UnusedBinding {
                range: TextRange::new(value_start, value_start + TextSize::new(5)),
                name: Name::new("value"),
            }]
        );
        Ok(())
    }

    #[test]
    fn skips_loop_condition_guarded_rebinding() -> anyhow::Result<()> {
        let source = dedent(
            "
            def f():
                flag = True
                while flag:
                    print(x)
                    x = 1
                    flag = False
                x = 2
            ",
        );

        let bindings = collect_unused_bindings(&source)?;
        let final_x_start = TextSize::try_from(source.rfind("x = 2").unwrap()).unwrap();
        // TODO: The `x = 1` binding is also unused, but we currently mark it used because it
        // reaches the synthetic loop header even though the next loop iteration is blocked by the
        // loop condition.
        assert_eq!(
            bindings,
            vec![UnusedBinding {
                range: TextRange::new(final_x_start, final_x_start + TextSize::new(1)),
                name: Name::new("x"),
            }]
        );
        Ok(())
    }
}
