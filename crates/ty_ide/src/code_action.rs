use crate::completion;

use ruff_db::{files::File, parsed::parsed_module};
use ruff_diagnostics::Edit;
use ruff_python_ast::find_node::covering_node;
use ruff_text_size::TextRange;
use ty_project::Db;
use ty_python_semantic::create_suppression_fix;
use ty_python_semantic::lint::LintId;
use ty_python_semantic::types::{UNDEFINED_REVEAL, UNRESOLVED_REFERENCE};

/// A `QuickFix` Code Action
#[derive(Debug, Clone)]
pub struct QuickFix {
    pub title: String,
    pub edits: Vec<Edit>,
    pub preferred: bool,
}

pub fn code_actions(
    db: &dyn Db,
    file: File,
    diagnostic_range: TextRange,
    diagnostic_id: &str,
) -> Vec<QuickFix> {
    let registry = db.lint_registry();
    let Ok(lint_id) = registry.get(diagnostic_id) else {
        return Vec::new();
    };

    let mut actions = Vec::new();

    // Suggest imports/qualifications for unresolved references (often ideal)
    let is_unresolved_reference =
        lint_id == LintId::of(&UNRESOLVED_REFERENCE) || lint_id == LintId::of(&UNDEFINED_REVEAL);
    if is_unresolved_reference
        && let Some(import_quick_fix) = unresolved_fixes(db, file, diagnostic_range)
    {
        actions.extend(import_quick_fix);
    }

    // Suggest just suppressing the lint (always a valid option, but never ideal)
    actions.push(QuickFix {
        title: format!("Ignore '{}' for this line", lint_id.name()),
        edits: create_suppression_fix(db, file, lint_id, diagnostic_range).into_edits(),
        preferred: false,
    });

    actions
}

fn unresolved_fixes(
    db: &dyn Db,
    file: File,
    diagnostic_range: TextRange,
) -> Option<impl Iterator<Item = QuickFix>> {
    let parsed = parsed_module(db, file).load(db);
    let node = covering_node(parsed.syntax().into(), diagnostic_range).node();
    let symbol = &node.expr_name()?.id;

    Some(
        completion::unresolved_fixes(db, file, &parsed, symbol, node)
            .into_iter()
            .map(|import| QuickFix {
                title: import.label,
                edits: vec![import.edit],
                preferred: true,
            }),
    )
}

#[cfg(test)]
mod tests {

    use crate::code_actions;

    use insta::assert_snapshot;
    use ruff_db::{
        diagnostic::{
            Annotation, Diagnostic, DiagnosticFormat, DiagnosticId, DisplayDiagnosticConfig,
            LintName, Span, SubDiagnostic,
        },
        files::{File, system_path_to_file},
        system::{DbWithWritableSystem, SystemPathBuf},
    };
    use ruff_diagnostics::Fix;
    use ruff_python_trivia::textwrap::dedent;
    use ruff_text_size::{TextRange, TextSize};
    use ty_project::ProjectMetadata;
    use ty_python_semantic::{
        lint::LintMetadata,
        types::{UNDEFINED_REVEAL, UNRESOLVED_REFERENCE},
    };

    #[test]
    fn add_ignore() {
        let test = CodeActionTest::with_source(r#"b = <START>a<END> / 10"#);

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @r"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:1:5
          |
        1 | b = a / 10
          |     ^
          |
          - b = a / 10
        1 + b = a / 10  # ty:ignore[unresolved-reference]
        ");
    }

    #[test]
    fn add_ignore_existing_comment() {
        let test = CodeActionTest::with_source(r#"b = <START>a<END> / 10  # fmt: off"#);

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @r"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:1:5
          |
        1 | b = a / 10  # fmt: off
          |     ^
          |
          - b = a / 10  # fmt: off
        1 + b = a / 10  # fmt: off  # ty:ignore[unresolved-reference]
        ");
    }

    #[test]
    fn add_ignore_trailing_whitespace() {
        let test = CodeActionTest::with_source(r#"b = <START>a<END> / 10  "#);

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @r"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:1:5
          |
        1 | b = a / 10  
          |     ^
          |
          - b = a / 10  
        1 + b = a / 10  # ty:ignore[unresolved-reference]
        ");
    }

    #[test]
    fn add_code_existing_ignore() {
        let test = CodeActionTest::with_source(
            r#"
            b = <START>a<END> / 0  # ty:ignore[division-by-zero]
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @r"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:2:5
          |
        2 | b = a / 0  # ty:ignore[division-by-zero]
          |     ^
          |
        1 | 
          - b = a / 0  # ty:ignore[division-by-zero]
        2 + b = a / 0  # ty:ignore[division-by-zero, unresolved-reference]
        ");
    }

    #[test]
    fn add_code_existing_ignore_trailing_comma() {
        let test = CodeActionTest::with_source(
            r#"
            b = <START>a<END> / 0  # ty:ignore[division-by-zero,]
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @r"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:2:5
          |
        2 | b = a / 0  # ty:ignore[division-by-zero,]
          |     ^
          |
        1 | 
          - b = a / 0  # ty:ignore[division-by-zero,]
        2 + b = a / 0  # ty:ignore[division-by-zero, unresolved-reference]
        ");
    }

    #[test]
    fn add_code_existing_ignore_trailing_whitespace() {
        let test = CodeActionTest::with_source(
            r#"
            b = <START>a<END> / 0  # ty:ignore[division-by-zero   ]
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @r"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:2:5
          |
        2 | b = a / 0  # ty:ignore[division-by-zero   ]
          |     ^
          |
        1 | 
          - b = a / 0  # ty:ignore[division-by-zero   ]
        2 + b = a / 0  # ty:ignore[division-by-zero, unresolved-reference   ]
        ");
    }

    #[test]
    fn add_code_existing_ignore_with_reason() {
        let test = CodeActionTest::with_source(
            r#"
            b = <START>a<END> / 0  # ty:ignore[division-by-zero] some explanation
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @r"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:2:5
          |
        2 | b = a / 0  # ty:ignore[division-by-zero] some explanation
          |     ^
          |
        1 | 
          - b = a / 0  # ty:ignore[division-by-zero] some explanation
        2 + b = a / 0  # ty:ignore[division-by-zero] some explanation  # ty:ignore[unresolved-reference]
        ");
    }

    #[test]
    fn add_code_existing_ignore_start_line() {
        let test = CodeActionTest::with_source(
            r#"
            b = (
                    <START>a  # ty:ignore[division-by-zero]
                    /
                    0<END>
            )
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @r"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:3:9
          |
        2 |   b = (
        3 | /         a  # ty:ignore[division-by-zero]
        4 | |         /
        5 | |         0
          | |_________^
        6 |   )
          |
        1 | 
        2 | b = (
          -         a  # ty:ignore[division-by-zero]
        3 +         a  # ty:ignore[division-by-zero, unresolved-reference]
        4 |         /
        5 |         0
        6 | )
        ");
    }

    #[test]
    fn add_code_existing_ignore_end_line() {
        let test = CodeActionTest::with_source(
            r#"
            b = (
                    <START>a
                    /
                    0<END>  # ty:ignore[division-by-zero]
            )
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @r"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:3:9
          |
        2 |   b = (
        3 | /         a
        4 | |         /
        5 | |         0  # ty:ignore[division-by-zero]
          | |_________^
        6 |   )
          |
        2 | b = (
        3 |         a
        4 |         /
          -         0  # ty:ignore[division-by-zero]
        5 +         0  # ty:ignore[division-by-zero, unresolved-reference]
        6 | )
        ");
    }

    #[test]
    fn add_code_existing_ignores() {
        let test = CodeActionTest::with_source(
            r#"
            b = (
                    <START>a  # ty:ignore[division-by-zero]
                    /
                    0<END>  # ty:ignore[division-by-zero]
            )
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @r"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:3:9
          |
        2 |   b = (
        3 | /         a  # ty:ignore[division-by-zero]
        4 | |         /
        5 | |         0  # ty:ignore[division-by-zero]
          | |_________^
        6 |   )
          |
        1 | 
        2 | b = (
          -         a  # ty:ignore[division-by-zero]
        3 +         a  # ty:ignore[division-by-zero, unresolved-reference]
        4 |         /
        5 |         0  # ty:ignore[division-by-zero]
        6 | )
        ");
    }

    #[test]
    fn add_code_interpolated_string() {
        let test = CodeActionTest::with_source(
            r#"
            b = f"""
                {<START>a<END>}
                more text
            """
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @r#"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:3:6
          |
        2 | b = f"""
        3 |     {a}
          |      ^
        4 |     more text
        5 | """
          |
        2 | b = f"""
        3 |     {a}
        4 |     more text
          - """
        5 + """  # ty:ignore[unresolved-reference]
        "#);
    }

    #[test]
    fn add_code_multiline_interpolation() {
        let test = CodeActionTest::with_source(
            r#"
            b = f"""
                {
                <START>a<END>
                }
                more text
            """
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @r#"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:4:5
          |
        2 | b = f"""
        3 |     {
        4 |     a
          |     ^
        5 |     }
        6 |     more text
          |
        1 | 
        2 | b = f"""
        3 |     {
          -     a
        4 +     a  # ty:ignore[unresolved-reference]
        5 |     }
        6 |     more text
        7 | """
        "#);
    }

    #[test]
    fn add_code_followed_by_multiline_string() {
        let test = CodeActionTest::with_source(
            r#"
            b = <START>a<END> + """
                more text
            """
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @r#"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:2:5
          |
        2 | b = a + """
          |     ^
        3 |     more text
        4 | """
          |
        1 | 
        2 | b = a + """
        3 |     more text
          - """
        4 + """  # ty:ignore[unresolved-reference]
        "#);
    }

    #[test]
    fn add_code_followed_by_continuation() {
        let test = CodeActionTest::with_source(
            r#"
            b = <START>a<END> \
            + "test"
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @r#"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:2:5
          |
        2 | b = a \
          |     ^
        3 | + "test"
          |
        1 | 
        2 | b = a \
          - + "test"
        3 + + "test"  # ty:ignore[unresolved-reference]
        "#);
    }

    #[test]
    fn undefined_reveal_type() {
        let test = CodeActionTest::with_source(
            r#"
            <START>reveal_type<END>(1)
        "#,
        );

        assert_snapshot!(test.code_actions(&UNDEFINED_REVEAL), @r"
        info[code-action]: import typing.reveal_type
         --> main.py:2:1
          |
        2 | reveal_type(1)
          | ^^^^^^^^^^^
          |
        help: This is a preferred code action
        1 + from typing import reveal_type
        2 | 
        3 | reveal_type(1)

        info[code-action]: Ignore 'undefined-reveal' for this line
         --> main.py:2:1
          |
        2 | reveal_type(1)
          | ^^^^^^^^^^^
          |
        1 | 
          - reveal_type(1)
        2 + reveal_type(1)  # ty:ignore[undefined-reveal]
        ");
    }

    #[test]
    fn unresolved_deprecated() {
        let test = CodeActionTest::with_source(
            r#"
            @<START>deprecated<END>("do not use")
            def my_func(): ...
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @r#"
        info[code-action]: import warnings.deprecated
         --> main.py:2:2
          |
        2 | @deprecated("do not use")
          |  ^^^^^^^^^^
        3 | def my_func(): ...
          |
        help: This is a preferred code action
        1 + from warnings import deprecated
        2 | 
        3 | @deprecated("do not use")
        4 | def my_func(): ...

        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:2:2
          |
        2 | @deprecated("do not use")
          |  ^^^^^^^^^^
        3 | def my_func(): ...
          |
        1 | 
          - @deprecated("do not use")
        2 + @deprecated("do not use")  # ty:ignore[unresolved-reference]
        3 | def my_func(): ...
        "#);
    }

    #[test]
    fn unresolved_deprecated_warnings_imported() {
        let test = CodeActionTest::with_source(
            r#"
            import warnings

            @<START>deprecated<END>("do not use")
            def my_func(): ...
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @r#"
        info[code-action]: import warnings.deprecated
         --> main.py:4:2
          |
        2 | import warnings
        3 |
        4 | @deprecated("do not use")
          |  ^^^^^^^^^^
        5 | def my_func(): ...
          |
        help: This is a preferred code action
        1 + from warnings import deprecated
        2 | 
        3 | import warnings
        4 | 

        info[code-action]: qualify warnings.deprecated
         --> main.py:4:2
          |
        2 | import warnings
        3 |
        4 | @deprecated("do not use")
          |  ^^^^^^^^^^
        5 | def my_func(): ...
          |
        help: This is a preferred code action
        1 | 
        2 | import warnings
        3 | 
          - @deprecated("do not use")
        4 + @warnings.deprecated("do not use")
        5 | def my_func(): ...

        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:4:2
          |
        2 | import warnings
        3 |
        4 | @deprecated("do not use")
          |  ^^^^^^^^^^
        5 | def my_func(): ...
          |
        1 | 
        2 | import warnings
        3 | 
          - @deprecated("do not use")
        4 + @deprecated("do not use")  # ty:ignore[unresolved-reference]
        5 | def my_func(): ...
        "#);
    }

    // using `importlib.abc.ExecutionLoader` when no imports are in scope
    #[test]
    fn unresolved_loader() {
        let test = CodeActionTest::with_source(
            r#"
            <START>ExecutionLoader<END>
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @r"
        info[code-action]: import importlib.abc.ExecutionLoader
         --> main.py:2:1
          |
        2 | ExecutionLoader
          | ^^^^^^^^^^^^^^^
          |
        help: This is a preferred code action
        1 + from importlib.abc import ExecutionLoader
        2 | 
        3 | ExecutionLoader

        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:2:1
          |
        2 | ExecutionLoader
          | ^^^^^^^^^^^^^^^
          |
        1 | 
          - ExecutionLoader
        2 + ExecutionLoader  # ty:ignore[unresolved-reference]
        ");
    }

    // using `importlib.abc.ExecutionLoader` when `import importlib` is in scope
    //
    // TODO: `importlib.abc` is available whenever `importlib` is, so qualifying
    // `importlib.abc.ExecutionLoader` without adding imports is actually legal here!
    #[test]
    fn unresolved_loader_importlib_imported() {
        let test = CodeActionTest::with_source(
            r#"
            import importlib
            <START>ExecutionLoader<END>
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @r"
        info[code-action]: import importlib.abc.ExecutionLoader
         --> main.py:3:1
          |
        2 | import importlib
        3 | ExecutionLoader
          | ^^^^^^^^^^^^^^^
          |
        help: This is a preferred code action
        1 + from importlib.abc import ExecutionLoader
        2 | 
        3 | import importlib
        4 | ExecutionLoader

        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:3:1
          |
        2 | import importlib
        3 | ExecutionLoader
          | ^^^^^^^^^^^^^^^
          |
        1 | 
        2 | import importlib
          - ExecutionLoader
        3 + ExecutionLoader  # ty:ignore[unresolved-reference]
        ");
    }

    // Using `importlib.abc.ExecutionLoader` when `import importlib.abc` is in scope
    #[test]
    fn unresolved_loader_abc_imported() {
        let test = CodeActionTest::with_source(
            r#"
            import importlib.abc
            <START>ExecutionLoader<END>
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @r"
        info[code-action]: import importlib.abc.ExecutionLoader
         --> main.py:3:1
          |
        2 | import importlib.abc
        3 | ExecutionLoader
          | ^^^^^^^^^^^^^^^
          |
        help: This is a preferred code action
        1 + from importlib.abc import ExecutionLoader
        2 | 
        3 | import importlib.abc
        4 | ExecutionLoader

        info[code-action]: qualify importlib.abc.ExecutionLoader
         --> main.py:3:1
          |
        2 | import importlib.abc
        3 | ExecutionLoader
          | ^^^^^^^^^^^^^^^
          |
        help: This is a preferred code action
        1 | 
        2 | import importlib.abc
          - ExecutionLoader
        3 + importlib.abc.ExecutionLoader

        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:3:1
          |
        2 | import importlib.abc
        3 | ExecutionLoader
          | ^^^^^^^^^^^^^^^
          |
        1 | 
        2 | import importlib.abc
          - ExecutionLoader
        3 + ExecutionLoader  # ty:ignore[unresolved-reference]
        ");
    }

    pub(super) struct CodeActionTest {
        pub(super) db: ty_project::TestDb,
        pub(super) file: File,
        pub(super) diagnostic_range: TextRange,
    }

    impl CodeActionTest {
        pub(super) fn with_source(source: &str) -> Self {
            let mut db = ty_project::TestDb::new(ProjectMetadata::new(
                "test".into(),
                SystemPathBuf::from("/"),
            ));

            db.init_program().unwrap();

            let mut cleansed = dedent(source).to_string();

            let start = cleansed
                .find("<START>")
                .expect("source text should contain a `<START>` marker");
            cleansed.replace_range(start..start + "<START>".len(), "");

            let end = cleansed
                .find("<END>")
                .expect("source text should contain a `<END>` marker");

            cleansed.replace_range(end..end + "<END>".len(), "");

            assert!(start <= end, "<START> marker should be before <END> marker");

            db.write_file("main.py", cleansed)
                .expect("write to memory file system to be successful");

            let file = system_path_to_file(&db, "main.py").expect("newly written file to existing");

            Self {
                db,
                file,
                diagnostic_range: TextRange::new(
                    TextSize::try_from(start).unwrap(),
                    TextSize::try_from(end).unwrap(),
                ),
            }
        }

        pub(super) fn code_actions(&self, lint: &'static LintMetadata) -> String {
            use std::fmt::Write;

            let mut buf = String::new();

            let config = DisplayDiagnosticConfig::default()
                .color(false)
                .show_fix_diff(true)
                .format(DiagnosticFormat::Full);

            for mut action in code_actions(&self.db, self.file, self.diagnostic_range, &lint.name) {
                let mut diagnostic = Diagnostic::new(
                    DiagnosticId::Lint(LintName::of("code-action")),
                    ruff_db::diagnostic::Severity::Info,
                    action.title,
                );

                diagnostic.annotate(Annotation::primary(
                    Span::from(self.file).with_range(self.diagnostic_range),
                ));

                if action.preferred {
                    diagnostic.sub(SubDiagnostic::new(
                        ruff_db::diagnostic::SubDiagnosticSeverity::Help,
                        "This is a preferred code action",
                    ));
                }

                let first_edit = action.edits.remove(0);
                diagnostic.set_fix(Fix::safe_edits(first_edit, action.edits));

                write!(buf, "{}", diagnostic.display(&self.db, &config)).unwrap();
            }

            buf
        }
    }
}
