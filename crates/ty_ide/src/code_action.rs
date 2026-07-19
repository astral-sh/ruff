use crate::completion;

use ruff_db::{files::File, parsed::parsed_module};
use ruff_diagnostics::Edit;
use ruff_python_ast::find_node::covering_node;
use ruff_text_size::TextRange;
use ty_project::Db;
use ty_python_semantic::lint::LintId;
use ty_python_semantic::suppress_single;
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

    // Suggest just suppressing the lint when a safe suppression can be added.
    if let Some(fix) = suppress_single(db, file, lint_id, diagnostic_range) {
        actions.push(QuickFix {
            title: format!("Ignore '{}' for this line", lint_id.name()),
            edits: fix.into_edits(),
            preferred: false,
        });
    }

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
    use ty_project::metadata::Options;
    use ty_project::metadata::options::AnalysisOptions;
    use ty_python_core::program::FallibleStrategy;
    use ty_python_semantic::{
        default_lint_registry,
        lint::LintMetadata,
        types::{UNDEFINED_REVEAL, UNRESOLVED_REFERENCE},
    };

    #[test]
    fn add_ignore() {
        let test = CodeActionTest::with_source(r#"b = <START>a<END> / 10"#);

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:1:5
          |
        1 | b = a / 10
          |     ^
          |
          |
          - b = a / 10
        1 + b = a / 10  # ty:ignore[unresolved-reference]
          |
        ");
    }

    #[test]
    fn add_ignore_existing_comment() {
        let test = CodeActionTest::with_source(r#"b = <START>a<END> / 10  # fmt: off"#);

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:1:5
          |
        1 | b = a / 10  # fmt: off
          |     ^
          |
          |
          - b = a / 10  # fmt: off
        1 + b = a / 10  # fmt: off  # ty:ignore[unresolved-reference]
          |
        ");
    }

    #[test]
    fn add_ignore_trailing_whitespace() {
        let test = CodeActionTest::with_source(r#"b = <START>a<END> / 10  "#);

        // Not an inline snapshot because of trailing whitespace.
        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE));
    }

    #[test]
    fn add_code_existing_ignore() {
        let test = CodeActionTest::with_source(
            r#"
            b = <START>a<END> / 0  # ty:ignore[division-by-zero]
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:2:5
          |
        2 | b = a / 0  # ty:ignore[division-by-zero]
          |     ^
          |
          |
        1 |
          - b = a / 0  # ty:ignore[division-by-zero]
        2 + b = a / 0  # ty:ignore[division-by-zero, unresolved-reference]
          |
        ");
    }

    #[test]
    fn add_code_existing_empty_ignore() {
        let test = CodeActionTest::with_source(
            r#"
            b = <START>a<END> / 10  # ty:ignore[]
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:2:5
          |
        2 | b = a / 10  # ty:ignore[]
          |     ^
          |
          |
        1 |
          - b = a / 10  # ty:ignore[]
        2 + b = a / 10  # ty:ignore[unresolved-reference]
          |
        ");
    }

    #[test]
    fn add_ignore_updates_preceding_own_line_suppression() {
        let test = CodeActionTest::with_source(
            r#"
            seen_code = True
            # ty:ignore[]
            b = <START>a<END> / 10
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:4:5
          |
        4 | b = a / 10
          |     ^
          |
          |
        2 | seen_code = True
          - # ty:ignore[]
        3 + # ty:ignore[unresolved-reference]
        4 | b = a / 10
          |
        ");
    }

    #[test]
    fn add_ignore_matches_existing_suppression_against_diagnostic_range() {
        // The first suppression is intentional: `not-a-rule` has no indexed suppression, and
        // repeatedly extending the final suppression can't suppress a diagnostic before it.
        let test = CodeActionTest::with_source(
            r#"
            seen_code = True
            # ty:ignore[] # ty:ignore[<START>not-a-rule<END>] # ty:ignore[division-by-zero]
            value = 1 / 0
        "#,
        );

        let lint = default_lint_registry()
            .get("ignore-comment-unknown-rule")
            .unwrap();
        assert_snapshot!(test.code_actions(&lint), @"
        info[code-action]: Ignore 'ignore-comment-unknown-rule' for this line
         --> main.py:3:27
          |
        3 | # ty:ignore[] # ty:ignore[not-a-rule] # ty:ignore[division-by-zero]
          |                           ^^^^^^^^^^
          |
          |
        2 | seen_code = True
          - # ty:ignore[] # ty:ignore[not-a-rule] # ty:ignore[division-by-zero]
        3 + # ty:ignore[ignore-comment-unknown-rule] # ty:ignore[not-a-rule] # ty:ignore[division-by-zero]
        4 | value = 1 / 0
          |
        ");
    }

    #[test]
    fn add_ignore_does_not_make_nested_suppression_unused() {
        let test = CodeActionTest::with_source(
            r#"
            seen_code = True
            # ty:ignore[]
            values = [
                # ty:ignore[unresolved-reference]
                missing,
                <START>absent<END>,
            ]
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:7:5
          |
        7 |     absent,
          |     ^^^^^^
          |
          |
        2 | seen_code = True
          - # ty:ignore[]
        3 + # ty:ignore[unresolved-reference]
        4 | values = [
          |
        ");
    }

    #[test]
    fn add_ignore_reuses_outer_suppression_with_nested_blanket() {
        let test = CodeActionTest::with_source(
            r#"
            def f(value: int) -> int: return value

            seen_code = True
            # ty:ignore[invalid-assignment]
            values: tuple[int] = [
                # ty:ignore
                f("bad"),
                <START>absent<END>,
            ]
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:9:5
          |
        9 |     absent,
          |     ^^^^^^
          |
          |
        4 | seen_code = True
          - # ty:ignore[invalid-assignment]
        5 + # ty:ignore[invalid-assignment, unresolved-reference]
        6 | values: tuple[int] = [
          |
        ");
    }

    #[test]
    fn add_code_own_line_unknown_rule_ignore() {
        let test = CodeActionTest::with_source(
            r#"
            seen_code = True
            # ty: ignore[<START>not-a-rule<END>] tracked by [123]
            value = 1
        "#,
        );

        assert_snapshot!(test.code_actions_for("ignore-comment-unknown-rule"), @"
        info[code-action]: Ignore 'ignore-comment-unknown-rule' for this line
         --> main.py:3:14
          |
        3 | # ty: ignore[not-a-rule] tracked by [123]
          |              ^^^^^^^^^^
          |
          |
        2 | seen_code = True
          - # ty: ignore[not-a-rule] tracked by [123]
        3 + # ty:ignore[ignore-comment-unknown-rule]  # ty: ignore[not-a-rule] tracked by [123]
        4 | value = 1
          |
        ");
    }

    #[test]
    fn add_code_mixed_unknown_rule_ignore() {
        let test = CodeActionTest::with_source(
            r#"
            value = missing  # ty: ignore[unresolved-reference, <START>not-a-rule<END>] tracked by [123]
        "#,
        );

        assert_snapshot!(test.code_actions_for("ignore-comment-unknown-rule"), @"
        info[code-action]: Ignore 'ignore-comment-unknown-rule' for this line
         --> main.py:2:53
          |
        2 | value = missing  # ty: ignore[unresolved-reference, not-a-rule] tracked by [123]
          |                                                     ^^^^^^^^^^
          |
          |
        1 |
          - value = missing  # ty: ignore[unresolved-reference, not-a-rule] tracked by [123]
        2 + value = missing  # ty: ignore[unresolved-reference, not-a-rule] tracked by [123]  # ty:ignore[ignore-comment-unknown-rule]
          |
        ");
    }

    #[test]
    fn add_code_unknown_rule_after_type_comment() {
        let test = CodeActionTest::with_source(
            r#"
            items = []  # type: list[int]  # ty: ignore[<START>not-a-rule<END>]
        "#,
        );

        assert_snapshot!(test.code_actions_for("ignore-comment-unknown-rule"), @"
        info[code-action]: Ignore 'ignore-comment-unknown-rule' for this line
         --> main.py:2:45
          |
        2 | items = []  # type: list[int]  # ty: ignore[not-a-rule]
          |                                             ^^^^^^^^^^
          |
          |
        1 |
          - items = []  # type: list[int]  # ty: ignore[not-a-rule]
        2 + items = []  # type: list[int]  # ty: ignore[not-a-rule]  # ty:ignore[ignore-comment-unknown-rule]
          |
        ");
    }

    #[test]
    fn add_code_unknown_rule_after_type_ignore() {
        let test = CodeActionTest::with_source(
            r#"
            value = "x"  # type: ignore[assignment]  # ty: ignore[<START>not-a-rule<END>]
        "#,
        );

        assert_snapshot!(test.code_actions_for("ignore-comment-unknown-rule"), @r#"
        info[code-action]: Ignore 'ignore-comment-unknown-rule' for this line
         --> main.py:2:55
          |
        2 | value = "x"  # type: ignore[assignment]  # ty: ignore[not-a-rule]
          |                                                       ^^^^^^^^^^
          |
          |
        1 |
          - value = "x"  # type: ignore[assignment]  # ty: ignore[not-a-rule]
        2 + value = "x"  # type: ignore[assignment]  # ty: ignore[not-a-rule]  # ty:ignore[ignore-comment-unknown-rule]
          |
        "#);
    }

    #[test]
    fn add_code_nested_unknown_rule_ignore() {
        let test = CodeActionTest::with_source(
            r#"
            seen_code = True
            # ty: ignore[<START>not-a-rule<END>]  # ty: ignore[unresolved-reference]
            value = missing
        "#,
        );

        assert_snapshot!(test.code_actions_for("ignore-comment-unknown-rule"), @"
        info[code-action]: Ignore 'ignore-comment-unknown-rule' for this line
         --> main.py:3:14
          |
        3 | # ty: ignore[not-a-rule]  # ty: ignore[unresolved-reference]
          |              ^^^^^^^^^^
          |
          |
        2 | seen_code = True
          - # ty: ignore[not-a-rule]  # ty: ignore[unresolved-reference]
        3 + # ty:ignore[ignore-comment-unknown-rule]  # ty: ignore[not-a-rule]  # ty: ignore[unresolved-reference]
        4 | value = missing
          |
        ");
    }

    #[test]
    fn add_code_nested_invalid_ignore() {
        let test = CodeActionTest::with_source(
            r#"
            seen_code = True
            # ty: ignore[<START>*-*<END>]  # ty: ignore[unresolved-reference]
            value = missing
        "#,
        );

        assert_snapshot!(test.code_actions_for("invalid-ignore-comment"), @"
        info[code-action]: Ignore 'invalid-ignore-comment' for this line
         --> main.py:3:14
          |
        3 | # ty: ignore[*-*]  # ty: ignore[unresolved-reference]
          |              ^^^
          |
          |
        2 | seen_code = True
          - # ty: ignore[*-*]  # ty: ignore[unresolved-reference]
        3 + # ty:ignore[invalid-ignore-comment]  # ty: ignore[*-*]  # ty: ignore[unresolved-reference]
        4 | value = missing
          |
        ");
    }

    #[test]
    fn add_code_invalid_hash_ignore() {
        let test = CodeActionTest::with_source(
            r#"
            seen_code = True
            # ty: ignore[<START>#<END>]
            value = 1
        "#,
        );

        assert_snapshot!(test.code_actions_for("invalid-ignore-comment"), @"
        info[code-action]: Ignore 'invalid-ignore-comment' for this line
         --> main.py:3:14
          |
        3 | # ty: ignore[#]
          |              ^
          |
          |
        2 | seen_code = True
          - # ty: ignore[#]
        3 + # ty:ignore[invalid-ignore-comment]  # ty: ignore[#]
        4 | value = 1
          |
        ");
    }

    #[test]
    fn add_code_invalid_trailing_hash_ignore() {
        let test = CodeActionTest::with_source(
            r#"
            seen_code = True
            # ty: ignore[unresolved-reference<START>#<END>]
            value = 1
        "#,
        );

        assert_snapshot!(test.code_actions_for("invalid-ignore-comment"), @"
        info[code-action]: Ignore 'invalid-ignore-comment' for this line
         --> main.py:3:34
          |
        3 | # ty: ignore[unresolved-reference#]
          |                                  ^
          |
          |
        2 | seen_code = True
          - # ty: ignore[unresolved-reference#]
        3 + # ty:ignore[invalid-ignore-comment]  # ty: ignore[unresolved-reference#]
        4 | value = 1
          |
        ");
    }

    #[test]
    fn add_code_file_level_unknown_rule_ignore() {
        let test = CodeActionTest::with_source(
            r#"
            # ty: ignore[<START>not-a-rule<END>]
            value = 1
        "#,
        );

        assert_snapshot!(test.code_actions_for("ignore-comment-unknown-rule"), @"
        info[code-action]: Ignore 'ignore-comment-unknown-rule' for this line
         --> main.py:2:14
          |
        2 | # ty: ignore[not-a-rule]
          |              ^^^^^^^^^^
          |
          |
        1 |
          - # ty: ignore[not-a-rule]
        2 + # ty: ignore[not-a-rule]  # ty:ignore[ignore-comment-unknown-rule]
        3 | value = 1
          |
        ");
    }

    #[test]
    fn no_ignore_code_action_for_shebang_suppression() {
        let test = CodeActionTest::with_source(
            r#"
            #!/usr/bin/env -S python3 -u # ty: ignore[<START>not-a-rule<END>]
            value = 1
        "#,
        );

        assert!(
            test.code_actions_for("ignore-comment-unknown-rule")
                .is_empty()
        );
    }

    #[test]
    fn add_code_inline_unknown_rule_ignore_with_type_ignores_disabled() {
        let test = CodeActionTest::with_source_and_type_ignores(
            r#"value = 1  # ty: ignore[<START>not-a-rule<END>]"#,
            false,
        );

        assert_snapshot!(test.code_actions_for("ignore-comment-unknown-rule"), @"
        info[code-action]: Ignore 'ignore-comment-unknown-rule' for this line
         --> main.py:1:25
          |
        1 | value = 1  # ty: ignore[not-a-rule]
          |                         ^^^^^^^^^^
          |
          |
          - value = 1  # ty: ignore[not-a-rule]
        1 + value = 1  # ty: ignore[not-a-rule]  # ty:ignore[ignore-comment-unknown-rule]
          |
        ");
    }

    #[test]
    fn add_code_existing_type_ignore() {
        let test = CodeActionTest::with_source(
            r#"
            b = <START>a<END> / 0  # type:ignore[ty:division-by-zero]
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:2:5
          |
        2 | b = a / 0  # type:ignore[ty:division-by-zero]
          |     ^
          |
          |
        1 |
          - b = a / 0  # type:ignore[ty:division-by-zero]
        2 + b = a / 0  # type:ignore[ty:division-by-zero, ty:unresolved-reference]
          |
        ");
    }

    #[test]
    fn add_code_existing_type_ignore_without_any_ty_code() {
        let test = CodeActionTest::with_source(
            r#"
            b = <START>a<END> / 0  # type:ignore[mypy-code]
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:2:5
          |
        2 | b = a / 0  # type:ignore[mypy-code]
          |     ^
          |
          |
        1 |
          - b = a / 0  # type:ignore[mypy-code]
        2 + b = a / 0  # type:ignore[mypy-code]  # ty:ignore[unresolved-reference]
          |
        ");
    }

    #[test]
    fn add_ignore_existing_file_level_ignore() {
        let test = CodeActionTest::with_source(
            r#"
            # ty:ignore[division-by-zero]

            b = <START>a<END> / 0
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:4:5
          |
        4 | b = a / 0
          |     ^
          |
          |
        3 |
          - b = a / 0
        4 + b = a / 0  # ty:ignore[unresolved-reference]
          |
        ");
    }

    #[test]
    fn add_code_existing_ignore_trailing_comma() {
        let test = CodeActionTest::with_source(
            r#"
            b = <START>a<END> / 0  # ty:ignore[division-by-zero,]
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:2:5
          |
        2 | b = a / 0  # ty:ignore[division-by-zero,]
          |     ^
          |
          |
        1 |
          - b = a / 0  # ty:ignore[division-by-zero,]
        2 + b = a / 0  # ty:ignore[division-by-zero, unresolved-reference]
          |
        ");
    }

    #[test]
    fn add_code_existing_ignore_trailing_whitespace() {
        let test = CodeActionTest::with_source(
            r#"
            b = <START>a<END> / 0  # ty:ignore[division-by-zero   ]
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:2:5
          |
        2 | b = a / 0  # ty:ignore[division-by-zero   ]
          |     ^
          |
          |
        1 |
          - b = a / 0  # ty:ignore[division-by-zero   ]
        2 + b = a / 0  # ty:ignore[division-by-zero, unresolved-reference   ]
          |
        ");
    }

    #[test]
    fn add_code_existing_ignore_with_reason() {
        let test = CodeActionTest::with_source(
            r#"
            b = <START>a<END> / 0  # ty:ignore[division-by-zero] some explanation [123]
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:2:5
          |
        2 | b = a / 0  # ty:ignore[division-by-zero] some explanation [123]
          |     ^
          |
          |
        1 |
          - b = a / 0  # ty:ignore[division-by-zero] some explanation [123]
        2 + b = a / 0  # ty:ignore[division-by-zero] some explanation [123]  # ty:ignore[unresolved-reference]
          |
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

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:3:9
          |
        3 | /         a  # ty:ignore[division-by-zero]
        4 | |         /
        5 | |         0
          | |_________^
          |
          |
        2 | b = (
          -         a  # ty:ignore[division-by-zero]
        3 +         a  # ty:ignore[division-by-zero, unresolved-reference]
        4 |         /
          |
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

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:3:9
          |
        3 | /         a
        4 | |         /
        5 | |         0  # ty:ignore[division-by-zero]
          | |_________^
          |
          |
        4 |         /
          -         0  # ty:ignore[division-by-zero]
        5 +         0  # ty:ignore[division-by-zero, unresolved-reference]
        6 | )
          |
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

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:3:9
          |
        3 | /         a  # ty:ignore[division-by-zero]
        4 | |         /
        5 | |         0  # ty:ignore[division-by-zero]
          | |_________^
          |
          |
        2 | b = (
          -         a  # ty:ignore[division-by-zero]
        3 +         a  # ty:ignore[division-by-zero, unresolved-reference]
        4 |         /
          |
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
        3 |     {a}
          |      ^
          |
          |
        4 |     more text
          - """
        5 + """  # ty:ignore[unresolved-reference]
          |
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

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:4:5
          |
        4 |     a
          |     ^
          |
          |
        3 |     {
          -     a
        4 +     a  # ty:ignore[unresolved-reference]
        5 |     }
          |
        ");
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
          |
          |
        3 |     more text
          - """
        4 + """  # ty:ignore[unresolved-reference]
          |
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
          |
          |
        2 | b = a \
          - + "test"
        3 + + "test"  # ty:ignore[unresolved-reference]
          |
        "#);
    }

    #[test]
    fn add_ignore_line_continuation_empty_lines() {
        let test = CodeActionTest::with_source(
            r#"b = bbbbb \
    [  ccc # test

        + <START>ddd<END>  \

    ] # test
        "#,
        );

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @r"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:4:11
          |
        4 |         + ddd  \
          |           ^^^
          |
          |
        4 |         + ddd  \
          -
        5 +   # ty:ignore[unresolved-reference]
        6 |     ] # test
          |
        ");
    }

    #[test]
    fn undefined_reveal_type() {
        let test = CodeActionTest::with_source(
            r#"
            <START>reveal_type<END>(1)
        "#,
        );

        assert_snapshot!(test.code_actions(&UNDEFINED_REVEAL), @"
        info[code-action]: import typing.reveal_type
         --> main.py:2:1
          |
        2 | reveal_type(1)
          | ^^^^^^^^^^^
          |
        help: This is a preferred code action
          |
        1 + from typing import reveal_type
        2 |
          |

        info[code-action]: Ignore 'undefined-reveal' for this line
         --> main.py:2:1
          |
        2 | reveal_type(1)
          | ^^^^^^^^^^^
          |
          |
        1 |
          - reveal_type(1)
        2 + reveal_type(1)  # ty:ignore[undefined-reveal]
          |
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
          |
        help: This is a preferred code action
          |
        1 + from warnings import deprecated
        2 |
          |

        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:2:2
          |
        2 | @deprecated("do not use")
          |  ^^^^^^^^^^
          |
          |
        1 |
          - @deprecated("do not use")
        2 + @deprecated("do not use")  # ty:ignore[unresolved-reference]
        3 | def my_func(): ...
          |
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
        4 | @deprecated("do not use")
          |  ^^^^^^^^^^
          |
        help: This is a preferred code action
          |
        1 + from warnings import deprecated
        2 |
          |

        info[code-action]: qualify warnings.deprecated
         --> main.py:4:2
          |
        4 | @deprecated("do not use")
          |  ^^^^^^^^^^
          |
        help: This is a preferred code action
          |
        3 |
          - @deprecated("do not use")
        4 + @warnings.deprecated("do not use")
        5 | def my_func(): ...
          |

        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:4:2
          |
        4 | @deprecated("do not use")
          |  ^^^^^^^^^^
          |
          |
        3 |
          - @deprecated("do not use")
        4 + @deprecated("do not use")  # ty:ignore[unresolved-reference]
        5 | def my_func(): ...
          |
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

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: import importlib.abc.ExecutionLoader
         --> main.py:2:1
          |
        2 | ExecutionLoader
          | ^^^^^^^^^^^^^^^
          |
        help: This is a preferred code action
          |
        1 + from importlib.abc import ExecutionLoader
        2 |
          |

        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:2:1
          |
        2 | ExecutionLoader
          | ^^^^^^^^^^^^^^^
          |
          |
        1 |
          - ExecutionLoader
        2 + ExecutionLoader  # ty:ignore[unresolved-reference]
          |
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

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: import importlib.abc.ExecutionLoader
         --> main.py:3:1
          |
        3 | ExecutionLoader
          | ^^^^^^^^^^^^^^^
          |
        help: This is a preferred code action
          |
        1 + from importlib.abc import ExecutionLoader
        2 |
          |

        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:3:1
          |
        3 | ExecutionLoader
          | ^^^^^^^^^^^^^^^
          |
          |
        2 | import importlib
          - ExecutionLoader
        3 + ExecutionLoader  # ty:ignore[unresolved-reference]
          |
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

        assert_snapshot!(test.code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: import importlib.abc.ExecutionLoader
         --> main.py:3:1
          |
        3 | ExecutionLoader
          | ^^^^^^^^^^^^^^^
          |
        help: This is a preferred code action
          |
        1 + from importlib.abc import ExecutionLoader
        2 |
          |

        info[code-action]: qualify importlib.abc.ExecutionLoader
         --> main.py:3:1
          |
        3 | ExecutionLoader
          | ^^^^^^^^^^^^^^^
          |
        help: This is a preferred code action
          |
        2 | import importlib.abc
          - ExecutionLoader
        3 + importlib.abc.ExecutionLoader
          |

        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:3:1
          |
        3 | ExecutionLoader
          | ^^^^^^^^^^^^^^^
          |
          |
        2 | import importlib.abc
          - ExecutionLoader
        3 + ExecutionLoader  # ty:ignore[unresolved-reference]
          |
        ");
    }

    pub(super) struct CodeActionTest {
        pub(super) db: ty_project::TestDb,
        pub(super) file: File,
        pub(super) diagnostic_range: TextRange,
    }

    impl CodeActionTest {
        pub(super) fn with_source(source: &str) -> Self {
            Self::with_source_and_type_ignores(source, true)
        }

        fn with_source_and_type_ignores(source: &str, respect_type_ignores: bool) -> Self {
            let project = ProjectMetadata::from_options(
                Options {
                    analysis: Some(AnalysisOptions {
                        respect_type_ignore_comments: Some(respect_type_ignores),
                        ..AnalysisOptions::default()
                    }),
                    ..Options::default()
                },
                SystemPathBuf::from("/"),
                None,
                &FallibleStrategy,
            )
            .unwrap();
            let mut db = ty_project::TestDb::new(project);

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

        pub(super) fn code_actions(&self, lint: &LintMetadata) -> String {
            self.code_actions_for(&lint.name)
        }

        fn code_actions_for(&self, diagnostic_id: &str) -> String {
            use std::fmt::Write;

            let mut buf = String::new();

            let config = DisplayDiagnosticConfig::new("ty")
                .color(false)
                .show_fix_diff(true)
                .context(0)
                .format(DiagnosticFormat::Full);

            for mut action in
                code_actions(&self.db, self.file, self.diagnostic_range, diagnostic_id)
            {
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
