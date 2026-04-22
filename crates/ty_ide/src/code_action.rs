use crate::completion;

use ruff_db::{files::File, parsed::parsed_module};
use ruff_diagnostics::Edit;
use ruff_python_ast::NodeKind;
use ruff_python_ast::find_node::covering_node;
use ruff_python_ast::token::{Token, TokenKind, Tokens};
use ruff_text_size::{Ranged, TextRange, TextSize};
use ty_project::Db;
use ty_python_semantic::lint::LintId;
use ty_python_semantic::suppress_single;
use ty_python_semantic::types::{UNDEFINED_REVEAL, UNRESOLVED_REFERENCE};

/// A Code Action
#[derive(Debug, Clone)]
pub struct CodeAction {
    pub title: String,
    pub edits: Vec<Edit>,
    pub preferred: bool,
}

pub fn diagnostic_code_actions(
    db: &dyn Db,
    file: File,
    diagnostic_range: TextRange,
    diagnostic_id: &str,
) -> Vec<CodeAction> {
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
    actions.push(CodeAction {
        title: format!("Ignore '{}' for this line", lint_id.name()),
        edits: suppress_single(db, file, lint_id, diagnostic_range).into_edits(),
        preferred: false,
    });

    actions
}

pub fn refactor_code_actions(db: &dyn Db, file: File, range: TextRange) -> Vec<CodeAction> {
    let mut actions = Vec::new();

    actions.extend(unwrap_block(db, file, range));

    actions
}

fn unresolved_fixes(
    db: &dyn Db,
    file: File,
    diagnostic_range: TextRange,
) -> Option<impl Iterator<Item = CodeAction>> {
    let parsed = parsed_module(db, file).load(db);
    let node = covering_node(parsed.syntax().into(), diagnostic_range).node();
    let symbol = &node.expr_name()?.id;

    Some(
        completion::unresolved_fixes(db, file, &parsed, symbol, node)
            .into_iter()
            .map(|import| CodeAction {
                title: import.label,
                edits: vec![import.edit],
                preferred: true,
            }),
    )
}

fn unwrap_block(db: &dyn Db, file: File, range: TextRange) -> Option<CodeAction> {
    if !range.is_empty() {
        return None;
    }
    let parsed = parsed_module(db, file).load(db);
    let colon = parsed
        .tokens()
        .at_offset(range.start())
        .find(|it| it.kind() == TokenKind::Colon)?;

    let target = covering_node(parsed.syntax().into(), colon.range())
        .ancestors()
        .find(|it| {
            matches!(
                it.kind(),
                NodeKind::StmtIf
                    | NodeKind::StmtMatch
                    | NodeKind::StmtTry
                    | NodeKind::StmtWith
                    | NodeKind::StmtFor
                    | NodeKind::StmtWhile
            )
        })?;
    let newline = parsed
        .tokens()
        .before(colon.start())
        .iter()
        .rfind(|it| it.kind().is_any_newline())?;
    let keyword = parsed
        .tokens()
        .after(newline.end())
        .iter()
        .find(|it| it.kind().is_keyword() && target.range().contains_range(it.range()))?;

    let (stmts, delete_range, _) = unwrap_block_trigger_ranges(&target, keyword)
        .find(|(_, _, trigger_range)| range.intersect(*trigger_range).is_some())?;
    let current_indent = IndentSpan::of(parsed.tokens(), target);
    let stmts_indent = IndentSpan::of(parsed.tokens(), stmts.first()?);
    let dedent = stmts_indent.len().checked_sub(current_indent.len())?;

    let stmts_range = TextRange::new(stmts.first()?.start(), stmts.last()?.end());
    let before = Edit::deletion(delete_range.start(), stmts_range.start());
    let after = Edit::deletion(stmts_range.end(), delete_range.end());

    let mut edits = vec![before, after];
    for indent in IndentSpan::indent_spans(parsed.tokens().in_range(stmts_range)) {
        edits.extend(indent.dedent(dedent));
    }

    Some(CodeAction {
        title: "Unwrap this statements block".to_owned(),
        edits,
        preferred: false,
    })
}

fn unwrap_block_trigger_ranges<'a>(
    target: &ruff_python_ast::AnyNodeRef<'a>,
    keyword: &Token,
) -> impl Iterator<Item = (&'a [ruff_python_ast::Stmt], TextRange, TextRange)> {
    let keyword = keyword.range();

    let branches = target.as_stmt_if().into_iter().flat_map(|stmt_if| {
        [&stmt_if.body]
            .into_iter()
            .chain(stmt_if.elif_else_clauses.iter().map(|it| &it.body))
    });
    let branches = branches.chain(target.as_stmt_try().into_iter().flat_map(|stmt_try| {
        [&stmt_try.body]
            .into_iter()
            .chain(
                stmt_try
                    .handlers
                    .iter()
                    .filter_map(|it| Some(&it.as_except_handler()?.body)),
            )
            .chain([&stmt_try.orelse, &stmt_try.finalbody])
    }));
    let branches = branches.chain(
        target
            .as_stmt_while()
            .into_iter()
            .flat_map(|it| [&it.body, &it.orelse])
            .chain(
                target
                    .as_stmt_for()
                    .into_iter()
                    .flat_map(|it| [&it.body, &it.orelse]),
            ),
    );
    let branches = branches.chain(target.as_stmt_with().into_iter().map(|it| &it.body));

    let trigger_ranges = branches.filter_map(move |body| {
        let first = body.first()?.range();
        (keyword.start() < first.start()).then(|| {
            (
                &body[..],
                keyword.cover_offset(target.end()),
                keyword.cover(first),
            )
        })
    });

    let trigger_ranges =
        trigger_ranges.chain(target.as_stmt_match().into_iter().flat_map(|stmt_match| {
            stmt_match
                .cases
                .iter()
                .map(|case| (&case.body[..], stmt_match.range(), case.range))
        }));

    trigger_ranges
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct IndentSpan(TextRange);

impl std::ops::Deref for IndentSpan {
    type Target = TextRange;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl IndentSpan {
    fn of(tokens: &Tokens, range: impl Ranged) -> Self {
        let (before, _) = tokens.split_at(range.start());
        IndentSpan::indent_spans(before).next().unwrap_or_default()
    }

    fn indent_spans(tokens: &[Token]) -> impl Iterator<Item = Self> {
        let mut next_token = None;
        tokens.iter().rev().filter_map(move |token| {
            if !token.kind().is_any_newline() {
                next_token = Some(token);
                return None;
            }
            match next_token.take()? {
                next_token if next_token.kind() == TokenKind::Indent => {
                    Some(IndentSpan(next_token.range()))
                }
                next_token => Some(IndentSpan(TextRange::new(token.end(), next_token.start()))),
            }
        })
    }

    fn dedent(self, len: TextSize) -> Option<Edit> {
        self.len()
            .checked_sub(len)
            .map(|_| Edit::range_deletion(TextRange::at(self.start(), len)))
            .or_else(|| (!self.is_empty()).then(|| Edit::range_deletion(self.range())))
    }
}

#[cfg(test)]
mod tests {

    use crate::{diagnostic_code_actions, refactor_code_actions};

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

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @"
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

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @"
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

        // Not an inline snapshot because of trailing whitespace.
        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE));
    }

    #[test]
    fn add_code_existing_ignore() {
        let test = CodeActionTest::with_source(
            r#"
            b = <START>a<END> / 0  # ty:ignore[division-by-zero]
        "#,
        );

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @"
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
    fn add_code_existing_type_ignore() {
        let test = CodeActionTest::with_source(
            r#"
            b = <START>a<END> / 0  # type:ignore[ty:division-by-zero]
        "#,
        );

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:2:5
          |
        2 | b = a / 0  # type:ignore[ty:division-by-zero]
          |     ^
          |
        1 |
          - b = a / 0  # type:ignore[ty:division-by-zero]
        2 + b = a / 0  # type:ignore[ty:division-by-zero, ty:unresolved-reference]
        ");
    }

    #[test]
    fn add_code_existing_type_ignore_without_any_ty_code() {
        let test = CodeActionTest::with_source(
            r#"
            b = <START>a<END> / 0  # type:ignore[mypy-code]
        "#,
        );

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:2:5
          |
        2 | b = a / 0  # type:ignore[mypy-code]
          |     ^
          |
        1 |
          - b = a / 0  # type:ignore[mypy-code]
        2 + b = a / 0  # type:ignore[mypy-code]  # ty:ignore[unresolved-reference]
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

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:4:5
          |
        2 | # ty:ignore[division-by-zero]
        3 |
        4 | b = a / 0
          |     ^
          |
        1 |
        2 | # ty:ignore[division-by-zero]
        3 |
          - b = a / 0
        4 + b = a / 0  # ty:ignore[unresolved-reference]
        ");
    }

    #[test]
    fn add_code_existing_ignore_trailing_comma() {
        let test = CodeActionTest::with_source(
            r#"
            b = <START>a<END> / 0  # ty:ignore[division-by-zero,]
        "#,
        );

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @"
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

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @"
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

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @"
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

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @"
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

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @"
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

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @"
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

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @r#"
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

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @r#"
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

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @r#"
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

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @r#"
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
    fn add_ignore_line_continuation_empty_lines() {
        let test = CodeActionTest::with_source(
            r#"b = bbbbb \
    [  ccc # test

        + <START>ddd<END>  \

    ] # test
        "#,
        );

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @r"
        info[code-action]: Ignore 'unresolved-reference' for this line
         --> main.py:4:11
          |
        2 |     [  ccc # test
        3 |
        4 |         + ddd  \
          |           ^^^
        5 |
        6 |     ] # test
          |
        2 |     [  ccc # test
        3 |
        4 |         + ddd  \
          -
        5 +   # ty:ignore[unresolved-reference]
        6 |     ] # test
        ");
    }

    #[test]
    fn undefined_reveal_type() {
        let test = CodeActionTest::with_source(
            r#"
            <START>reveal_type<END>(1)
        "#,
        );

        assert_snapshot!(test.diagnostic_code_actions(&UNDEFINED_REVEAL), @"
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

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @r#"
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

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @r#"
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

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @"
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

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @"
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

        assert_snapshot!(test.diagnostic_code_actions(&UNRESOLVED_REFERENCE), @"
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

    #[test]
    fn unwrap_block_if_refactor() {
        let test = CodeActionTest::with_source(
            r#"
            if 1:
                if True<START><END>:
                    # comments
                    foo
                    bar()
                    if 1:
                        baz
                        xxx
                    ...
                    # comments
                ...
        "#,
        );

        assert_snapshot!(test.refactor_code_actions(), @"
        info[code-action(refactor)]: Unwrap this statements block
         --> main.py:3:12
          |
        2 | if 1:
        3 |     if True:
          |            ^
        4 |         # comments
        5 |         foo
          |
        1  |
        2  | if 1:
           -     if True:
           -         # comments
           -         foo
           -         bar()
           -         if 1:
           -             baz
           -             xxx
           -         ...
        3  +     foo
        4  +     bar()
        5  +     if 1:
        6  +         baz
        7  +         xxx
        8  +     ...
        9  |         # comments
        10 |     ...
        ");
    }

    #[test]
    fn unwrap_block_elif_refactor() {
        let test = CodeActionTest::with_source(
            r#"
            if 1:
                if False:
                    xxx()
                elif True<START><END>:
                    foo
                    bar()
                    if 1:
                        baz
                        xxx
                    ...
                else:
                    redundant()
                ...
        "#,
        );

        assert_snapshot!(test.refactor_code_actions(), @"
        info[code-action(refactor)]: Unwrap this statements block
         --> main.py:5:14
          |
        3 |     if False:
        4 |         xxx()
        5 |     elif True:
          |              ^
        6 |         foo
        7 |         bar()
          |
        2  | if 1:
        3  |     if False:
        4  |         xxx()
           -     elif True:
           -         foo
           -         bar()
           -         if 1:
           -             baz
           -             xxx
           -         ...
           -     else:
           -         redundant()
        5  +     foo
        6  +     bar()
        7  +     if 1:
        8  +         baz
        9  +         xxx
        10 +     ...
        11 |     ...
        ");
    }

    #[test]
    fn unwrap_block_try_body_refactor() {
        let test = CodeActionTest::with_source(
            r#"
            if 1:
                try<START><END>:
                    foo
                    bar()
                except:
                    ...
                ...
        "#,
        );

        assert_snapshot!(test.refactor_code_actions(), @"
        info[code-action(refactor)]: Unwrap this statements block
         --> main.py:3:8
          |
        2 | if 1:
        3 |     try:
          |        ^
        4 |         foo
        5 |         bar()
          |
        1 |
        2 | if 1:
          -     try:
          -         foo
          -         bar()
          -     except:
          -         ...
        3 +     foo
        4 +     bar()
        5 |     ...
        ");
    }

    #[test]
    fn unwrap_block_try_except_refactor() {
        let test = CodeActionTest::with_source(
            r#"
            if 1:
                try:
                    ...
                except<START><END>:
                    foo
                    bar()
                ...
        "#,
        );

        assert_snapshot!(test.refactor_code_actions(), @"
        info[code-action(refactor)]: Unwrap this statements block
         --> main.py:5:11
          |
        3 |     try:
        4 |         ...
        5 |     except:
          |           ^
        6 |         foo
        7 |         bar()
          |
        2 | if 1:
        3 |     try:
        4 |         ...
          -     except:
          -         foo
          -         bar()
        5 +     foo
        6 +     bar()
        7 |     ...
        ");
    }

    #[test]
    fn unwrap_block_try_else_refactor() {
        let test = CodeActionTest::with_source(
            r#"
            if 1:
                try:
                    ...
                else<START><END>:
                    foo
                    bar()
                ...
        "#,
        );

        assert_snapshot!(test.refactor_code_actions(), @"
        info[code-action(refactor)]: Unwrap this statements block
         --> main.py:5:9
          |
        3 |     try:
        4 |         ...
        5 |     else:
          |         ^
        6 |         foo
        7 |         bar()
          |
        2 | if 1:
        3 |     try:
        4 |         ...
          -     else:
          -         foo
          -         bar()
        5 +     foo
        6 +     bar()
        7 |     ...
        ");
    }

    #[test]
    fn unwrap_block_try_finally_refactor() {
        let test = CodeActionTest::with_source(
            r#"
            if 1:
                try:
                    ...
                except:
                    ...
                finally<START><END>:
                    foo
                    bar()
                ...
        "#,
        );

        assert_snapshot!(test.refactor_code_actions(), @"
        info[code-action(refactor)]: Unwrap this statements block
         --> main.py:7:12
          |
        5 |     except:
        6 |         ...
        7 |     finally:
          |            ^
        8 |         foo
        9 |         bar()
          |
        4 |         ...
        5 |     except:
        6 |         ...
          -     finally:
          -         foo
          -         bar()
        7 +     foo
        8 +     bar()
        9 |     ...
        ");
    }

    #[test]
    fn unwrap_block_while_refactor() {
        let test = CodeActionTest::with_source(
            r#"
            if 1:
                while True<START><END>:
                    foo
                    bar()
                ...
        "#,
        );

        assert_snapshot!(test.refactor_code_actions(), @"
        info[code-action(refactor)]: Unwrap this statements block
         --> main.py:3:15
          |
        2 | if 1:
        3 |     while True:
          |               ^
        4 |         foo
        5 |         bar()
          |
        1 |
        2 | if 1:
          -     while True:
          -         foo
          -         bar()
        3 +     foo
        4 +     bar()
        5 |     ...
        ");
    }

    #[test]
    fn unwrap_block_while_else_refactor() {
        let test = CodeActionTest::with_source(
            r#"
            if 1:
                while True:
                    ...
                else<START><END>:
                    foo
                    bar()
                ...
        "#,
        );

        assert_snapshot!(test.refactor_code_actions(), @"
        info[code-action(refactor)]: Unwrap this statements block
         --> main.py:5:9
          |
        3 |     while True:
        4 |         ...
        5 |     else:
          |         ^
        6 |         foo
        7 |         bar()
          |
        2 | if 1:
        3 |     while True:
        4 |         ...
          -     else:
          -         foo
          -         bar()
        5 +     foo
        6 +     bar()
        7 |     ...
        ");
    }

    #[test]
    fn unwrap_block_match_case_refactor() {
        let test = CodeActionTest::with_source(
            r#"
            if 1:
                match x:
                    case 0:
                        ...
                    case 1:<START><END>
                        foo
                        bar()
                    case _:
                        ...
                ...
        "#,
        );

        assert_snapshot!(test.refactor_code_actions(), @"
        info[code-action(refactor)]: Unwrap this statements block
         --> main.py:6:16
          |
        4 |         case 0:
        5 |             ...
        6 |         case 1:
          |                ^
        7 |             foo
        8 |             bar()
          |
        1 |
        2 | if 1:
          -     match x:
          -         case 0:
          -             ...
          -         case 1:
          -             foo
          -             bar()
          -         case _:
          -             ...
        3 +     foo
        4 +     bar()
        5 |     ...
        ");
    }

    #[test]
    fn unwrap_block_if_refactor_without_newline() {
        let test = CodeActionTest::with_source(
            r#"
            if 1:
                if True<START><END>: foo; bar()
                ...
        "#,
        );

        assert_snapshot!(test.refactor_code_actions(), @"
        info[code-action(refactor)]: Unwrap this statements block
         --> main.py:3:12
          |
        2 | if 1:
        3 |     if True: foo; bar()
          |            ^
        4 |     ...
          |
        1 |
        2 | if 1:
          -     if True: foo; bar()
        3 +     foo; bar()
        4 |     ...
        ");
    }

    pub(super) struct CodeActionTest {
        pub(super) db: ty_project::TestDb,
        pub(super) file: File,
        pub(super) range: TextRange,
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
                range: TextRange::new(
                    TextSize::try_from(start).unwrap(),
                    TextSize::try_from(end).unwrap(),
                ),
            }
        }

        pub(super) fn diagnostic_code_actions(&self, lint: &'static LintMetadata) -> String {
            use std::fmt::Write;

            let mut buf = String::new();

            let config = DisplayDiagnosticConfig::new("ty")
                .color(false)
                .show_fix_diff(true)
                .format(DiagnosticFormat::Full);

            for mut action in diagnostic_code_actions(&self.db, self.file, self.range, &lint.name) {
                let mut diagnostic = Diagnostic::new(
                    DiagnosticId::Lint(LintName::of("code-action")),
                    ruff_db::diagnostic::Severity::Info,
                    action.title,
                );

                diagnostic.annotate(Annotation::primary(
                    Span::from(self.file).with_range(self.range),
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

        pub(super) fn refactor_code_actions(&self) -> String {
            use std::fmt::Write;

            let mut buf = String::new();

            let config = DisplayDiagnosticConfig::new("ty")
                .color(false)
                .show_fix_diff(true)
                .format(DiagnosticFormat::Full);

            for mut action in refactor_code_actions(&self.db, self.file, self.range) {
                let mut diagnostic = Diagnostic::new(
                    DiagnosticId::Lint(LintName::of("code-action(refactor)")),
                    ruff_db::diagnostic::Severity::Info,
                    action.title,
                );

                diagnostic.annotate(Annotation::primary(
                    Span::from(self.file).with_range(self.range),
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
