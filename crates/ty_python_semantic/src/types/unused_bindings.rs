//! Collects unused local bindings for IDE-facing diagnostics.
//!
//! This intentionally reports only function-, lambda-, and comprehension-scope bindings.
//! Module and class bindings can be observed indirectly (e.g., imports, attribute access), so
//! reporting them here risks false positives without cross-file/reference analysis.

use crate::semantic_index::definition::{DefinitionKind, DefinitionState};
use crate::semantic_index::place::ScopedPlaceId;
use crate::semantic_index::scope::ScopeKind;
use crate::{Db, semantic_index};
use ruff_db::parsed::parsed_module;
use ruff_text_size::TextRange;

fn is_dunder_name(name: &str) -> bool {
    name.len() > 4 && name.starts_with("__") && name.ends_with("__")
}

fn should_mark_unnecessary(scope_kind: ScopeKind, name: &str) -> bool {
    if name.starts_with('_') || matches!(name, "self" | "cls") || is_dunder_name(name) {
        return false;
    }

    match scope_kind {
        ScopeKind::Function | ScopeKind::Lambda | ScopeKind::Comprehension => true,
        ScopeKind::Module | ScopeKind::Class | ScopeKind::TypeParams | ScopeKind::TypeAlias => {
            false
        }
    }
}

fn should_consider_definition(kind: &DefinitionKind<'_>) -> bool {
    matches!(
        kind,
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
            | DefinitionKind::ExceptHandler(_)
    )
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct UnusedBinding {
    pub range: TextRange,
    pub name: String,
}

#[salsa::tracked(returns(ref))]
pub fn unused_bindings(db: &dyn Db, file: ruff_db::files::File) -> Vec<UnusedBinding> {
    let parsed = parsed_module(db, file).load(db);
    if !parsed.errors().is_empty() || !parsed.unsupported_syntax_errors().is_empty() {
        return Vec::new();
    }

    let index = semantic_index::semantic_index(db, file);
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

        let place_table = index.place_table(file_scope_id);
        let use_def_map = index.use_def_map(file_scope_id);

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

            let ScopedPlaceId::Symbol(symbol_id) = definition.place(db) else {
                continue;
            };

            let symbol = place_table.symbol(symbol_id);
            let name = symbol.name().as_str();

            if !should_mark_unnecessary(scope_kind, name) {
                continue;
            }

            // Global and nonlocal assignments target bindings from outer scopes.
            // Treat them as externally managed to avoid false positives here.
            if symbol.is_global() || symbol.is_nonlocal() {
                continue;
            }

            let Some(range) = kind.binding_name_range(&parsed) else {
                continue;
            };

            unused.push(UnusedBinding {
                range,
                name: name.to_string(),
            });
        }
    }

    unused.sort_unstable_by_key(|binding| (binding.range.start(), binding.range.end()));
    unused.dedup_by_key(|binding| binding.range);

    unused
}

#[cfg(test)]
mod tests {
    use super::unused_bindings;
    use crate::db::tests::TestDbBuilder;
    use ruff_db::files::system_path_to_file;

    fn collect_unused_names(source: &str) -> anyhow::Result<Vec<String>> {
        let db = TestDbBuilder::new()
            .with_file("/src/main.py", source)
            .build()?;
        let file = system_path_to_file(&db, "/src/main.py").unwrap();
        let mut names = unused_bindings(&db, file)
            .iter()
            .map(|binding| binding.name.clone())
            .collect::<Vec<_>>();
        names.sort();
        Ok(names)
    }

    #[test]
    fn captures_safe_local_binding_kinds() -> anyhow::Result<()> {
        let source = r#"def f():
    used_assign, dead_assign = (1, 2)
    print(used_assign)

    for used_loop, dead_loop in [(1, 2)]:
        print(used_loop)

    with open("x") as dead_with:
        pass

    try:
        1 / 0
    except Exception as dead_exc:
        pass

    if (dead_walrus := 1):
        pass

    [1 for dead_comp in range(3)]
    [ok_comp for ok_comp, dead_comp2 in [(1, 2)]]

    match {"x": 1, "y": 2}:
        case {"x": used_match, **dead_rest}:
            print(used_match)
        case [used_star, *dead_star] as dead_as:
            print(used_star)
"#;

        let names = collect_unused_names(source)?;
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
    fn skips_module_class_placeholder_and_dunder_bindings() -> anyhow::Result<()> {
        let source = r#"_module_dead = 1

class C:
    __private_dead = 1

    def method(self):
        local_dead = 1
        _ = 2
        __dunder__ = 3
        return 0
"#;

        let names = collect_unused_names(source)?;
        assert_eq!(names, vec!["local_dead"]);
        Ok(())
    }

    #[test]
    fn skips_global_and_nonlocal_assignments() -> anyhow::Result<()> {
        let source = r#"global_value = 0

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
"#;

        let names = collect_unused_names(source)?;
        assert_eq!(names, vec!["local_dead"]);
        Ok(())
    }

    #[test]
    fn captures_unused_function_and_lambda_parameters() -> anyhow::Result<()> {
        let source = r#"def fn(used, dead, _ignored, __dunder__):
    return used

def fn_defaults(a, b=1, *, c=2, d):
    return a + d

lam = lambda x, y, z=1: x + z
"#;

        let names = collect_unused_names(source)?;
        assert_eq!(names, vec!["b", "c", "dead", "y"]);
        Ok(())
    }
}
