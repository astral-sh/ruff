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
  ╭╴
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
  ╭╴
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
  ╭╴
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
  ╭╴
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
 --> bar.ipynb:2:8
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
  ╭▸ bar.ipynb:2:8
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
 --> bar.ipynb:2:8
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
  ╭▸ bar.ipynb:2:8
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
