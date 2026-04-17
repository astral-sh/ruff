use crate::Db;
use crate::reachability::is_reachable;
use itertools::Itertools;
use ruff_db::files::File;
use ruff_text_size::TextRange;
use ty_python_core::reachability_constraints::ScopedReachabilityConstraintId;
use ty_python_core::semantic_index;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct UnreachableRange {
    pub range: TextRange,
    pub kind: UnreachableKind,
}

/// Classification for unreachable-code hints.
///
/// `Unconditional` means the code is unreachable regardless of the checked
/// Python version or platform, for example after a terminal statement:
///
/// ```python
/// def test():
///     return True
///     print("unreachable")
/// ```
///
/// `CurrentAnalysis` means the code is unreachable under the current analysis,
/// for example because of the configured Python version:
///
/// ```python
/// import sys
///
/// if sys.version_info <= (3, 10):
///     print("unreachable when checking with Python 3.11+")
/// ```
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum UnreachableKind {
    Unconditional,
    CurrentAnalysis,
}

/// Returns merged unreachable ranges for unnecessary-code hints, sorted by source order.
///
/// Collects all unreachable ranges recorded in each scope's use-def map.
/// `ALWAYS_FALSE` constraints are classified as unconditional; all others are
/// unreachable only under the current analysis.
#[salsa::tracked(returns(ref))]
pub fn unreachable_ranges(db: &dyn Db, file: File) -> Vec<UnreachableRange> {
    let index = semantic_index(db, file);
    let mut unreachable = Vec::new();

    for scope_id in index.scope_ids() {
        let use_def = index.use_def_map(scope_id.file_scope_id(db));
        unreachable.extend(
            use_def
                .range_reachability()
                .filter_map(|(range, constraint)| {
                    (!is_reachable(db, use_def, constraint)).then_some(UnreachableRange {
                        range,
                        kind: if constraint == ScopedReachabilityConstraintId::ALWAYS_FALSE {
                            UnreachableKind::Unconditional
                        } else {
                            UnreachableKind::CurrentAnalysis
                        },
                    })
                }),
        );
    }

    merge_overlapping_ranges(unreachable)
}

fn merge_overlapping_ranges(mut ranges: Vec<UnreachableRange>) -> Vec<UnreachableRange> {
    ranges.sort_unstable_by_key(|range| (range.range.start(), range.range.end(), range.kind));

    ranges
        .into_iter()
        .coalesce(|mut previous, range| {
            if range.range.start() < previous.range.end() {
                previous.range = TextRange::new(
                    previous.range.start(),
                    previous.range.end().max(range.range.end()),
                );
                previous.kind = previous.kind.max(range.kind);
                Ok(previous)
            } else {
                Err((previous, range))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{UnreachableKind, unreachable_ranges};
    use crate::db::tests::TestDbBuilder;
    use insta::assert_snapshot;
    use ruff_db::diagnostic::{
        Annotation, Diagnostic, DiagnosticId, DisplayDiagnosticConfig, DisplayDiagnostics, Severity,
    };
    use ruff_db::files::{FileRange, system_path_to_file};
    use ruff_python_ast::PythonVersion;
    use ruff_python_trivia::textwrap::dedent;
    use ty_python_core::platform::PythonPlatform;

    const TEST_PATH: &str = "/src/main.py";

    struct UnreachableTest {
        python_version: Option<PythonVersion>,
        python_platform: Option<PythonPlatform>,
    }

    impl UnreachableTest {
        fn new() -> Self {
            Self {
                python_version: None,
                python_platform: None,
            }
        }

        fn with_python_version(&mut self, version: PythonVersion) -> &mut Self {
            self.python_version = Some(version);
            self
        }

        fn with_python_platform(&mut self, platform: PythonPlatform) -> &mut Self {
            self.python_platform = Some(platform);
            self
        }

        fn render(&self, source: &str) -> anyhow::Result<String> {
            let mut db = TestDbBuilder::new();

            if let Some(version) = self.python_version {
                db = db.with_python_version(version);
            }

            if let Some(platform) = self.python_platform.clone() {
                db = db.with_python_platform(platform);
            }

            let source = dedent(source);
            let db = db.with_file(TEST_PATH, &source).build()?;
            Ok(render_unreachable_diagnostics(&db, TEST_PATH))
        }
    }

    fn render_unreachable_diagnostics(db: &crate::db::tests::TestDb, path: &str) -> String {
        let file = system_path_to_file(db, path).unwrap();
        let diagnostics = unreachable_ranges(db, file)
            .iter()
            .map(|range| {
                let mut diagnostic = Diagnostic::new(
                    DiagnosticId::lint("unreachable-code"),
                    Severity::Info,
                    match range.kind {
                        UnreachableKind::Unconditional => "Code is always unreachable",
                        UnreachableKind::CurrentAnalysis => "Code is unreachable",
                    },
                );
                diagnostic.annotate(Annotation::primary(
                    FileRange::new(file, range.range).into(),
                ));
                diagnostic
            })
            .collect::<Vec<_>>();

        DisplayDiagnostics::new(
            db,
            &DisplayDiagnosticConfig::new("ty").context(0),
            &diagnostics,
        )
        .to_string()
        .replace('\\', "/")
    }

    #[test]
    fn reports_statement_after_return() -> anyhow::Result<()> {
        let source = r#"
            def f():
                return 1
                print("dead")
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r#"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:4:5
          |
        4 |     print("dead")
          |     ^^^^^^^^^^^^^
          |
        "#);
        Ok(())
    }

    #[test]
    fn keeps_reachable_code_before_return_out_of_results() -> anyhow::Result<()> {
        let source = r#"
            def f():
                x = 1
                return x
                print("dead")
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r#"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:5:5
          |
        5 |     print("dead")
          |     ^^^^^^^^^^^^^
          |
        "#);
        Ok(())
    }

    #[test]
    fn keeps_reachable_code_after_unreachable_statement_out_of_results() -> anyhow::Result<()> {
        let source = r#"
            def f(value: int):
                x = 1

                if value == x:
                    return x
                    print("dead")

                print("not dead")
                return value
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r#"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:7:9
          |
        7 |         print("dead")
          |         ^^^^^^^^^^^^^
          |
        "#);
        Ok(())
    }

    #[test]
    fn merges_consecutive_unreachable_statements() -> anyhow::Result<()> {
        let source = r#"
            def f():
                return 1
                print("dead")
                print("still dead")
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r#"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:4:5
          |
        4 | /     print("dead")
        5 | |     print("still dead")
          | |_______________________^
          |
        "#);
        Ok(())
    }

    #[test]
    fn reports_statement_after_raise() -> anyhow::Result<()> {
        let source = r#"
            def f():
                raise RuntimeError()
                print("dead")
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r#"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:4:5
          |
        4 |     print("dead")
          |     ^^^^^^^^^^^^^
          |
        "#);
        Ok(())
    }

    #[test]
    fn reports_statement_after_raise_inside_try() -> anyhow::Result<()> {
        let source = r#"
            def f():
                try:
                    raise ValueError()
                    print("dead")
                except ValueError:
                    pass
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r#"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:5:9
          |
        5 |         print("dead")
          |         ^^^^^^^^^^^^^
          |
        "#);
        Ok(())
    }

    #[test]
    fn reports_statement_after_assert_false() -> anyhow::Result<()> {
        let source = r#"
            def f():
                assert False
                print("dead")
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r#"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:4:5
          |
        4 |     print("dead")
          |     ^^^^^^^^^^^^^
          |
        "#);
        Ok(())
    }

    #[test]
    fn reports_statement_after_break() -> anyhow::Result<()> {
        let source = r#"
            def f():
                while True:
                    break
                    print("dead")
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r#"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:5:9
          |
        5 |         print("dead")
          |         ^^^^^^^^^^^^^
          |
        "#);
        Ok(())
    }

    #[test]
    fn reports_statement_after_continue() -> anyhow::Result<()> {
        let source = r#"
            def f():
                for _ in range(1):
                    continue
                    print("dead")
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r#"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:5:9
          |
        5 |         print("dead")
          |         ^^^^^^^^^^^^^
          |
        "#);
        Ok(())
    }

    #[test]
    fn reports_statement_after_infinite_loop() -> anyhow::Result<()> {
        let source = r#"
            def f():
                while True:
                    pass
                print("dead")
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r#"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:5:5
          |
        5 |     print("dead")
          |     ^^^^^^^^^^^^^
          |
        "#);
        Ok(())
    }

    #[test]
    fn reports_false_branch_statement() -> anyhow::Result<()> {
        let source = r#"
            if False:
                print("dead")
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r#"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:3:5
          |
        3 |     print("dead")
          |     ^^^^^^^^^^^^^
          |
        "#);
        Ok(())
    }

    #[test]
    fn reports_while_false_body_statement() -> anyhow::Result<()> {
        let source = r#"
            while False:
                print("dead")
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r#"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:3:5
          |
        3 |     print("dead")
          |     ^^^^^^^^^^^^^
          |
        "#);
        Ok(())
    }

    #[test]
    fn reports_false_branch_from_statically_known_arithmetic() -> anyhow::Result<()> {
        let source = r#"
            if 2 + 3 > 10:
                print("dead")
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r#"
        info[unreachable-code]: Code is unreachable
         --> src/main.py:3:5
          |
        3 |     print("dead")
          |     ^^^^^^^^^^^^^
          |
        "#);
        Ok(())
    }

    #[test]
    fn reports_else_branch_after_true_condition() -> anyhow::Result<()> {
        let source = r#"
            if True:
                pass
            else:
                print("dead")
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r#"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:5:5
          |
        5 |     print("dead")
          |     ^^^^^^^^^^^^^
          |
        "#);
        Ok(())
    }

    #[test]
    fn reports_statement_in_unreachable_elif_branch() -> anyhow::Result<()> {
        let source = r#"
            if True:
                pass
            elif False:
                print("dead")
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r#"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:5:5
          |
        5 |     print("dead")
          |     ^^^^^^^^^^^^^
          |
        "#);
        Ok(())
    }

    #[test]
    fn reports_statement_after_chained_always_taken_terminating_branch() -> anyhow::Result<()> {
        let source = r#"
            def f():
                if False:
                    return
                elif True:
                    return
                else:
                    pass
                print("dead")
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r#"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:4:9
          |
        4 |         return
          |         ^^^^^^
          |

        info[unreachable-code]: Code is always unreachable
         --> src/main.py:8:9
          |
        8 | /         pass
        9 | |     print("dead")
          | |_________________^
          |
        "#);
        Ok(())
    }

    #[test]
    fn reports_statement_after_always_taken_terminating_branch() -> anyhow::Result<()> {
        let source = r#"
            def f():
                if True:
                    return
                print("dead")
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r#"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:5:5
          |
        5 |     print("dead")
          |     ^^^^^^^^^^^^^
          |
        "#);
        Ok(())
    }

    #[test]
    fn reports_unreachable_ternary_branch() -> anyhow::Result<()> {
        let source = r#"
            x = "yes" if True else "no"
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r#"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:2:24
          |
        2 | x = "yes" if True else "no"
          |                        ^^^^
          |
        "#);
        Ok(())
    }

    #[test]
    fn keeps_separate_unreachable_regions_separate() -> anyhow::Result<()> {
        let source = r#"
            if False:
                x = 1

            if False:
                y = 2
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:3:5
          |
        3 |     x = 1
          |     ^^^^^
          |

        info[unreachable-code]: Code is always unreachable
         --> src/main.py:6:5
          |
        6 |     y = 2
          |     ^^^^^
          |
        ");
        Ok(())
    }

    #[test]
    fn merges_unreachable_scope_range_into_enclosing_block() -> anyhow::Result<()> {
        let source = r#"
            if False:
                x = lambda: 1
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:3:5
          |
        3 |     x = lambda: 1
          |     ^^^^^^^^^^^^^
          |
        ");
        Ok(())
    }

    #[test]
    fn reports_unreachable_function_definition() -> anyhow::Result<()> {
        let source = r#"
            if False:
                def f():
                    pass
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:3:5
          |
        3 | /     def f():
        4 | |         pass
          | |____________^
          |
        ");
        Ok(())
    }

    #[test]
    fn reports_unreachable_class_definition() -> anyhow::Result<()> {
        let source = r#"
            if False:
                class Foo:
                    pass
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:3:5
          |
        3 | /     class Foo:
        4 | |         pass
          | |____________^
          |
        ");
        Ok(())
    }

    #[test]
    fn merges_unreachable_comprehension_scope_into_enclosing_block() -> anyhow::Result<()> {
        let source = r#"
            if False:
                x = [i for i in range(10)]
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:3:5
          |
        3 |     x = [i for i in range(10)]
          |     ^^^^^^^^^^^^^^^^^^^^^^^^^^
          |
        ");
        Ok(())
    }

    #[test]
    fn merges_unreachable_other_comprehension_scopes_into_enclosing_blocks() -> anyhow::Result<()> {
        let source = r#"
            if False:
                x = {k: v for k, v in {}.items()}

            if False:
                y = {i for i in range(10)}

            if False:
                z = (i for i in range(10))
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:3:5
          |
        3 |     x = {k: v for k, v in {}.items()}
          |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
          |

        info[unreachable-code]: Code is always unreachable
         --> src/main.py:6:5
          |
        6 |     y = {i for i in range(10)}
          |     ^^^^^^^^^^^^^^^^^^^^^^^^^^
          |

        info[unreachable-code]: Code is always unreachable
         --> src/main.py:9:5
          |
        9 |     z = (i for i in range(10))
          |     ^^^^^^^^^^^^^^^^^^^^^^^^^^
          |
        ");
        Ok(())
    }

    #[test]
    fn reports_unreachable_type_alias() -> anyhow::Result<()> {
        let source = r#"
            if False:
                type Alias[T] = list[T]
            "#;

        let mut test = UnreachableTest::new();
        test.with_python_version(PythonVersion::PY312);

        assert_snapshot!(test.render(source)?, @r"
        info[unreachable-code]: Code is always unreachable
         --> src/main.py:3:5
          |
        3 |     type Alias[T] = list[T]
          |     ^^^^^^^^^^^^^^^^^^^^^^^
          |
        ");
        Ok(())
    }

    #[test]
    fn reports_version_guarded_branch_as_current_analysis_unreachable() -> anyhow::Result<()> {
        let source = r#"
            import sys

            if sys.version_info >= (3, 11):
                from typing import Self
            "#;

        let mut test = UnreachableTest::new();
        test.with_python_version(PythonVersion::PY310);

        assert_snapshot!(test.render(source)?, @r"
        info[unreachable-code]: Code is unreachable
         --> src/main.py:5:5
          |
        5 |     from typing import Self
          |     ^^^^^^^^^^^^^^^^^^^^^^^
          |
        ");
        Ok(())
    }

    #[test]
    fn reports_platform_guarded_branch_as_current_analysis_unreachable() -> anyhow::Result<()> {
        let source = r#"
            import sys

            if sys.platform == "win32":
                import winreg
            "#;

        let mut test = UnreachableTest::new();
        test.with_python_platform(PythonPlatform::Identifier("linux".to_string()));

        assert_snapshot!(test.render(source)?, @r"
        info[unreachable-code]: Code is unreachable
         --> src/main.py:5:5
          |
        5 |     import winreg
          |     ^^^^^^^^^^^^^
          |
        ");
        Ok(())
    }

    #[test]
    fn reports_noreturn_tail_as_current_analysis_unreachable() -> anyhow::Result<()> {
        let source = r#"
            from typing_extensions import NoReturn

            def fail() -> NoReturn:
                raise RuntimeError()

            def f():
                fail()
                print("dead")
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @r#"
        info[unreachable-code]: Code is unreachable
         --> src/main.py:9:5
          |
        9 |     print("dead")
          |     ^^^^^^^^^^^^^
          |
        "#);
        Ok(())
    }

    #[test]
    fn does_not_report_conditional_noreturn_tail_as_unreachable() -> anyhow::Result<()> {
        let source = r#"
            from typing_extensions import NoReturn

            def fail() -> NoReturn:
                raise RuntimeError()

            def f(x: bool):
                if x:
                    fail()
                print("reachable")
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @"");
        Ok(())
    }

    // The merged span includes `if False:` (CurrentAnalysis) which dominates `x = lambda: 1`
    // (Unconditional), so the whole range is conservatively classified as CurrentAnalysis.
    // TODO: if we ever report sub-ranges separately, the inner range could be Unconditional.
    #[test]
    fn merges_overlapping_ranges_of_different_kinds() -> anyhow::Result<()> {
        let source = r#"
            import sys

            if sys.version_info >= (3, 11):
                if False:
                    x = lambda: 1
            "#;

        let mut test = UnreachableTest::new();
        test.with_python_version(PythonVersion::PY310);

        assert_snapshot!(test.render(source)?, @r"
        info[unreachable-code]: Code is unreachable
         --> src/main.py:5:5
          |
        5 | /     if False:
        6 | |         x = lambda: 1
          | |_____________________^
          |
        ");
        Ok(())
    }

    #[test]
    fn does_not_report_type_checking_block_as_unreachable() -> anyhow::Result<()> {
        let source = r#"
            from typing import TYPE_CHECKING

            if TYPE_CHECKING:
                import expensive_module
            "#;

        assert_snapshot!(UnreachableTest::new().render(source)?, @"");
        Ok(())
    }
}
