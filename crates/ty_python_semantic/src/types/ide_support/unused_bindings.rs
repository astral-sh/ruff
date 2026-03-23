use crate::Db;
use crate::semantic_index::definition::{DefinitionKind, DefinitionState};
use crate::semantic_index::place::ScopedPlaceId;
use crate::semantic_index::scope::{FileScopeId, ScopeKind};
use crate::semantic_index::semantic_index;
use crate::types::{ClassBase, ClassType};
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast::{self as ast, name::Name};
use ruff_text_size::TextRange;

/// Returns `true` for definition kinds that create user-facing bindings we consider for
/// unused-binding diagnostics.
fn should_consider_definition(kind: &DefinitionKind<'_>) -> bool {
    match kind {
        DefinitionKind::NamedExpression(_)
        | DefinitionKind::Assignment(_)
        | DefinitionKind::AnnotatedAssignment(_)
        | DefinitionKind::For(_)
        | DefinitionKind::Comprehension(_)
        | DefinitionKind::VariadicPositionalParameter(_)
        | DefinitionKind::VariadicKeywordParameter(_)
        | DefinitionKind::Parameter(_)
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

fn function_has_stub_body(function: &ast::StmtFunctionDef) -> bool {
    let suite = ruff_python_ast::helpers::body_without_leading_docstring(&function.body);

    suite.iter().all(|stmt| match stmt {
        ast::Stmt::Pass(_) => true,
        ast::Stmt::Expr(ast::StmtExpr { value, .. }) => value.is_ellipsis_literal_expr(),
        _ => false,
    })
}

fn class_defines_member_named(db: &dyn Db, class: ClassType<'_>, member_name: &str) -> bool {
    let Some((class_literal, specialization)) = class.static_class_literal(db) else {
        // If we cannot inspect class members precisely, be conservative and avoid false positives.
        return true;
    };

    let class_scope = class_literal.body_scope(db);
    let class_place_table = crate::semantic_index::place_table(db, class_scope);

    class_place_table
        .symbol_id(member_name)
        .is_some_and(|symbol_id| {
            let symbol = class_place_table.symbol(symbol_id);
            symbol.is_bound() || symbol.is_declared()
        })
        || class_literal
            .own_synthesized_member(db, specialization, None, member_name)
            .is_some()
}

// Returns true if a superclass in the method's MRO defines the same method name.
// Used to suppress unused-parameter diagnostics for likely overrides.
fn method_name_exists_in_superclass(
    db: &dyn Db,
    index: &crate::semantic_index::SemanticIndex<'_>,
    parsed: &ParsedModuleRef,
    file_scope_id: FileScopeId,
) -> bool {
    let scope = index.scope(file_scope_id);
    let Some(function) = scope.node().as_function() else {
        return false;
    };

    let Some(class_definition) = index.class_definition_of_method(file_scope_id) else {
        return false;
    };

    let method_name = function.node(parsed).name.as_str();
    let Some(class_type) = crate::types::binding_type(db, class_definition).to_class_type(db)
    else {
        return false;
    };

    class_type
        .iter_mro(db)
        .skip(1)
        .any(|class_base| match class_base {
            ClassBase::Protocol | ClassBase::Generic | ClassBase::TypedDict => false,
            ClassBase::Dynamic(_) => true,
            ClassBase::Class(superclass) => class_defines_member_named(db, superclass, method_name),
        })
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct UnusedBinding {
    pub range: TextRange,
    pub name: Name,
}

/// Collects unused local bindings for IDE-facing diagnostics.
///
/// This intentionally reports only function-, lambda-, and comprehension-scope bindings.
/// Even with local checks such as override detection, module- and class-scope bindings
/// can still be observed indirectly (for example via imports or attribute access), so
/// reporting them here would risk false positives without broader reference analysis.
#[salsa::tracked(returns(ref))]
pub fn unused_bindings(db: &dyn Db, file: ruff_db::files::File) -> Vec<UnusedBinding> {
    let parsed = parsed_module(db, file).load(db);
    if !parsed.errors().is_empty() {
        return Vec::new();
    }

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
            && scope
                .node()
                .as_function()
                .is_some_and(|function| function_has_stub_body(function.node(&parsed)));
        let place_table = index.place_table(file_scope_id);
        let use_def_map = index.use_def_map(file_scope_id);
        let mut skip_unused_parameters_for_override = None;

        for (_, state, is_used) in use_def_map.all_definitions_with_usage() {
            let DefinitionState::Defined(definition) = state else {
                continue;
            };

            if is_used {
                continue;
            }

            let kind = definition.kind(db);
            if !should_consider_definition(kind) {
                continue;
            }

            let is_parameter = kind.is_parameter_def();

            if is_parameter
                && (is_stub_file
                    || method_has_stub_body
                    || *skip_unused_parameters_for_override.get_or_insert_with(|| {
                        method_name_exists_in_superclass(db, index, &parsed, file_scope_id)
                    }))
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
    fn skips_unused_parameter_for_overriding_method() -> anyhow::Result<()> {
        let source = dedent(
            "
            class Test:
                def a(self, bar):
                    print(bar)

            class Test2(Test):
                def a(self, bar):
                    ...
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn skips_unused_parameter_for_indirect_override() -> anyhow::Result<()> {
        let source = dedent(
            "
            class A:
                def a(self, bar):
                    print(bar)

            class B(A):
                pass

            class C(B):
                def a(self, bar):
                    ...
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, Vec::<String>::new());
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
        assert_eq!(names, vec!["local_dead"]);
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
    fn skips_unused_binding_analysis_on_syntax_error() -> anyhow::Result<()> {
        let source = dedent(
            "
            def f(
                x = 1
            ",
        );

        let names = collect_unused_names(&source)?;
        assert_eq!(names, Vec::<String>::new());
        Ok(())
    }
}
