use crate::Db;
use crate::reachability::is_reachable;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_text_size::TextRange;
use ty_python_core::reachability_constraints::ScopedReachabilityConstraintId;
use ty_python_core::{FileScopeId, SemanticIndex, semantic_index};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct UnreachableRange {
    pub range: TextRange,
    pub kind: UnreachableKind,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum UnreachableKind {
    Unconditional,
    CurrentAnalysis,
}

fn unreachable_kind(is_unconditional: bool) -> UnreachableKind {
    if is_unconditional {
        UnreachableKind::Unconditional
    } else {
        UnreachableKind::CurrentAnalysis
    }
}

/// Returns merged unreachable ranges for unnecessary-code hints.
///
/// Includes both unreachable scopes and unreachable intra-scope ranges.
/// `ALWAYS_FALSE` ranges are classified as unconditional; all others are
/// unreachable only under the current analysis.
#[salsa::tracked(returns(ref))]
pub fn unreachable_ranges(db: &dyn Db, file: File) -> Vec<UnreachableRange> {
    let index = semantic_index(db, file);
    let parsed = parsed_module(db, file).load(db);
    let mut unreachable = Vec::new();

    for scope_id in index.scope_ids() {
        let file_scope_id = scope_id.file_scope_id(db);

        if let Some(range) = scope_id.node(db).node_range(&parsed)
            && let Some(kind) = range_unreachable_kind(db, index, file_scope_id, range)
        {
            unreachable.push(UnreachableRange { range, kind });
        }

        let use_def = index.use_def_map(file_scope_id);
        unreachable.extend(
            use_def
                .range_reachability()
                .filter_map(|(range, constraint)| {
                    (!is_reachable(db, use_def, constraint)).then_some(UnreachableRange {
                        range,
                        kind: unreachable_kind(
                            constraint == ScopedReachabilityConstraintId::ALWAYS_FALSE,
                        ),
                    })
                }),
        );
    }

    merge_ranges(unreachable)
}

fn range_unreachable_kind<'db>(
    db: &'db dyn Db,
    index: &SemanticIndex<'db>,
    scope_id: FileScopeId,
    range: TextRange,
) -> Option<UnreachableKind> {
    let mut kind: Option<UnreachableKind> = None;

    for (ancestor_scope_id, _) in index.ancestor_scopes(scope_id) {
        let use_def = index.use_def_map(ancestor_scope_id);

        for (entry_range, constraint) in use_def.range_reachability() {
            if entry_range.contains_range(range) && !is_reachable(db, use_def, constraint) {
                let entry_kind =
                    unreachable_kind(constraint == ScopedReachabilityConstraintId::ALWAYS_FALSE);
                kind = Some(kind.map_or(entry_kind, |kind| kind.max(entry_kind)));
            }
        }
    }

    kind
}

fn merge_ranges(mut ranges: Vec<UnreachableRange>) -> Vec<UnreachableRange> {
    ranges.sort_unstable_by_key(|range| (range.range.start(), range.range.end(), range.kind));

    let mut merged: Vec<UnreachableRange> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(previous) = merged.last_mut()
            && range.range.start() <= previous.range.end()
        {
            // Keep merely-adjacent ranges with different reachability kinds separate.
            // Unlike overlapping ranges, there is no shared span that forces us to
            // collapse them to a single user-facing message.
            let touches_without_overlap =
                range.kind != previous.kind && range.range.start() == previous.range.end();

            if !touches_without_overlap {
                previous.range = TextRange::new(
                    previous.range.start(),
                    previous.range.end().max(range.range.end()),
                );
                previous.kind = previous.kind.max(range.kind);
                continue;
            }
        }

        merged.push(range);
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::{UnreachableKind, unreachable_ranges};
    use crate::db::tests::TestDbBuilder;
    use ruff_db::files::system_path_to_file;
    use ruff_python_ast::PythonVersion;
    use ruff_python_trivia::textwrap::dedent;

    fn collect_unreachable_entries(
        db: &crate::db::tests::TestDb,
        path: &str,
        source: &str,
    ) -> Vec<(String, UnreachableKind)> {
        let file = system_path_to_file(db, path).unwrap();
        let mut entries = unreachable_ranges(db, file)
            .iter()
            .map(|range| {
                (
                    source[usize::from(range.range.start())..usize::from(range.range.end())]
                        .trim()
                        .to_owned(),
                    range.kind,
                )
            })
            .collect::<Vec<_>>();
        entries.sort();
        entries
    }

    fn collect_unreachable_snippets(source: &str) -> anyhow::Result<Vec<String>> {
        let db = TestDbBuilder::new()
            .with_file("/src/main.py", source)
            .build()?;

        Ok(collect_unreachable_entries(&db, "/src/main.py", source)
            .into_iter()
            .map(|(snippet, _)| snippet)
            .collect())
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
    fn keeps_reachable_code_before_return_out_of_results() -> anyhow::Result<()> {
        let source = dedent(
            "
            def f():
                x = 1
                return x
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
    fn reports_statement_after_raise_inside_try() -> anyhow::Result<()> {
        let source = dedent(
            "
            def f():
                try:
                    raise ValueError()
                    print(\"dead\")
                except ValueError:
                    pass
            ",
        );

        let snippets = collect_unreachable_snippets(&source)?;
        assert_eq!(snippets, vec!["print(\"dead\")"]);
        Ok(())
    }

    #[test]
    fn reports_statement_after_assert_false() -> anyhow::Result<()> {
        let source = dedent(
            "
            def f():
                assert False
                print(\"dead\")
            ",
        );

        let snippets = collect_unreachable_entries(
            &TestDbBuilder::new()
                .with_file("/src/main.py", &source)
                .build()?,
            "/src/main.py",
            &source,
        );
        assert_eq!(
            snippets,
            vec![("print(\"dead\")".to_owned(), UnreachableKind::Unconditional)]
        );
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
    fn reports_while_false_body_statement() -> anyhow::Result<()> {
        let source = dedent(
            "
            while False:
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
    fn reports_unreachable_ternary_branch() -> anyhow::Result<()> {
        let source = dedent(
            "
            x = \"yes\" if True else \"no\"
            ",
        );

        let snippets = collect_unreachable_snippets(&source)?;
        assert_eq!(snippets, vec!["\"no\""]);
        Ok(())
    }

    #[test]
    fn keeps_separate_unreachable_regions_separate() -> anyhow::Result<()> {
        let source = dedent(
            "
            if False:
                x = 1

            if False:
                y = 2
            ",
        );

        let snippets = collect_unreachable_snippets(&source)?;
        assert_eq!(snippets, vec!["x = 1", "y = 2"]);
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
    fn reports_unreachable_function_definition() -> anyhow::Result<()> {
        let source = dedent(
            "
            if False:
                def f():
                    pass
            ",
        );

        let snippets = collect_unreachable_snippets(&source)?;
        assert_eq!(snippets, vec!["def f():\n        pass"]);
        Ok(())
    }

    #[test]
    fn reports_unreachable_class_definition() -> anyhow::Result<()> {
        let source = dedent(
            "
            if False:
                class Foo:
                    pass
            ",
        );

        let snippets = collect_unreachable_snippets(&source)?;
        assert_eq!(snippets, vec!["class Foo:\n        pass"]);
        Ok(())
    }

    #[test]
    fn merges_unreachable_comprehension_scope_into_enclosing_block() -> anyhow::Result<()> {
        let source = dedent(
            "
            if False:
                x = [i for i in range(10)]
            ",
        );

        let snippets = collect_unreachable_snippets(&source)?;
        assert_eq!(snippets, vec!["x = [i for i in range(10)]"]);
        Ok(())
    }

    #[test]
    fn merges_unreachable_other_comprehension_scopes_into_enclosing_blocks() -> anyhow::Result<()> {
        let source = dedent(
            "
            if False:
                x = {k: v for k, v in {}.items()}

            if False:
                y = {i for i in range(10)}

            if False:
                z = (i for i in range(10))
            ",
        );

        let snippets = collect_unreachable_snippets(&source)?;
        assert_eq!(
            snippets,
            vec![
                "x = {k: v for k, v in {}.items()}",
                "y = {i for i in range(10)}",
                "z = (i for i in range(10))",
            ]
        );
        Ok(())
    }

    #[test]
    fn reports_unreachable_type_alias() -> anyhow::Result<()> {
        let source = dedent(
            "
            if False:
                type Alias[T] = list[T]
            ",
        );

        let db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY312)
            .with_file("/src/main.py", &source)
            .build()?;

        let snippets = collect_unreachable_entries(&db, "/src/main.py", &source)
            .into_iter()
            .map(|(snippet, _)| snippet)
            .collect::<Vec<_>>();
        assert_eq!(snippets, vec!["type Alias[T] = list[T]"]);
        Ok(())
    }

    #[test]
    fn reports_version_guarded_branch_as_current_analysis_unreachable() -> anyhow::Result<()> {
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

        let snippets = collect_unreachable_entries(&db, "/src/main.py", &source);
        assert_eq!(
            snippets,
            vec![(
                "from typing import Self".to_owned(),
                UnreachableKind::CurrentAnalysis
            )]
        );
        Ok(())
    }

    #[test]
    fn reports_noreturn_tail_as_current_analysis_unreachable() -> anyhow::Result<()> {
        let source = dedent(
            "
            from typing_extensions import NoReturn

            def fail() -> NoReturn:
                raise RuntimeError()

            def f():
                fail()
                print(\"dead\")
            ",
        );

        let db = TestDbBuilder::new()
            .with_file("/src/main.py", &source)
            .build()?;

        let snippets = collect_unreachable_entries(&db, "/src/main.py", &source);
        assert_eq!(
            snippets,
            vec![(
                "print(\"dead\")".to_owned(),
                UnreachableKind::CurrentAnalysis
            )]
        );
        Ok(())
    }

    #[test]
    fn does_not_report_conditional_noreturn_tail_as_unreachable() -> anyhow::Result<()> {
        let source = dedent(
            "
            from typing_extensions import NoReturn

            def fail() -> NoReturn:
                raise RuntimeError()

            def f(x: bool):
                if x:
                    fail()
                print(\"reachable\")
            ",
        );

        let snippets = collect_unreachable_snippets(&source)?;
        assert_eq!(snippets, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn merges_overlapping_ranges_of_different_kinds() -> anyhow::Result<()> {
        let source = dedent(
            "
            import sys

            if sys.version_info >= (3, 11):
                if False:
                    x = lambda: 1
            ",
        );

        let db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY310)
            .with_file("/src/main.py", &source)
            .build()?;

        let snippets = collect_unreachable_entries(&db, "/src/main.py", &source);
        assert_eq!(
            snippets,
            vec![(
                "if False:\n        x = lambda: 1".to_owned(),
                UnreachableKind::CurrentAnalysis
            )]
        );
        Ok(())
    }

    #[test]
    fn does_not_report_type_checking_block_as_unreachable() -> anyhow::Result<()> {
        let source = dedent(
            "
            from typing import TYPE_CHECKING

            if TYPE_CHECKING:
                import expensive_module
            ",
        );

        let snippets = collect_unreachable_snippets(&source)?;
        assert_eq!(snippets, Vec::<String>::new());
        Ok(())
    }
}
