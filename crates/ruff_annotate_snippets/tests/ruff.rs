use annotate_snippets::{AnnotationKind, Level, Patch, Renderer, Snippet};

use annotate_snippets::renderer::DecorStyle;
use snapbox::{assert_data_eq, str};

#[test]
fn snippet_with_cell() {
    let source = "First line\r\nSecond oops line";
    let input = &[Level::ERROR.primary_title("oops").element(
        Snippet::source(source)
            .cell_index(Some(1))
            .annotation(AnnotationKind::Primary.span(19..23).label("oops")),
    )];
    let expected_ascii = str![[r#"
error: oops
  |
2 | Second oops line
  |        ^^^^ oops
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: oops
  ╭▸ 
2 │ Second oops line
  ╰╴       ━━━━ oops
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn snippet_with_path_cell() {
    let source = "First line\r\nSecond oops line";
    let input = &[Level::ERROR.primary_title("oops").element(
        Snippet::source(source)
            .path("foo.ipynb")
            .cell_index(Some(1))
            .annotation(AnnotationKind::Primary.span(19..23).label("oops")),
    )];
    let expected_ascii = str![[r#"
error: oops
 --> foo.ipynb:cell 1:2:8
  |
2 | Second oops line
  |        ^^^^ oops
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: oops
  ╭▸ foo.ipynb:cell 1:2:8
  │
2 │ Second oops line
  ╰╴       ━━━━ oops
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn patch_with_primary_cell() {
    let source = "First line\r\nSecond oops line";
    let input = &[
        Level::ERROR.primary_title("oops").element(
            Snippet::source(source)
                .cell_index(Some(1))
                .annotation(AnnotationKind::Primary.span(19..23).label("oops")),
        ),
        Level::HELP
            .secondary_title("remove the entry")
            .element(
                Snippet::source(source)
                    .cell_index(Some(1))
                    .patch(Patch::new(19..24, "")),
            )
            .element(
                Snippet::source(source)
                    .cell_index(Some(1))
                    .patch(Patch::new(23..28, "")),
            ),
    ];
    let expected_ascii = str![[r#"
error: oops
  |
2 | Second oops line
  |        ^^^^ oops
  |
help: remove the entry
 ::: cell 1:2:8
  |
2 - Second oops line
2 + Second line
  |
2 - Second oops line
2 + Second oops
  |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: oops
  ╭▸ 
2 │ Second oops line
  │        ━━━━ oops
  ╰╴
help: remove the entry
  ⸬  cell 1:2:8
  │
2 - Second oops line
2 + Second line
  ├╴
2 - Second oops line
2 + Second oops
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn patch_with_primary_path_cell() {
    let source = "First line\r\nSecond oops line";
    let input = &[
        Level::ERROR.primary_title("oops").element(
            Snippet::source(source)
                .path("foo.ipynb")
                .cell_index(Some(1))
                .annotation(AnnotationKind::Primary.span(19..23).label("oops")),
        ),
        Level::HELP
            .secondary_title("remove the entry")
            .element(
                Snippet::source(source)
                    .path("foo.ipynb")
                    .cell_index(Some(1))
                    .patch(Patch::new(19..24, "")),
            )
            .element(
                Snippet::source(source)
                    .path("foo.ipynb")
                    .cell_index(Some(1))
                    .patch(Patch::new(23..28, "")),
            ),
    ];
    let expected_ascii = str![[r#"
error: oops
 --> foo.ipynb:cell 1:2:8
  |
2 | Second oops line
  |        ^^^^ oops
  |
help: remove the entry
 ::: cell 1:2:8
  |
2 - Second oops line
2 + Second line
  |
2 - Second oops line
2 + Second oops
  |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: oops
  ╭▸ foo.ipynb:cell 1:2:8
  │
2 │ Second oops line
  │        ━━━━ oops
  ╰╴
help: remove the entry
  ⸬  cell 1:2:8
  │
2 - Second oops line
2 + Second line
  ├╴
2 - Second oops line
2 + Second oops
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn patch_with_primary_path_incrementing_cell() {
    let source = "First line\r\nSecond oops line";
    let input = &[
        Level::ERROR.primary_title("oops").element(
            Snippet::source(source)
                .path("foo.ipynb")
                .cell_index(Some(1))
                .annotation(AnnotationKind::Primary.span(19..23).label("oops")),
        ),
        Level::HELP
            .secondary_title("remove the entry")
            .element(
                Snippet::source(source)
                    .path("foo.ipynb")
                    .cell_index(Some(1))
                    .patch(Patch::new(19..24, "")),
            )
            .element(
                Snippet::source(source)
                    .path("foo.ipynb")
                    .cell_index(Some(2))
                    .patch(Patch::new(23..28, "")),
            ),
    ];
    let expected_ascii = str![[r#"
error: oops
 --> foo.ipynb:cell 1:2:8
  |
2 | Second oops line
  |        ^^^^ oops
  |
help: remove the entry
 ::: cell 1:2:8
  |
2 - Second oops line
2 + Second line
  |
 ::: cell 2:2:12
  |
2 - Second oops line
2 + Second oops
  |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: oops
  ╭▸ foo.ipynb:cell 1:2:8
  │
2 │ Second oops line
  │        ━━━━ oops
  ╰╴
help: remove the entry
  ⸬  cell 1:2:8
  │
2 - Second oops line
2 + Second line
  │
  ⸬  cell 2:2:12
  │
2 - Second oops line
2 + Second oops
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn patch_with_other_cell() {
    let source = "First line\r\nSecond oops line";
    let input = &[
        Level::ERROR.primary_title("oops").element(
            Snippet::source(source)
                .cell_index(Some(1))
                .annotation(AnnotationKind::Primary.span(19..23).label("oops")),
        ),
        Level::HELP
            .secondary_title("remove the entry")
            .element(
                Snippet::source(source)
                    .cell_index(Some(2))
                    .patch(Patch::new(19..24, "")),
            )
            .element(
                Snippet::source(source)
                    .cell_index(Some(2))
                    .patch(Patch::new(23..28, "")),
            ),
    ];
    let expected_ascii = str![[r#"
error: oops
  |
2 | Second oops line
  |        ^^^^ oops
  |
help: remove the entry
 ::: cell 2:2:8
  |
2 - Second oops line
2 + Second line
  |
2 - Second oops line
2 + Second oops
  |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: oops
  ╭▸ 
2 │ Second oops line
  │        ━━━━ oops
  ╰╴
help: remove the entry
  ⸬  cell 2:2:8
  │
2 - Second oops line
2 + Second line
  ├╴
2 - Second oops line
2 + Second oops
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn patch_with_other_path_cell() {
    let source = "First line\r\nSecond oops line";
    let input = &[
        Level::ERROR.primary_title("oops").element(
            Snippet::source(source)
                .path("foo.ipynb")
                .cell_index(Some(1))
                .annotation(AnnotationKind::Primary.span(19..23).label("oops")),
        ),
        Level::HELP
            .secondary_title("remove the entry")
            .element(
                Snippet::source(source)
                    .path("bar.ipynb")
                    .cell_index(Some(2))
                    .patch(Patch::new(19..24, "")),
            )
            .element(
                Snippet::source(source)
                    .path("bar.ipynb")
                    .cell_index(Some(2))
                    .patch(Patch::new(23..28, "")),
            ),
    ];
    let expected_ascii = str![[r#"
error: oops
 --> foo.ipynb:cell 1:2:8
  |
2 | Second oops line
  |        ^^^^ oops
  |
help: remove the entry
 ::: bar.ipynb:cell 2:2:8
  |
2 - Second oops line
2 + Second line
  |
2 - Second oops line
2 + Second oops
  |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: oops
  ╭▸ foo.ipynb:cell 1:2:8
  │
2 │ Second oops line
  │        ━━━━ oops
  ╰╴
help: remove the entry
  ⸬  bar.ipynb:cell 2:2:8
  │
2 - Second oops line
2 + Second line
  ├╴
2 - Second oops line
2 + Second oops
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn patch_with_other_path_incrementing_cell() {
    let source = "First line\r\nSecond oops line";
    let input = &[
        Level::ERROR.primary_title("oops").element(
            Snippet::source(source)
                .path("foo.ipynb")
                .cell_index(Some(1))
                .annotation(AnnotationKind::Primary.span(19..23).label("oops")),
        ),
        Level::HELP
            .secondary_title("remove the entry")
            .element(
                Snippet::source(source)
                    .path("bar.ipynb")
                    .cell_index(Some(2))
                    .patch(Patch::new(19..24, "")),
            )
            .element(
                Snippet::source(source)
                    .path("bar.ipynb")
                    .cell_index(Some(3))
                    .patch(Patch::new(23..28, "")),
            ),
    ];
    let expected_ascii = str![[r#"
error: oops
 --> foo.ipynb:cell 1:2:8
  |
2 | Second oops line
  |        ^^^^ oops
  |
help: remove the entry
 ::: bar.ipynb:cell 2:2:8
  |
2 - Second oops line
2 + Second line
  |
 ::: bar.ipynb:cell 3:2:12
  |
2 - Second oops line
2 + Second oops
  |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: oops
  ╭▸ foo.ipynb:cell 1:2:8
  │
2 │ Second oops line
  │        ━━━━ oops
  ╰╴
help: remove the entry
  ⸬  bar.ipynb:cell 2:2:8
  │
2 - Second oops line
2 + Second line
  │
  ⸬  bar.ipynb:cell 3:2:12
  │
2 - Second oops line
2 + Second oops
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn insertion_with_trailing_whitespace() {
    let source = "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10\nline 11\nline 12\nline 13\n";
    let input = &[
        Level::ERROR
            .primary_title("main diagnostic message")
            .id("test-diagnostic")
            .element(
                Snippet::source(source)
                    .path("example.py")
                    .annotation(AnnotationKind::Primary.span(7..13)),
            ),
        Level::HELP
            .secondary_title("Replace three lines")
            .element(Snippet::source(source).patch(Patch::new(7..7, "fixed "))),
    ];
    let expected_ascii = str![[r#"
error[test-diagnostic]: main diagnostic message
 --> example.py:2:1
  |
2 | line 2
  | ^^^^^^
  |
help: Replace three lines
  |
2 | fixed line 2
  | +++++
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[test-diagnostic]: main diagnostic message
  ╭▸ example.py:2:1
  │
2 │ line 2
  │ ━━━━━━
  ╰╴
help: Replace three lines
  ╭╴
2 │ fixed line 2
  ╰╴+++++
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn multiple_insertions() {
    let source = "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10\nline 11\nline 12\nline 13\n";
    let input = &[
        Level::ERROR
            .primary_title("main diagnostic message")
            .id("test-diagnostic")
            .element(
                Snippet::source(source)
                    .path("example.py")
                    .annotation(AnnotationKind::Primary.span(7..13)),
            ),
        Level::HELP.secondary_title("Replace three lines").element(
            Snippet::source(source)
                .patch(Patch::new(7..7, "fixed "))
                .patch(Patch::new(42..42, "fixed "))
                .patch(Patch::new(87..87, "fixed ")),
        ),
    ];
    let expected_ascii = str![[r#"
error[test-diagnostic]: main diagnostic message
  --> example.py:2:1
   |
 2 | line 2
   | ^^^^^^
   |
help: Replace three lines
   |
 2 ~ fixed line 2
 3 | line 3
...
 6 | line 6
 7 ~ fixed line 7
 8 | line 8
...
12 | line 12
13 ~ fixed line 13
   |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[test-diagnostic]: main diagnostic message
   ╭▸ example.py:2:1
   │
 2 │ line 2
   │ ━━━━━━
   ╰╴
help: Replace three lines
   ╭╴
 2 ± fixed line 2
 3 │ line 3
 …
 6 │ line 6
 7 ± fixed line 7
 8 │ line 8
 …
12 │ line 12
13 ± fixed line 13
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
