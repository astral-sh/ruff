use crate::Db;
use crate::semantic_index::{
    SemanticIndex,
    scope::{FileScopeId, NodeWithScopeKind, ScopeId},
    semantic_index,
};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_text_size::{Ranged, TextRange};

/// Collects merged structural unreachable ranges for IDE-facing diagnostics.
///
/// This includes both unconditionally unreachable scopes and unconditionally unreachable
/// basic-block ranges within otherwise reachable scopes.
#[salsa::tracked(returns(ref))]
pub fn unreachable_ranges(db: &dyn Db, file: File) -> Vec<TextRange> {
    let index = semantic_index(db, file);
    let mut unreachable = Vec::new();

    for scope_id in index.scope_ids() {
        let file_scope_id = scope_id.file_scope_id(db);
        if is_scope_unconditionally_unreachable(index, file_scope_id) {
            if index
                .parent_scope_id(file_scope_id)
                .is_none_or(|parent_scope_id| {
                    !is_scope_unconditionally_unreachable(index, parent_scope_id)
                })
                && let Some(range) = scope_range(db, scope_id)
            {
                unreachable.push(range);
            }
            continue;
        }

        if !index.is_scope_reachable(db, file_scope_id) {
            continue;
        }

        unreachable.extend(
            index
                .use_def_map(file_scope_id)
                .unconditionally_unreachable_ranges(),
        );
    }

    merge_ranges(unreachable)
}

fn is_scope_unconditionally_unreachable(index: &SemanticIndex<'_>, scope_id: FileScopeId) -> bool {
    index
        .parent_scope_id(scope_id)
        .is_some_and(|parent_scope_id| is_scope_unconditionally_unreachable(index, parent_scope_id))
        || index.scope(scope_id).is_unconditionally_unreachable()
}

fn scope_range(db: &dyn Db, scope_id: ScopeId<'_>) -> Option<TextRange> {
    let parsed = parsed_module(db, scope_id.file(db)).load(db);

    Some(match scope_id.node(db) {
        NodeWithScopeKind::Module => return None,
        NodeWithScopeKind::Class(class) | NodeWithScopeKind::ClassTypeParameters(class) => {
            class.node(&parsed).range()
        }
        NodeWithScopeKind::Function(function)
        | NodeWithScopeKind::FunctionTypeParameters(function) => function.node(&parsed).range(),
        NodeWithScopeKind::TypeAlias(type_alias)
        | NodeWithScopeKind::TypeAliasTypeParameters(type_alias) => {
            type_alias.node(&parsed).range()
        }
        NodeWithScopeKind::Lambda(lambda) => lambda.node(&parsed).range(),
        NodeWithScopeKind::ListComprehension(comprehension) => comprehension.node(&parsed).range(),
        NodeWithScopeKind::SetComprehension(comprehension) => comprehension.node(&parsed).range(),
        NodeWithScopeKind::DictComprehension(comprehension) => comprehension.node(&parsed).range(),
        NodeWithScopeKind::GeneratorExpression(generator) => generator.node(&parsed).range(),
    })
}

fn merge_ranges(mut ranges: Vec<TextRange>) -> Vec<TextRange> {
    ranges.sort_unstable_by_key(|range| (range.start(), range.end()));

    let mut merged: Vec<TextRange> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(previous) = merged.last_mut()
            && range.start() <= previous.end()
        {
            *previous = TextRange::new(previous.start(), previous.end().max(range.end()));
            continue;
        }

        merged.push(range);
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::unreachable_ranges;
    use crate::db::tests::TestDbBuilder;
    use ruff_db::files::system_path_to_file;
    use ruff_python_ast::PythonVersion;
    use ruff_python_trivia::textwrap::dedent;

    fn collect_unreachable_snippets_with_db(
        db: &crate::db::tests::TestDb,
        path: &str,
        source: &str,
    ) -> Vec<String> {
        let file = system_path_to_file(db, path).unwrap();
        let mut snippets = unreachable_ranges(db, file)
            .iter()
            .map(|range| {
                source[usize::from(range.start())..usize::from(range.end())]
                    .trim()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        snippets.sort();
        snippets
    }

    fn collect_unreachable_snippets(source: &str) -> anyhow::Result<Vec<String>> {
        let db = TestDbBuilder::new()
            .with_file("/src/main.py", source)
            .build()?;
        Ok(collect_unreachable_snippets_with_db(
            &db,
            "/src/main.py",
            source,
        ))
    }

    #[test]
    fn reports_statement_after_return() -> anyhow::Result<()> {
        let source = dedent(
            "
            def f():
                return 1
                print(\"dead\")
            ",
        );

        let snippets = collect_unreachable_snippets(&source)?;
        assert_eq!(snippets, vec!["print(\"dead\")"]);
        Ok(())
    }

    #[test]
    fn merges_consecutive_unreachable_statements() -> anyhow::Result<()> {
        let source = dedent(
            "
            def f():
                return 1
                print(\"dead\")
                print(\"still dead\")
            ",
        );

        let snippets = collect_unreachable_snippets(&source)?;
        assert_eq!(snippets, vec!["print(\"dead\")\n    print(\"still dead\")"]);
        Ok(())
    }

    #[test]
    fn reports_false_branch_statement() -> anyhow::Result<()> {
        let source = dedent(
            "
            if False:
                print(\"dead\")
            ",
        );

        let snippets = collect_unreachable_snippets(&source)?;
        assert_eq!(snippets, vec!["print(\"dead\")"]);
        Ok(())
    }

    #[test]
    fn merges_unreachable_scope_range_into_enclosing_block() -> anyhow::Result<()> {
        let source = dedent(
            "
            if False:
                x = lambda: 1
            ",
        );

        let snippets = collect_unreachable_snippets(&source)?;
        assert_eq!(snippets, vec!["x = lambda: 1"]);
        Ok(())
    }

    #[test]
    fn reports_statement_after_raise() -> anyhow::Result<()> {
        let source = dedent(
            "
            def f():
                raise RuntimeError()
                print(\"dead\")
            ",
        );

        let snippets = collect_unreachable_snippets(&source)?;
        assert_eq!(snippets, vec!["print(\"dead\")"]);
        Ok(())
    }

    #[test]
    fn reports_statement_after_break() -> anyhow::Result<()> {
        let source = dedent(
            "
            def f():
                while True:
                    break
                    print(\"dead\")
            ",
        );

        let snippets = collect_unreachable_snippets(&source)?;
        assert_eq!(snippets, vec!["print(\"dead\")"]);
        Ok(())
    }

    #[test]
    fn reports_statement_after_continue() -> anyhow::Result<()> {
        let source = dedent(
            "
            def f():
                for _ in range(1):
                    continue
                    print(\"dead\")
            ",
        );

        let snippets = collect_unreachable_snippets(&source)?;
        assert_eq!(snippets, vec!["print(\"dead\")"]);
        Ok(())
    }

    #[test]
    fn reports_else_branch_after_true_condition() -> anyhow::Result<()> {
        let source = dedent(
            "
            if True:
                pass
            else:
                print(\"dead\")
            ",
        );

        let snippets = collect_unreachable_snippets(&source)?;
        assert_eq!(snippets, vec!["print(\"dead\")"]);
        Ok(())
    }

    #[test]
    fn reports_statement_in_unreachable_elif_branch() -> anyhow::Result<()> {
        let source = dedent(
            "
            if True:
                pass
            elif False:
                print(\"dead\")
            ",
        );

        let snippets = collect_unreachable_snippets(&source)?;
        assert_eq!(snippets, vec!["print(\"dead\")"]);
        Ok(())
    }

    #[test]
    fn skips_version_guarded_branch() -> anyhow::Result<()> {
        let source = dedent(
            "
            import sys

            if sys.version_info >= (3, 11):
                from typing import Self
            ",
        );

        let db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY310)
            .with_file("/src/main.py", &source)
            .build()?;

        let snippets = collect_unreachable_snippets_with_db(&db, "/src/main.py", &source);
        assert_eq!(snippets, Vec::<String>::new());
        Ok(())
    }
}
