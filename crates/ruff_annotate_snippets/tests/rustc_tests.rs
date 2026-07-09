//! These tests have been adapted from [Rust's parser tests][parser-tests].
//!
//! [parser-tests]: https://github.com/rust-lang/rust/blob/894f7a4ba6554d3797404bbf550d9919df060b97/compiler/rustc_parse/src/parser/tests.rs

use annotate_snippets::{AnnotationKind, Group, Level, Origin, Padding, Patch, Renderer, Snippet};

use annotate_snippets::renderer::DecorStyle;
use snapbox::{IntoData, assert_data_eq, str};

#[test]
fn ends_on_col0() {
    let source = r#"
fn foo() {
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(AnnotationKind::Primary.span(10..13).label("test")),
    )];

    let expected_ascii = str![[r#"
error: foo
 --> test.rs:2:10
  |
2 |   fn foo() {
  |  __________^
3 | | }
  | |_^ test
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
  ╭▸ test.rs:2:10
  │
2 │   fn foo() {
  │ ┏━━━━━━━━━━┛
3 │ ┃ }
  ╰╴┗━┛ test
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn ends_on_col2() {
    let source = r#"
fn foo() {


  }
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(AnnotationKind::Primary.span(10..17).label("test")),
    )];
    let expected_ascii = str![[r#"
error: foo
 --> test.rs:2:10
  |
2 |   fn foo() {
  |  __________^
... |
5 | |   }
  | |___^ test
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
  ╭▸ test.rs:2:10
  │
2 │   fn foo() {
  │ ┏━━━━━━━━━━┛
  ┆ ┇
5 │ ┃   }
  ╰╴┗━━━┛ test
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn non_nested() {
    let source = r#"
fn foo() {
  X0 Y0
  X1 Y1
  X2 Y2
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(
                AnnotationKind::Primary
                    .span(14..32)
                    .label("`X` is a good letter"),
            )
            .annotation(
                AnnotationKind::Context
                    .span(17..35)
                    .label("`Y` is a good letter too"),
            ),
    )];

    let expected_ascii = str![[r#"
error: foo
 --> test.rs:3:3
  |
3 |      X0 Y0
  |  ____^  -
  | | ______|
4 | ||   X1 Y1
5 | ||   X2 Y2
  | ||____^__- `Y` is a good letter too
  | |_____|
  |       `X` is a good letter
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
  ╭▸ test.rs:3:3
  │
3 │      X0 Y0
  │ ┏━━━━┛  │
  │ ┃┌──────┘
4 │ ┃│   X1 Y1
5 │ ┃│   X2 Y2
  │ ┃└────╿──┘ `Y` is a good letter too
  │ ┗━━━━━┥
  ╰╴      `X` is a good letter
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn nested() {
    let source = r#"
fn foo() {
  X0 Y0
  Y1 X1
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(
                AnnotationKind::Primary
                    .span(14..27)
                    .label("`X` is a good letter"),
            )
            .annotation(
                AnnotationKind::Context
                    .span(17..24)
                    .label("`Y` is a good letter too"),
            ),
    )];

    let expected_ascii = str![[r#"
error: foo
 --> test.rs:3:3
  |
3 |      X0 Y0
  |  ____^  -
  | | ______|
4 | ||   Y1 X1
  | ||____-__^ `X` is a good letter
  |  |____|
  |       `Y` is a good letter too
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
  ╭▸ test.rs:3:3
  │
3 │      X0 Y0
  │ ┏━━━━┛  │
  │ ┃┌──────┘
4 │ ┃│   Y1 X1
  │ ┗│━━━━│━━┛ `X` is a good letter
  │  └────┤
  ╰╴      `Y` is a good letter too
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn different_overlap() {
    let source = r#"
fn foo() {
  X0 Y0 Z0
  X1 Y1 Z1
  X2 Y2 Z2
  X3 Y3 Z3
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(
                AnnotationKind::Primary
                    .span(17..38)
                    .label("`X` is a good letter"),
            )
            .annotation(
                AnnotationKind::Context
                    .span(31..49)
                    .label("`Y` is a good letter too"),
            ),
    )];

    let expected_ascii = str![[r#"
error: foo
 --> test.rs:3:6
  |
3 |      X0 Y0 Z0
  |  _______^
4 | |    X1 Y1 Z1
  | | _________-
5 | ||   X2 Y2 Z2
  | ||____^ `X` is a good letter
6 |  |   X3 Y3 Z3
  |  |____- `Y` is a good letter too
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
  ╭▸ test.rs:3:6
  │
3 │      X0 Y0 Z0
  │ ┏━━━━━━━┛
4 │ ┃    X1 Y1 Z1
  │ ┃┌─────────┘
5 │ ┃│   X2 Y2 Z2
  │ ┗│━━━━┛ `X` is a good letter
6 │  │   X3 Y3 Z3
  ╰╴ └────┘ `Y` is a good letter too
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn triple_overlap() {
    let source = r#"
fn foo() {
  X0 Y0 Z0
  X1 Y1 Z1
  X2 Y2 Z2
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(
                AnnotationKind::Primary
                    .span(14..38)
                    .label("`X` is a good letter"),
            )
            .annotation(
                AnnotationKind::Context
                    .span(17..41)
                    .label("`Y` is a good letter too"),
            )
            .annotation(AnnotationKind::Context.span(20..44).label("`Z` label")),
    )];

    let expected_ascii = str![[r#"
error: foo
 --> test.rs:3:3
  |
3 |       X0 Y0 Z0
  |  _____^  -  -
  | | _______|  |
  | || _________|
4 | |||   X1 Y1 Z1
5 | |||   X2 Y2 Z2
  | |||____^__-__- `Z` label
  | ||_____|__|
  | |______|  `Y` is a good letter too
  |        `X` is a good letter
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
  ╭▸ test.rs:3:3
  │
3 │       X0 Y0 Z0
  │ ┏━━━━━┛  │  │
  │ ┃┌───────┘  │
  │ ┃│┌─────────┘
4 │ ┃││   X1 Y1 Z1
5 │ ┃││   X2 Y2 Z2
  │ ┃│└────╿──│──┘ `Z` label
  │ ┃└─────│──┤
  │ ┗━━━━━━┥  `Y` is a good letter too
  ╰╴       `X` is a good letter
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn triple_exact_overlap() {
    let source = r#"
fn foo() {
  X0 Y0 Z0
  X1 Y1 Z1
  X2 Y2 Z2
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(
                AnnotationKind::Primary
                    .span(14..38)
                    .label("`X` is a good letter"),
            )
            .annotation(
                AnnotationKind::Context
                    .span(14..38)
                    .label("`Y` is a good letter too"),
            )
            .annotation(AnnotationKind::Context.span(14..38).label("`Z` label")),
    )];

    // This should have a `^` but we currently don't support the idea of a
    // "primary" annotation, which would solve this
    let expected_ascii = str![[r#"
error: foo
 --> test.rs:3:3
  |
3 | /   X0 Y0 Z0
4 | |   X1 Y1 Z1
5 | |   X2 Y2 Z2
  | |    ^
  | |    |
  | |    `X` is a good letter
  | |____`Y` is a good letter too
  |      `Z` label
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
  ╭▸ test.rs:3:3
  │
3 │ ┏   X0 Y0 Z0
4 │ ┃   X1 Y1 Z1
5 │ ┃   X2 Y2 Z2
  │ ┃    ╿
  │ ┃    │
  │ ┃    `X` is a good letter
  │ ┗━━━━`Y` is a good letter too
  ╰╴     `Z` label
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn minimum_depth() {
    let source = r#"
fn foo() {
  X0 Y0 Z0
  X1 Y1 Z1
  X2 Y2 Z2
  X3 Y3 Z3
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(
                AnnotationKind::Primary
                    .span(17..27)
                    .label("`X` is a good letter"),
            )
            .annotation(
                AnnotationKind::Context
                    .span(28..44)
                    .label("`Y` is a good letter too"),
            )
            .annotation(AnnotationKind::Context.span(36..52).label("`Z`")),
    )];

    let expected_ascii = str![[r#"
error: foo
 --> test.rs:3:6
  |
3 |      X0 Y0 Z0
  |  _______^
4 | |    X1 Y1 Z1
  | | ____^_-
  | ||____|
  |  |    `X` is a good letter
5 |  |   X2 Y2 Z2
  |  |___-______- `Y` is a good letter too
  |   ___|
  |  |
6 |  |   X3 Y3 Z3
  |  |_______- `Z`
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
  ╭▸ test.rs:3:6
  │
3 │      X0 Y0 Z0
  │ ┏━━━━━━━┛
4 │ ┃    X1 Y1 Z1
  │ ┃┌────╿─┘
  │ ┗│━━━━┥
  │  │    `X` is a good letter
5 │  │   X2 Y2 Z2
  │  └───│──────┘ `Y` is a good letter too
  │  ┌───┘
  │  │
6 │  │   X3 Y3 Z3
  ╰╴ └───────┘ `Z`
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn non_overlapping() {
    let source = r#"
fn foo() {
  X0 Y0 Z0
  X1 Y1 Z1
  X2 Y2 Z2
  X3 Y3 Z3
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(
                AnnotationKind::Primary
                    .span(14..27)
                    .label("`X` is a good letter"),
            )
            .annotation(
                AnnotationKind::Context
                    .span(39..55)
                    .label("`Y` is a good letter too"),
            ),
    )];

    let expected_ascii = str![[r#"
error: foo
 --> test.rs:3:3
  |
3 | /   X0 Y0 Z0
4 | |   X1 Y1 Z1
  | |____^ `X` is a good letter
5 |     X2 Y2 Z2
  |  ______-
6 | |   X3 Y3 Z3
  | |__________- `Y` is a good letter too
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
  ╭▸ test.rs:3:3
  │
3 │ ┏   X0 Y0 Z0
4 │ ┃   X1 Y1 Z1
  │ ┗━━━━┛ `X` is a good letter
5 │     X2 Y2 Z2
  │ ┌──────┘
6 │ │   X3 Y3 Z3
  ╰╴└──────────┘ `Y` is a good letter too
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn overlapping_start_and_end() {
    let source = r#"
fn foo() {
  X0 Y0 Z0
  X1 Y1 Z1
  X2 Y2 Z2
  X3 Y3 Z3
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(
                AnnotationKind::Primary
                    .span(17..27)
                    .label("`X` is a good letter"),
            )
            .annotation(
                AnnotationKind::Context
                    .span(31..55)
                    .label("`Y` is a good letter too"),
            ),
    )];

    let expected_ascii = str![[r#"
error: foo
 --> test.rs:3:6
  |
3 |      X0 Y0 Z0
  |  _______^
4 | |    X1 Y1 Z1
  | | ____^____-
  | ||____|
  |  |    `X` is a good letter
5 |  |   X2 Y2 Z2
6 |  |   X3 Y3 Z3
  |  |__________- `Y` is a good letter too
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
  ╭▸ test.rs:3:6
  │
3 │      X0 Y0 Z0
  │ ┏━━━━━━━┛
4 │ ┃    X1 Y1 Z1
  │ ┃┌────╿────┘
  │ ┗│━━━━┥
  │  │    `X` is a good letter
5 │  │   X2 Y2 Z2
6 │  │   X3 Y3 Z3
  ╰╴ └──────────┘ `Y` is a good letter too
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn multiple_labels_primary_without_message() {
    let source = r#"
fn foo() {
  a { b { c } d }
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(AnnotationKind::Primary.span(18..25).label(""))
            .annotation(
                AnnotationKind::Context
                    .span(14..27)
                    .label("`a` is a good letter"),
            )
            .annotation(AnnotationKind::Context.span(22..23).label("")),
    )];

    let expected_ascii = str![[r#"
error: foo
 --> test.rs:3:7
  |
3 |   a { b { c } d }
  |   ----^^^^-^^-- `a` is a good letter
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
  ╭▸ test.rs:3:7
  │
3 │   a { b { c } d }
  ╰╴  ────━━━━─━━── `a` is a good letter
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn multiple_labels_secondary_without_message() {
    let source = r#"
fn foo() {
  a { b { c } d }
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(
                AnnotationKind::Primary
                    .span(14..27)
                    .label("`a` is a good letter"),
            )
            .annotation(AnnotationKind::Context.span(18..25).label("")),
    )];

    let expected_ascii = str![[r#"
error: foo
 --> test.rs:3:3
  |
3 |   a { b { c } d }
  |   ^^^^-------^^ `a` is a good letter
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
  ╭▸ test.rs:3:3
  │
3 │   a { b { c } d }
  ╰╴  ━━━━───────━━ `a` is a good letter
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn multiple_labels_primary_without_message_2() {
    let source = r#"
fn foo() {
  a { b { c } d }
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(
                AnnotationKind::Primary
                    .span(18..25)
                    .label("`b` is a good letter"),
            )
            .annotation(AnnotationKind::Context.span(14..27).label(""))
            .annotation(AnnotationKind::Context.span(22..23).label("")),
    )];

    let expected_ascii = str![[r#"
error: foo
 --> test.rs:3:7
  |
3 |   a { b { c } d }
  |   ----^^^^-^^--
  |       |
  |       `b` is a good letter
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
  ╭▸ test.rs:3:7
  │
3 │   a { b { c } d }
  │   ────┯━━━─━━──
  │       │
  ╰╴      `b` is a good letter
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn multiple_labels_secondary_without_message_2() {
    let source = r#"
fn foo() {
  a { b { c } d }
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(AnnotationKind::Primary.span(14..27).label(""))
            .annotation(
                AnnotationKind::Context
                    .span(18..25)
                    .label("`b` is a good letter"),
            ),
    )];

    let expected_ascii = str![[r#"
error: foo
 --> test.rs:3:3
  |
3 |   a { b { c } d }
  |   ^^^^-------^^
  |       |
  |       `b` is a good letter
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
  ╭▸ test.rs:3:3
  │
3 │   a { b { c } d }
  │   ━━━━┬──────━━
  │       │
  ╰╴      `b` is a good letter
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn multiple_labels_secondary_without_message_3() {
    let source = r#"
fn foo() {
  a  bc  d
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(
                AnnotationKind::Primary
                    .span(14..18)
                    .label("`a` is a good letter"),
            )
            .annotation(AnnotationKind::Context.span(18..22).label("")),
    )];

    let expected_ascii = str![[r#"
error: foo
 --> test.rs:3:3
  |
3 |   a  bc  d
  |   ^^^^----
  |   |
  |   `a` is a good letter
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
  ╭▸ test.rs:3:3
  │
3 │   a  bc  d
  │   ┯━━━────
  │   │
  ╰╴  `a` is a good letter
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn multiple_labels_without_message() {
    let source = r#"
fn foo() {
  a { b { c } d }
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(AnnotationKind::Primary.span(14..27).label(""))
            .annotation(AnnotationKind::Context.span(18..25).label("")),
    )];

    let expected_ascii = str![[r#"
error: foo
 --> test.rs:3:3
  |
3 |   a { b { c } d }
  |   ^^^^-------^^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
  ╭▸ test.rs:3:3
  │
3 │   a { b { c } d }
  ╰╴  ━━━━───────━━
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn multiple_labels_without_message_2() {
    let source = r#"
fn foo() {
  a { b { c } d }
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(AnnotationKind::Primary.span(18..25).label(""))
            .annotation(AnnotationKind::Context.span(14..27).label(""))
            .annotation(AnnotationKind::Context.span(22..23).label("")),
    )];

    let expected_ascii = str![[r#"
error: foo
 --> test.rs:3:7
  |
3 |   a { b { c } d }
  |   ----^^^^-^^--
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
  ╭▸ test.rs:3:7
  │
3 │   a { b { c } d }
  ╰╴  ────━━━━─━━──
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn multiple_labels_with_message() {
    let source = r#"
fn foo() {
  a { b { c } d }
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(
                AnnotationKind::Primary
                    .span(14..27)
                    .label("`a` is a good letter"),
            )
            .annotation(
                AnnotationKind::Context
                    .span(18..25)
                    .label("`b` is a good letter"),
            ),
    )];

    let expected_ascii = str![[r#"
error: foo
 --> test.rs:3:3
  |
3 |   a { b { c } d }
  |   ^^^^-------^^
  |   |   |
  |   |   `b` is a good letter
  |   `a` is a good letter
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
  ╭▸ test.rs:3:3
  │
3 │   a { b { c } d }
  │   ┯━━━┬──────━━
  │   │   │
  │   │   `b` is a good letter
  ╰╴  `a` is a good letter
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn ingle_label_with_message() {
    let source = r#"
fn foo() {
  a { b { c } d }
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(
                AnnotationKind::Primary
                    .span(14..27)
                    .label("`a` is a good letter"),
            ),
    )];

    let expected_ascii = str![[r#"
error: foo
 --> test.rs:3:3
  |
3 |   a { b { c } d }
  |   ^^^^^^^^^^^^^ `a` is a good letter
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
  ╭▸ test.rs:3:3
  │
3 │   a { b { c } d }
  ╰╴  ━━━━━━━━━━━━━ `a` is a good letter
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn single_label_without_message() {
    let source = r#"
fn foo() {
  a { b { c } d }
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(AnnotationKind::Primary.span(14..27).label("")),
    )];

    let expected_ascii = str![[r#"
error: foo
 --> test.rs:3:3
  |
3 |   a { b { c } d }
  |   ^^^^^^^^^^^^^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
  ╭▸ test.rs:3:3
  │
3 │   a { b { c } d }
  ╰╴  ━━━━━━━━━━━━━
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn long_snippet() {
    let source = r#"
fn foo() {
  X0 Y0 Z0
  X1 Y1 Z1
1
2
3
4
5
6
7
8
9
10
  X2 Y2 Z2
  X3 Y3 Z3
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(
                AnnotationKind::Primary
                    .span(17..27)
                    .label("`X` is a good letter"),
            )
            .annotation(
                AnnotationKind::Context
                    .span(31..76)
                    .label("`Y` is a good letter too"),
            ),
    )];

    let expected_ascii = str![[r#"
error: foo
  --> test.rs:3:6
   |
 3 |      X0 Y0 Z0
   |  _______^
 4 | |    X1 Y1 Z1
   | | ____^____-
   | ||____|
   |  |    `X` is a good letter
 5 |  | 1
 6 |  | 2
 7 |  | 3
...   |
15 |  |   X2 Y2 Z2
16 |  |   X3 Y3 Z3
   |  |__________- `Y` is a good letter too
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
   ╭▸ test.rs:3:6
   │
 3 │      X0 Y0 Z0
   │ ┏━━━━━━━┛
 4 │ ┃    X1 Y1 Z1
   │ ┃┌────╿────┘
   │ ┗│━━━━┥
   │  │    `X` is a good letter
 5 │  │ 1
 6 │  │ 2
 7 │  │ 3
   ┆  ┆
15 │  │   X2 Y2 Z2
16 │  │   X3 Y3 Z3
   ╰╴ └──────────┘ `Y` is a good letter too
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
#[test]
fn long_snippet_multiple_spans() {
    let source = r#"
fn foo() {
  X0 Y0 Z0
1
2
3
  X1 Y1 Z1
4
5
6
  X2 Y2 Z2
7
8
9
10
  X3 Y3 Z3
}
"#;
    let input = &[Level::ERROR.primary_title("foo").element(
        Snippet::source(source)
            .line_start(1)
            .path("test.rs")
            .annotation(
                AnnotationKind::Primary
                    .span(17..73)
                    .label("`Y` is a good letter"),
            )
            .annotation(
                AnnotationKind::Context
                    .span(37..56)
                    .label("`Z` is a good letter too"),
            ),
    )];

    let expected_ascii = str![[r#"
error: foo
  --> test.rs:3:6
   |
 3 |      X0 Y0 Z0
   |  _______^
 4 | |  1
 5 | |  2
 6 | |  3
 7 | |    X1 Y1 Z1
   | | _________-
 8 | || 4
 9 | || 5
10 | || 6
11 | ||   X2 Y2 Z2
   | ||__________- `Z` is a good letter too
...  |
15 | |  10
16 | |    X3 Y3 Z3
   | |________^ `Y` is a good letter
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: foo
   ╭▸ test.rs:3:6
   │
 3 │      X0 Y0 Z0
   │ ┏━━━━━━━┛
 4 │ ┃  1
 5 │ ┃  2
 6 │ ┃  3
 7 │ ┃    X1 Y1 Z1
   │ ┃┌─────────┘
 8 │ ┃│ 4
 9 │ ┃│ 5
10 │ ┃│ 6
11 │ ┃│   X2 Y2 Z2
   │ ┃└──────────┘ `Z` is a good letter too
   ┆ ┇
15 │ ┃  10
16 │ ┃    X3 Y3 Z3
   ╰╴┗━━━━━━━━┛ `Y` is a good letter
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn issue_91334() {
    let source = r#"// Regression test for the ICE described in issue #91334.

//@ error-pattern: this file contains an unclosed delimiter

#![feature(coroutines)]

fn f(){||yield(((){),
"#;
    let input = &[Level::ERROR
        .primary_title("this file contains an unclosed delimiter")
        .element(
            Snippet::source(source)
                .line_start(1)
                .path("$DIR/issue-91334.rs")
                .annotation(
                    AnnotationKind::Context
                        .span(151..152)
                        .label("unclosed delimiter"),
                )
                .annotation(
                    AnnotationKind::Context
                        .span(159..160)
                        .label("unclosed delimiter"),
                )
                .annotation(
                    AnnotationKind::Context
                        .span(164..164)
                        .label("missing open `(` for this delimiter"),
                )
                .annotation(AnnotationKind::Primary.span(167..167)),
        )];
    let expected_ascii = str![[r#"
error: this file contains an unclosed delimiter
 --> $DIR/issue-91334.rs:7:23
  |
7 | fn f(){||yield(((){),
  |       -       -    - ^
  |       |       |    |
  |       |       |    missing open `(` for this delimiter
  |       |       unclosed delimiter
  |       unclosed delimiter
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: this file contains an unclosed delimiter
  ╭▸ $DIR/issue-91334.rs:7:23
  │
7 │ fn f(){||yield(((){),
  │       ┬       ┬    ┬ ━
  │       │       │    │
  │       │       │    missing open `(` for this delimiter
  │       │       unclosed delimiter
  ╰╴      unclosed delimiter
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn issue_114529_illegal_break_with_value() {
    // tests/ui/typeck/issue-114529-illegal-break-with-value.rs
    let source = r#"// Regression test for issue #114529
// Tests that we do not ICE during const eval for a
// break-with-value in contexts where it is illegal

#[allow(while_true)]
fn main() {
    [(); {
        while true {
            break 9; //~ ERROR `break` with value from a `while` loop
        };
        51
    }];

    [(); {
        while let Some(v) = Some(9) {
            break v; //~ ERROR `break` with value from a `while` loop
        };
        51
    }];

    while true {
        break (|| { //~ ERROR `break` with value from a `while` loop
            let local = 9;
        });
    }
}
"#;
    let input = &[
        Level::ERROR
            .primary_title("`break` with value from a `while` loop")
            .id("E0571")
            .element(
                Snippet::source(source)
                    .line_start(1)
                    .path("$DIR/issue-114529-illegal-break-with-value.rs")
                    .annotation(
                        AnnotationKind::Primary
                            .span(483..581)
                            .label("can only break with a value inside `loop` or breakable block"),
                    )
                    .annotation(
                        AnnotationKind::Context
                            .span(462..472)
                            .label("you can't `break` with a value in a `while` loop"),
                    ),
            ),
        Level::HELP
            .secondary_title("use `break` on its own without a value inside this `while` loop")
            .element(
                Snippet::source(source)
                    .line_start(1)
                    .path("$DIR/issue-114529-illegal-break-with-value.rs")
                    .annotation(AnnotationKind::Context.span(483..581).label("break")),
            ),
    ];
    let expected_ascii = str![[r#"
error[E0571]: `break` with value from a `while` loop
  --> $DIR/issue-114529-illegal-break-with-value.rs:22:9
   |
21 |       while true {
   |       ---------- you can't `break` with a value in a `while` loop
22 | /         break (|| { //~ ERROR `break` with value from a `while` loop
23 | |             let local = 9;
24 | |         });
   | |__________^ can only break with a value inside `loop` or breakable block
   |
help: use `break` on its own without a value inside this `while` loop
  --> $DIR/issue-114529-illegal-break-with-value.rs:22:9
   |
22 | /         break (|| { //~ ERROR `break` with value from a `while` loop
23 | |             let local = 9;
24 | |         });
   | |__________- break
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0571]: `break` with value from a `while` loop
   ╭▸ $DIR/issue-114529-illegal-break-with-value.rs:22:9
   │
21 │       while true {
   │       ────────── you can't `break` with a value in a `while` loop
22 │ ┏         break (|| { //~ ERROR `break` with value from a `while` loop
23 │ ┃             let local = 9;
24 │ ┃         });
   │ ┗━━━━━━━━━━┛ can only break with a value inside `loop` or breakable block
   ╰╴
help: use `break` on its own without a value inside this `while` loop
   ╭▸ $DIR/issue-114529-illegal-break-with-value.rs:22:9
   │
22 │ ┌         break (|| { //~ ERROR `break` with value from a `while` loop
23 │ │             let local = 9;
24 │ │         });
   ╰╴└──────────┘ break
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn primitive_reprs_should_have_correct_length() {
    // tests/ui/transmutability/enums/repr/primitive_reprs_should_have_correct_length.rs
    let source = r#"//! An enum with a primitive repr should have exactly the size of that primitive.

#![crate_type = "lib"]
#![feature(transmutability)]
#![allow(dead_code)]

mod assert {
    use std::mem::{Assume, TransmuteFrom};

    pub fn is_transmutable<Src, Dst>()
    where
        Dst: TransmuteFrom<Src, {
            Assume {
                alignment: true,
                lifetimes: true,
                safety: true,
                validity: true,
            }
        }>
    {}
}

#[repr(C)]
struct Zst;

#[derive(Clone, Copy)]
#[repr(i8)] enum V0i8 { V }
#[repr(u8)] enum V0u8 { V }
#[repr(i16)] enum V0i16 { V }
#[repr(u16)] enum V0u16 { V }
#[repr(i32)] enum V0i32 { V }
#[repr(u32)] enum V0u32 { V }
#[repr(i64)] enum V0i64 { V }
#[repr(u64)] enum V0u64 { V }
#[repr(isize)] enum V0isize { V }
#[repr(usize)] enum V0usize { V }

fn n8() {
    type Smaller = Zst;
    type Analog = u8;
    type Larger = u16;

    fn i_should_have_correct_length() {
        type Current = V0i8;

        assert::is_transmutable::<Smaller, Current>(); //~ ERROR cannot be safely transmuted
        assert::is_transmutable::<Current, Analog>();
        assert::is_transmutable::<Current, Larger>(); //~ ERROR cannot be safely transmuted
    }

    fn u_should_have_correct_length() {
        type Current = V0u8;

        assert::is_transmutable::<Smaller, Current>(); //~ ERROR cannot be safely transmuted
        assert::is_transmutable::<Current, Analog>();
        assert::is_transmutable::<Current, Larger>(); //~ ERROR cannot be safely transmuted
    }
}

fn n16() {
    type Smaller = u8;
    type Analog = u16;
    type Larger = u32;

    fn i_should_have_correct_length() {
        type Current = V0i16;

        assert::is_transmutable::<Smaller, Current>(); //~ ERROR cannot be safely transmuted
        assert::is_transmutable::<Current, Analog>();
        assert::is_transmutable::<Current, Larger>(); //~ ERROR cannot be safely transmuted
    }

    fn u_should_have_correct_length() {
        type Current = V0u16;

        assert::is_transmutable::<Smaller, Current>(); //~ ERROR cannot be safely transmuted
        assert::is_transmutable::<Current, Analog>();
        assert::is_transmutable::<Current, Larger>(); //~ ERROR cannot be safely transmuted
    }
}

fn n32() {
    type Smaller = u16;
    type Analog = u32;
    type Larger = u64;

    fn i_should_have_correct_length() {
        type Current = V0i32;

        assert::is_transmutable::<Smaller, Current>(); //~ ERROR cannot be safely transmuted
        assert::is_transmutable::<Current, Analog>();
        assert::is_transmutable::<Current, Larger>(); //~ ERROR cannot be safely transmuted
    }

    fn u_should_have_correct_length() {
        type Current = V0u32;

        assert::is_transmutable::<Smaller, Current>(); //~ ERROR cannot be safely transmuted
        assert::is_transmutable::<Current, Analog>();
        assert::is_transmutable::<Current, Larger>(); //~ ERROR cannot be safely transmuted
    }
}

fn n64() {
    type Smaller = u32;
    type Analog = u64;
    type Larger = u128;

    fn i_should_have_correct_length() {
        type Current = V0i64;

        assert::is_transmutable::<Smaller, Current>(); //~ ERROR cannot be safely transmuted
        assert::is_transmutable::<Current, Analog>();
        assert::is_transmutable::<Current, Larger>(); //~ ERROR cannot be safely transmuted
    }

    fn u_should_have_correct_length() {
        type Current = V0u64;

        assert::is_transmutable::<Smaller, Current>(); //~ ERROR cannot be safely transmuted
        assert::is_transmutable::<Current, Analog>();
        assert::is_transmutable::<Current, Larger>(); //~ ERROR cannot be safely transmuted
    }
}

fn nsize() {
    type Smaller = u8;
    type Analog = usize;
    type Larger = [usize; 2];

    fn i_should_have_correct_length() {
        type Current = V0isize;

        assert::is_transmutable::<Smaller, Current>(); //~ ERROR cannot be safely transmuted
        assert::is_transmutable::<Current, Analog>();
        assert::is_transmutable::<Current, Larger>(); //~ ERROR cannot be safely transmuted
    }

    fn u_should_have_correct_length() {
        type Current = V0usize;

        assert::is_transmutable::<Smaller, Current>(); //~ ERROR cannot be safely transmuted
        assert::is_transmutable::<Current, Analog>();
        assert::is_transmutable::<Current, Larger>(); //~ ERROR cannot be safely transmuted
    }
}
"#;
    let input =
        &[
            Level::ERROR
                .primary_title("`V0usize` cannot be safely transmuted into `[usize; 2]`")
                .id("E0277")
                .element(
                    Snippet::source(source)
                        .line_start(1)
                        .path("$DIR/primitive_reprs_should_have_correct_length.rs")
                        .annotation(AnnotationKind::Primary.span(4375..4381).label(
                            "the size of `V0usize` is smaller than the size of `[usize; 2]`",
                        )),
                ),
            Level::NOTE
                .secondary_title("required by a bound in `is_transmutable`")
                .element(
                    Snippet::source(source)
                        .line_start(1)
                        .path("$DIR/primitive_reprs_should_have_correct_length.rs")
                        .annotation(
                            AnnotationKind::Context
                                .span(225..240)
                                .label("required by a bound in this function"),
                        )
                        .annotation(
                            AnnotationKind::Primary
                                .span(276..470)
                                .label("required by this bound in `is_transmutable`"),
                        ),
                ),
        ];
    let expected_ascii = str![[r#"
error[E0277]: `V0usize` cannot be safely transmuted into `[usize; 2]`
   --> $DIR/primitive_reprs_should_have_correct_length.rs:144:44
    |
144 |         assert::is_transmutable::<Current, Larger>(); //~ ERROR cannot be safely transmuted
    |                                            ^^^^^^ the size of `V0usize` is smaller than the size of `[usize; 2]`
    |
note: required by a bound in `is_transmutable`
   --> $DIR/primitive_reprs_should_have_correct_length.rs:12:14
    |
 10 |       pub fn is_transmutable<Src, Dst>()
    |              --------------- required by a bound in this function
 11 |       where
 12 |           Dst: TransmuteFrom<Src, {
    |  ______________^
 13 | |             Assume {
 14 | |                 alignment: true,
 15 | |                 lifetimes: true,
...   |
 19 | |         }>
    | |__________^ required by this bound in `is_transmutable`
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0277]: `V0usize` cannot be safely transmuted into `[usize; 2]`
    ╭▸ $DIR/primitive_reprs_should_have_correct_length.rs:144:44
    │
144 │         assert::is_transmutable::<Current, Larger>(); //~ ERROR cannot be safely transmuted
    │                                            ━━━━━━ the size of `V0usize` is smaller than the size of `[usize; 2]`
    ╰╴
note: required by a bound in `is_transmutable`
    ╭▸ $DIR/primitive_reprs_should_have_correct_length.rs:12:14
    │
 10 │       pub fn is_transmutable<Src, Dst>()
    │              ─────────────── required by a bound in this function
 11 │       where
 12 │           Dst: TransmuteFrom<Src, {
    │ ┏━━━━━━━━━━━━━━┛
 13 │ ┃             Assume {
 14 │ ┃                 alignment: true,
 15 │ ┃                 lifetimes: true,
    ┆ ┇
 19 │ ┃         }>
    ╰╴┗━━━━━━━━━━┛ required by this bound in `is_transmutable`
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn align_fail() {
    // tests/ui/transmutability/alignment/align-fail.rs
    let source = r#"//@ check-fail
#![feature(transmutability)]

mod assert {
    use std::mem::{Assume, TransmuteFrom};

    pub fn is_maybe_transmutable<Src, Dst>()
    where
        Dst: TransmuteFrom<Src, {
            Assume {
                alignment: false,
                lifetimes: true,
                safety: true,
                validity: true,
            }
        }>
    {}
}

fn main() {
    assert::is_maybe_transmutable::<&'static [u8; 0], &'static [u16; 0]>(); //~ ERROR `&[u8; 0]` cannot be safely transmuted into `&[u16; 0]`
}
"#;
    let input = &[Level::ERROR
        .primary_title("`&[u8; 0]` cannot be safely transmuted into `&[u16; 0]`")
        .id("E027s7").element(
                Snippet::source(source)
                    .line_start(1)

                    .path("$DIR/align-fail.rs")
                    .annotation(
                        AnnotationKind::Primary
                            .span(442..459)
                            .label("the minimum alignment of `&[u8; 0]` (1) should be greater than that of `&[u16; 0]` (2)")
                    ),
            )];
    let expected_ascii = str![[r#"
error[E027s7]: `&[u8; 0]` cannot be safely transmuted into `&[u16; 0]`
  --> $DIR/align-fail.rs:21:55
   |
21 | ...ic [u8; 0], &'static [u16; 0]>(); //~ ERROR `&[u8; 0]` cannot be safely transmuted into `&[u16; 0]`
   |                ^^^^^^^^^^^^^^^^^ the minimum alignment of `&[u8; 0]` (1) should be greater than that of `&[u16; 0]` (2)
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E027s7]: `&[u8; 0]` cannot be safely transmuted into `&[u16; 0]`
   ╭▸ $DIR/align-fail.rs:21:55
   │
21 │ …atic [u8; 0], &'static [u16; 0]>(); //~ ERROR `&[u8; 0]` cannot be safely transmuted into `&[u16; 0]`
   ╰╴               ━━━━━━━━━━━━━━━━━ the minimum alignment of `&[u8; 0]` (1) should be greater than that of `&[u16; 0]` (2)
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn missing_semicolon() {
    // tests/ui/suggestions/missing-semicolon.rs
    let source = r#"//@ run-rustfix
#![allow(dead_code, unused_variables, path_statements)]
fn a() {
    let x = 5;
    let y = x //~ ERROR expected function
    () //~ ERROR expected `;`, found `}`
}

fn b() {
    let x = 5;
    let y = x //~ ERROR expected function
    ();
}
fn c() {
    let x = 5;
    x //~ ERROR expected function
    ()
}
fn d() { // ok
    let x = || ();
    x
    ()
}
fn e() { // ok
    let x = || ();
    x
    ();
}
fn f()
 {
    let y = 5 //~ ERROR expected function
    () //~ ERROR expected `;`, found `}`
}
fn g() {
    5 //~ ERROR expected function
    ();
}
fn main() {}
"#;
    let input =
        &[Level::ERROR
            .primary_title("expected function, found `{integer}`")
            .id("E0618")
            .element(
                Snippet::source(source)
                    .line_start(1)
                    .path("$DIR/missing-semicolon.rs")
                    .annotation(
                        AnnotationKind::Context
                            .span(108..144)
                            .label("call expression requires function"),
                    )
                    .annotation(
                        AnnotationKind::Context
                            .span(89..90)
                            .label("`x` has type `{integer}`"),
                    )
                    .annotation(AnnotationKind::Context.span(109..109).label(
                        "help: consider using a semicolon here to finish the statement: `;`",
                    ))
                    .annotation(AnnotationKind::Primary.span(108..109)),
            )];
    let expected_ascii = str![[r#"
error[E0618]: expected function, found `{integer}`
 --> $DIR/missing-semicolon.rs:5:13
  |
4 |       let x = 5;
  |           - `x` has type `{integer}`
5 |       let y = x //~ ERROR expected function
  |               ^- help: consider using a semicolon here to finish the statement: `;`
  |  _____________|
  | |
6 | |     () //~ ERROR expected `;`, found `}`
  | |______- call expression requires function
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0618]: expected function, found `{integer}`
  ╭▸ $DIR/missing-semicolon.rs:5:13
  │
4 │       let x = 5;
  │           ─ `x` has type `{integer}`
5 │       let y = x //~ ERROR expected function
  │               ━─ help: consider using a semicolon here to finish the statement: `;`
  │ ┌─────────────┘
  │ │
6 │ │     () //~ ERROR expected `;`, found `}`
  ╰╴└──────┘ call expression requires function
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn nested_macro_rules() {
    // tests/ui/proc-macro/nested-macro-rules.rs
    let source = r#"//@ run-pass
//@ aux-build:nested-macro-rules.rs
//@ proc-macro: test-macros.rs
//@ compile-flags: -Z span-debug -Z macro-backtrace
//@ edition:2018

#![no_std] // Don't load unnecessary hygiene information from std
#![warn(non_local_definitions)]

extern crate std;

extern crate nested_macro_rules;
extern crate test_macros;

use test_macros::{print_bang, print_attr};

use nested_macro_rules::FirstStruct;
struct SecondStruct;

fn main() {
    nested_macro_rules::inner_macro!(print_bang, print_attr);

    nested_macro_rules::outer_macro!(SecondStruct, SecondAttrStruct);
    //~^ WARN non-local `macro_rules!` definition
    inner_macro!(print_bang, print_attr);
}
"#;

    let aux_source = r#"pub struct FirstStruct;

#[macro_export]
macro_rules! outer_macro {
    ($name:ident, $attr_struct_name:ident) => {
        #[macro_export]
        macro_rules! inner_macro {
            ($bang_macro:ident, $attr_macro:ident) => {
                $bang_macro!($name);
                #[$attr_macro] struct $attr_struct_name {}
            }
        }
    }
}

outer_macro!(FirstStruct, FirstAttrStruct);
"#;
    let input =
           &[ Level::WARNING
                .primary_title("non-local `macro_rules!` definition, `#[macro_export]` macro should be written at top level module")
                .element(
                    Snippet::source(aux_source)
                        .line_start(1)
                        .path("$DIR/auxiliary/nested-macro-rules.rs")

                        .annotation(
                            AnnotationKind::Context
                                .span(41..65)
                                .label("in this expansion of `nested_macro_rules::outer_macro!`"),
                        )
                        .annotation(AnnotationKind::Primary.span(148..350)),
                )
                .element(
                    Snippet::source(source)
                        .line_start(1)
                        .path("$DIR/nested-macro-rules.rs")

                        .annotation(
                            AnnotationKind::Context
                                .span(510..574)
                                .label("in this macro invocation"),
                        ),
                )
                .element(
                    Level::HELP
                        .message("remove the `#[macro_export]` or move this `macro_rules!` outside the of the current function `main`")
                )
                .element(
                    Level::NOTE
                        .message("a `macro_rules!` definition is non-local if it is nested inside an item and has a `#[macro_export]` attribute")
                ),
            Level::NOTE.secondary_title("the lint level is defined here")
                .element(
                    Snippet::source(source)
                        .line_start(1)
                        .path("$DIR/nested-macro-rules.rs")

                        .annotation(AnnotationKind::Primary.span(224..245)),
                )];
    let expected_ascii = str![[r#"
warning: non-local `macro_rules!` definition, `#[macro_export]` macro should be written at top level module
  --> $DIR/auxiliary/nested-macro-rules.rs:7:9
   |
 4 |   macro_rules! outer_macro {
   |   ------------------------ in this expansion of `nested_macro_rules::outer_macro!`
...
 7 | /         macro_rules! inner_macro {
 8 | |             ($bang_macro:ident, $attr_macro:ident) => {
 9 | |                 $bang_macro!($name);
10 | |                 #[$attr_macro] struct $attr_struct_name {}
11 | |             }
12 | |         }
   | |_________^
   |
  ::: $DIR/nested-macro-rules.rs:23:5
   |
23 |       nested_macro_rules::outer_macro!(SecondStruct, SecondAttrStruct);
   |       ---------------------------------------------------------------- in this macro invocation
   |
   = help: remove the `#[macro_export]` or move this `macro_rules!` outside the of the current function `main`
   = note: a `macro_rules!` definition is non-local if it is nested inside an item and has a `#[macro_export]` attribute
note: the lint level is defined here
  --> $DIR/nested-macro-rules.rs:8:9
   |
 8 | #![warn(non_local_definitions)]
   |         ^^^^^^^^^^^^^^^^^^^^^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
warning: non-local `macro_rules!` definition, `#[macro_export]` macro should be written at top level module
   ╭▸ $DIR/auxiliary/nested-macro-rules.rs:7:9
   │
 4 │   macro_rules! outer_macro {
   │   ──────────────────────── in this expansion of `nested_macro_rules::outer_macro!`
   ┆
 7 │ ┏         macro_rules! inner_macro {
 8 │ ┃             ($bang_macro:ident, $attr_macro:ident) => {
 9 │ ┃                 $bang_macro!($name);
10 │ ┃                 #[$attr_macro] struct $attr_struct_name {}
11 │ ┃             }
12 │ ┃         }
   │ ┗━━━━━━━━━┛
   │
   ⸬  $DIR/nested-macro-rules.rs:23:5
   │
23 │       nested_macro_rules::outer_macro!(SecondStruct, SecondAttrStruct);
   │       ──────────────────────────────────────────────────────────────── in this macro invocation
   │
   ├ help: remove the `#[macro_export]` or move this `macro_rules!` outside the of the current function `main`
   ╰ note: a `macro_rules!` definition is non-local if it is nested inside an item and has a `#[macro_export]` attribute
note: the lint level is defined here
   ╭▸ $DIR/nested-macro-rules.rs:8:9
   │
 8 │ #![warn(non_local_definitions)]
   ╰╴        ━━━━━━━━━━━━━━━━━━━━━
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn method_on_ambiguous_numeric_type() {
    // tests/ui/methods/method-on-ambiguous-numeric-type.rs
    let source = r#"//@ aux-build:macro-in-other-crate.rs

#[macro_use] extern crate macro_in_other_crate;

macro_rules! local_mac {
    ($ident:ident) => { let $ident = 42; }
}
macro_rules! local_mac_tt {
    ($tt:tt) => { let $tt = 42; }
}

fn main() {
    let x = 2.0.neg();
    //~^ ERROR can't call method `neg` on ambiguous numeric type `{float}`

    let y = 2.0;
    let x = y.neg();
    //~^ ERROR can't call method `neg` on ambiguous numeric type `{float}`
    println!("{:?}", x);

    for i in 0..100 {
        println!("{}", i.pow(2));
        //~^ ERROR can't call method `pow` on ambiguous numeric type `{integer}`
    }

    local_mac!(local_bar);
    local_bar.pow(2);
    //~^ ERROR can't call method `pow` on ambiguous numeric type `{integer}`

    local_mac_tt!(local_bar_tt);
    local_bar_tt.pow(2);
    //~^ ERROR can't call method `pow` on ambiguous numeric type `{integer}`
}

fn qux() {
    mac!(bar);
    bar.pow(2);
    //~^ ERROR can't call method `pow` on ambiguous numeric type `{integer}`
}
"#;

    let aux_source = r#"#[macro_export]
macro_rules! mac {
    ($ident:ident) => { let $ident = 42; }
}

#[macro_export]
macro_rules! inline {
    () => ()
}
"#;
    let input = &[
        Level::ERROR
            .primary_title("can't call method `pow` on ambiguous numeric type `{integer}`")
            .id("E0689")
            .element(
                Snippet::source(source)
                    .line_start(1)
                    .path("$DIR/method-on-ambiguous-numeric-type.rs")
                    .annotation(AnnotationKind::Primary.span(916..919)),
            ),
        Level::HELP
            .secondary_title("you must specify a type for this binding, like `i32`")
            .element(
                Snippet::source(aux_source)
                    .line_start(1)
                    .path("$DIR/auxiliary/macro-in-other-crate.rs")
                    .annotation(AnnotationKind::Context.span(69..69).label(": i32")),
            ),
    ];
    let expected_ascii = str![[r#"
error[E0689]: can't call method `pow` on ambiguous numeric type `{integer}`
  --> $DIR/method-on-ambiguous-numeric-type.rs:37:9
   |
37 |     bar.pow(2);
   |         ^^^
   |
help: you must specify a type for this binding, like `i32`
  --> $DIR/auxiliary/macro-in-other-crate.rs:3:35
   |
 3 |     ($ident:ident) => { let $ident = 42; }
   |                                   - : i32
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0689]: can't call method `pow` on ambiguous numeric type `{integer}`
   ╭▸ $DIR/method-on-ambiguous-numeric-type.rs:37:9
   │
37 │     bar.pow(2);
   │         ━━━
   ╰╴
help: you must specify a type for this binding, like `i32`
   ╭▸ $DIR/auxiliary/macro-in-other-crate.rs:3:35
   │
 3 │     ($ident:ident) => { let $ident = 42; }
   ╰╴                                  ─ : i32
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn issue_42234_unknown_receiver_type() {
    // tests/ui/span/issue-42234-unknown-receiver-type.rs
    let source = r#"//@ revisions: full generic_arg
#![cfg_attr(generic_arg, feature(generic_arg_infer))]

// When the type of a method call's receiver is unknown, the span should point
// to the receiver (and not the entire call, as was previously the case before
// the fix of which this tests).

fn shines_a_beacon_through_the_darkness() {
    let x: Option<_> = None; //~ ERROR type annotations needed
    x.unwrap().method_that_could_exist_on_some_type();
}

fn courier_to_des_moines_and_points_west(data: &[u32]) -> String {
    data.iter()
        .sum::<_>() //~ ERROR type annotations needed
        .to_string()
}

fn main() {}
"#;

    let input = &[Level::ERROR
        .primary_title("type annotations needed")
        .id("E0282")
        .element(
            Snippet::source(source)
                .line_start(1)
                .path("$DIR/issue-42234-unknown-receiver-type.rs")
                .annotation(AnnotationKind::Primary.span(536..539).label(
                    "cannot infer type of the type parameter `S` declared on the method `sum`",
                )),
        )];
    let expected_ascii = str![[r#"
error[E0282]: type annotations needed
  --> $DIR/issue-42234-unknown-receiver-type.rs:15:10
   |
15 |         .sum::<_>() //~ ERROR type annotations needed
   |          ^^^ cannot infer type of the type parameter `S` declared on the method `sum`
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0282]: type annotations needed
   ╭▸ $DIR/issue-42234-unknown-receiver-type.rs:15:10
   │
15 │         .sum::<_>() //~ ERROR type annotations needed
   ╰╴         ━━━ cannot infer type of the type parameter `S` declared on the method `sum`
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn pattern_usefulness_empty_match() {
    // tests/ui/pattern/usefulness/empty-match.rs
    let source = r##"//@ revisions: normal exhaustive_patterns
//
// This tests a match with no arms on various types.
#![feature(never_type)]
#![cfg_attr(exhaustive_patterns, feature(exhaustive_patterns))]
#![deny(unreachable_patterns)]

fn nonempty<const N: usize>(arrayN_of_empty: [!; N]) {
    macro_rules! match_no_arms {
        ($e:expr) => {
            match $e {}
        };
    }
    macro_rules! match_guarded_arm {
        ($e:expr) => {
            match $e {
                _ if false => {}
            }
        };
    }

    struct NonEmptyStruct1;
    struct NonEmptyStruct2(bool);
    union NonEmptyUnion1 {
        foo: (),
    }
    union NonEmptyUnion2 {
        foo: (),
        bar: !,
    }
    enum NonEmptyEnum1 {
        Foo(bool),
    }
    enum NonEmptyEnum2 {
        Foo(bool),
        Bar,
    }
    enum NonEmptyEnum5 {
        V1,
        V2,
        V3,
        V4,
        V5,
    }
    let array0_of_empty: [!; 0] = [];

    match_no_arms!(0u8); //~ ERROR type `u8` is non-empty
    match_no_arms!(0i8); //~ ERROR type `i8` is non-empty
    match_no_arms!(0usize); //~ ERROR type `usize` is non-empty
    match_no_arms!(0isize); //~ ERROR type `isize` is non-empty
    match_no_arms!(NonEmptyStruct1); //~ ERROR type `NonEmptyStruct1` is non-empty
    match_no_arms!(NonEmptyStruct2(true)); //~ ERROR type `NonEmptyStruct2` is non-empty
    match_no_arms!((NonEmptyUnion1 { foo: () })); //~ ERROR type `NonEmptyUnion1` is non-empty
    match_no_arms!((NonEmptyUnion2 { foo: () })); //~ ERROR type `NonEmptyUnion2` is non-empty
    match_no_arms!(NonEmptyEnum1::Foo(true)); //~ ERROR `NonEmptyEnum1::Foo(_)` not covered
    match_no_arms!(NonEmptyEnum2::Foo(true)); //~ ERROR `NonEmptyEnum2::Foo(_)` and `NonEmptyEnum2::Bar` not covered
    match_no_arms!(NonEmptyEnum5::V1); //~ ERROR `NonEmptyEnum5::V1`, `NonEmptyEnum5::V2`, `NonEmptyEnum5::V3` and 2 more not covered
    match_no_arms!(array0_of_empty); //~ ERROR type `[!; 0]` is non-empty
    match_no_arms!(arrayN_of_empty); //~ ERROR type `[!; N]` is non-empty

    match_guarded_arm!(0u8); //~ ERROR `0_u8..=u8::MAX` not covered
    match_guarded_arm!(0i8); //~ ERROR `i8::MIN..=i8::MAX` not covered
    match_guarded_arm!(0usize); //~ ERROR `0_usize..` not covered
    match_guarded_arm!(0isize); //~ ERROR `_` not covered
    match_guarded_arm!(NonEmptyStruct1); //~ ERROR `NonEmptyStruct1` not covered
    match_guarded_arm!(NonEmptyStruct2(true)); //~ ERROR `NonEmptyStruct2(_)` not covered
    match_guarded_arm!((NonEmptyUnion1 { foo: () })); //~ ERROR `NonEmptyUnion1 { .. }` not covered
    match_guarded_arm!((NonEmptyUnion2 { foo: () })); //~ ERROR `NonEmptyUnion2 { .. }` not covered
    match_guarded_arm!(NonEmptyEnum1::Foo(true)); //~ ERROR `NonEmptyEnum1::Foo(_)` not covered
    match_guarded_arm!(NonEmptyEnum2::Foo(true)); //~ ERROR `NonEmptyEnum2::Foo(_)` and `NonEmptyEnum2::Bar` not covered
    match_guarded_arm!(NonEmptyEnum5::V1); //~ ERROR `NonEmptyEnum5::V1`, `NonEmptyEnum5::V2`, `NonEmptyEnum5::V3` and 2 more not covered
    match_guarded_arm!(array0_of_empty); //~ ERROR `[]` not covered
    match_guarded_arm!(arrayN_of_empty); //~ ERROR `[]` not covered
}

fn main() {}
"##;

    let input =
           &[Level::ERROR
                .primary_title(
                    "non-exhaustive patterns: `NonEmptyEnum5::V1`, `NonEmptyEnum5::V2`, `NonEmptyEnum5::V3` and 2 more not covered"
                )
                .id("E0004")
                .element(
                    Snippet::source(source)
                        .line_start(1)
                        .path("$DIR/empty-match.rs")

                        .annotation(
                            AnnotationKind::Primary
                                .span(2911..2928)
                                .label("patterns `NonEmptyEnum5::V1`, `NonEmptyEnum5::V2`, `NonEmptyEnum5::V3` and 2 more not covered")
                        ),
                ),
            Level::NOTE.secondary_title("`NonEmptyEnum5` defined here")
                .element(
                    Snippet::source(source)
                        .line_start(1)
                        .path("$DIR/empty-match.rs")

                        .annotation(AnnotationKind::Primary.span(818..831))
                        .annotation(AnnotationKind::Context.span(842..844).label("not covered"))
                        .annotation(AnnotationKind::Context.span(854..856).label("not covered"))
                        .annotation(AnnotationKind::Context.span(866..868).label("not covered"))
                        .annotation(AnnotationKind::Context.span(878..880).label("not covered"))
                        .annotation(AnnotationKind::Context.span(890..892).label("not covered"))
                )
                .element(Level::NOTE.message("the matched value is of type `NonEmptyEnum5`"))
                .element(Level::NOTE.message("match arms with guards don't count towards exhaustivity")
            ),
            Level::HELP
                .secondary_title("ensure that all possible cases are being handled by adding a match arm with a wildcard pattern as shown, or multiple match arms")
                .element(
                    Snippet::source(source)
                        .line_start(1)
                        .path("$DIR/empty-match.rs")
                        .annotation(AnnotationKind::Context.span(485..485).label(",\n                _ => todo!()"))
            )
    ];
    let expected_ascii = str![[r#"
error[E0004]: non-exhaustive patterns: `NonEmptyEnum5::V1`, `NonEmptyEnum5::V2`, `NonEmptyEnum5::V3` and 2 more not covered
  --> $DIR/empty-match.rs:71:24
   |
71 |     match_guarded_arm!(NonEmptyEnum5::V1); //~ ERROR `NonEmptyEnum5::V1`, `NonEmptyEnum5::V2`, `NonEmptyEnum5::V3` and 2 more not covered
   |                        ^^^^^^^^^^^^^^^^^ patterns `NonEmptyEnum5::V1`, `NonEmptyEnum5::V2`, `NonEmptyEnum5::V3` and 2 more not covered
   |
note: `NonEmptyEnum5` defined here
  --> $DIR/empty-match.rs:38:10
   |
38 |     enum NonEmptyEnum5 {
   |          ^^^^^^^^^^^^^
39 |         V1,
   |         -- not covered
40 |         V2,
   |         -- not covered
41 |         V3,
   |         -- not covered
42 |         V4,
   |         -- not covered
43 |         V5,
   |         -- not covered
   = note: the matched value is of type `NonEmptyEnum5`
   = note: match arms with guards don't count towards exhaustivity
help: ensure that all possible cases are being handled by adding a match arm with a wildcard pattern as shown, or multiple match arms
  --> $DIR/empty-match.rs:17:33
   |
17 |                 _ if false => {}
   |                                 - ,
                _ => todo!()
"#]];
    let renderer =
        Renderer::plain().term_width(annotate_snippets::renderer::DEFAULT_TERM_WIDTH + 4);
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0004]: non-exhaustive patterns: `NonEmptyEnum5::V1`, `NonEmptyEnum5::V2`, `NonEmptyEnum5::V3` and 2 more not covered
   ╭▸ $DIR/empty-match.rs:71:24
   │
71 │     match_guarded_arm!(NonEmptyEnum5::V1); //~ ERROR `NonEmptyEnum5::V1`, `NonEmptyEnum5::V2`, `NonEmptyEnum5::V3` and 2 more not covered
   │                        ━━━━━━━━━━━━━━━━━ patterns `NonEmptyEnum5::V1`, `NonEmptyEnum5::V2`, `NonEmptyEnum5::V3` and 2 more not covered
   ╰╴
note: `NonEmptyEnum5` defined here
   ╭▸ $DIR/empty-match.rs:38:10
   │
38 │     enum NonEmptyEnum5 {
   │          ━━━━━━━━━━━━━
39 │         V1,
   │         ── not covered
40 │         V2,
   │         ── not covered
41 │         V3,
   │         ── not covered
42 │         V4,
   │         ── not covered
43 │         V5,
   │         ── not covered
   ├ note: the matched value is of type `NonEmptyEnum5`
   ╰ note: match arms with guards don't count towards exhaustivity
help: ensure that all possible cases are being handled by adding a match arm with a wildcard pattern as shown, or multiple match arms
   ╭▸ $DIR/empty-match.rs:17:33
   │
17 │                 _ if false => {}
   ╰╴                                ─ ,
                _ => todo!()
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn object_fail() {
    // tests/ui/traits/alias/object-fail.rs
    let source = r#"#![feature(trait_alias)]

trait EqAlias = Eq;
trait IteratorAlias = Iterator;

fn main() {
    let _: &dyn EqAlias = &123;
    //~^ ERROR the trait alias `EqAlias` is not dyn compatible [E0038]
    let _: &dyn IteratorAlias = &vec![123].into_iter();
    //~^ ERROR must be specified
}
"#;
    let input = &[Level::ERROR
        .primary_title("the trait alias `EqAlias` is not dyn compatible")
        .id("E0038").element(
                Snippet::source(source)
                    .line_start(1)
                    .path("$DIR/object-fail.rs")

                    .annotation(
                        AnnotationKind::Primary
                            .span(107..114)
                            .label("`EqAlias` is not dyn compatible"),
                    ),
            ),
                    Level::NOTE
                        .secondary_title("for a trait to be dyn compatible it needs to allow building a vtable\nfor more information, visit <https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility>")
                .element(
                    Origin::path("$SRC_DIR/core/src/cmp.rs")
                        .line(334)
                        .char_column(14)
                )
                .element(Padding)
                .element(Level::NOTE.message("...because it uses `Self` as a type parameter"))
                .element(
                    Snippet::source(source)
                        .line_start(1)
                        .path("$DIR/object-fail.rs")

                        .annotation(
                            AnnotationKind::Context
                                .span(32..39)
                                .label("this trait is not dyn compatible..."),
                        ),
                )];
    let expected_ascii = str![[r#"
error[E0038]: the trait alias `EqAlias` is not dyn compatible
 --> $DIR/object-fail.rs:7:17
  |
7 |     let _: &dyn EqAlias = &123;
  |                 ^^^^^^^ `EqAlias` is not dyn compatible
  |
note: for a trait to be dyn compatible it needs to allow building a vtable
      for more information, visit <https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility>
 --> $SRC_DIR/core/src/cmp.rs:334:14
  |
  = note: ...because it uses `Self` as a type parameter
  |
 ::: $DIR/object-fail.rs:3:7
  |
3 | trait EqAlias = Eq;
  |       ------- this trait is not dyn compatible...
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0038]: the trait alias `EqAlias` is not dyn compatible
  ╭▸ $DIR/object-fail.rs:7:17
  │
7 │     let _: &dyn EqAlias = &123;
  │                 ━━━━━━━ `EqAlias` is not dyn compatible
  ╰╴
note: for a trait to be dyn compatible it needs to allow building a vtable
      for more information, visit <https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility>
  ╭▸ $SRC_DIR/core/src/cmp.rs:334:14
  │
  ├ note: ...because it uses `Self` as a type parameter
  │
  ⸬  $DIR/object-fail.rs:3:7
  │
3 │ trait EqAlias = Eq;
  ╰╴      ─────── this trait is not dyn compatible...
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn long_span_shortest() {
    // tests/ui/diagnostic-width/long-span.rs
    let source = r#"
const C: u8 = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

fn main() {}
"#;
    let input = &[Level::ERROR
        .primary_title("mismatched types")
        .id("E0038")
        .element(
            Snippet::source(source)
                .path("$DIR/long-span.rs")
                .annotation(
                    AnnotationKind::Primary
                        .span(15..5055)
                        .label("expected `u8`, found `[{integer}; 1680]`"),
                ),
        )];
    let expected_ascii = str![[r#"
error[E0038]: mismatched types
 --> $DIR/long-span.rs:2:15
  |
2 | ... = [0, 0...0, 0];
  |       ^^^^^...^^^^^ expected `u8`, found `[{integer}; 1680]`
"#]];

    let renderer = Renderer::plain().term_width(8);
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0038]: mismatched types
  ╭▸ $DIR/long-span.rs:2:15
  │
2 │ …u8 = [0, 0…0, 0];
  ╰╴      ━━━━━…━━━━━ expected `u8`, found `[{integer}; 1680]`
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn long_span_short() {
    // tests/ui/diagnostic-width/long-span.rs
    let source = r#"
const C: u8 = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

fn main() {}
"#;
    let input = &[Level::ERROR
        .primary_title("mismatched types")
        .id("E0038")
        .element(
            Snippet::source(source)
                .path("$DIR/long-span.rs")
                .annotation(
                    AnnotationKind::Primary
                        .span(15..5055)
                        .label("expected `u8`, found `[{integer}; 1680]`"),
                ),
        )];
    let expected_ascii = str![[r#"
error[E0038]: mismatched types
  ╭▸ $DIR/long-span.rs:2:15
  │
2 │ …u8 = [0, 0…0, 0];
  ╰╴      ━━━━━…━━━━━ expected `u8`, found `[{integer}; 1680]`
"#]];

    let renderer = Renderer::plain()
        .term_width(12)
        .decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0038]: mismatched types
  ╭▸ $DIR/long-span.rs:2:15
  │
2 │ …u8 = [0, 0…0, 0];
  ╰╴      ━━━━━…━━━━━ expected `u8`, found `[{integer}; 1680]`
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn long_span_long() {
    // tests/ui/diagnostic-width/long-span.rs
    let source = r#"
const C: u8 = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

fn main() {}
"#;
    let input = &[Level::ERROR
        .primary_title("mismatched types")
        .id("E0038")
        .element(
            Snippet::source(source)
                .path("$DIR/long-span.rs")
                .annotation(
                    AnnotationKind::Primary
                        .span(15..5055)
                        .label("expected `u8`, found `[{integer}; 1680]`"),
                ),
        )];
    let expected_ascii = str![[r#"
error[E0038]: mismatched types
  ╭▸ $DIR/long-span.rs:2:15
  │
2 │ …u8 = [0, 0, 0, 0, 0, 0, 0, 0, …, 0, 0, 0, 0, 0, 0, 0, 0];
  ╰╴      ━━━━━━━━━━━━━━━━━━━━━━━━━…━━━━━━━━━━━━━━━━━━━━━━━━━ expected `u8`, found `[{integer}; 1680]`
"#]];

    let renderer = Renderer::plain()
        .term_width(80)
        .decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0038]: mismatched types
  ╭▸ $DIR/long-span.rs:2:15
  │
2 │ …u8 = [0, 0, 0, 0, 0, 0, 0, 0, …, 0, 0, 0, 0, 0, 0, 0, 0];
  ╰╴      ━━━━━━━━━━━━━━━━━━━━━━━━━…━━━━━━━━━━━━━━━━━━━━━━━━━ expected `u8`, found `[{integer}; 1680]`
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn long_span_longest() {
    // tests/ui/diagnostic-width/long-span.rs
    let source = r#"
const C: u8 = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

fn main() {}
"#;
    let input = &[Level::ERROR
        .primary_title("mismatched types")
        .id("E0038")
        .element(
            Snippet::source(source)
                .path("$DIR/long-span.rs")
                .annotation(
                    AnnotationKind::Primary
                        .span(15..5055)
                        .label("expected `u8`, found `[{integer}; 1680]`"),
                ),
        )];
    let expected_ascii = str![[r#"
error[E0038]: mismatched types
 --> $DIR/long-span.rs:2:15
  |
2 | ... = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0...0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
  |       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^...^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ expected `u8`, found `[{integer}; 1680]`
"#]];

    let renderer = Renderer::plain().term_width(120);
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0038]: mismatched types
  ╭▸ $DIR/long-span.rs:2:15
  │
2 │ …u8 = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0…0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
  ╰╴      ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━…━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ expected `u8`, found `[{integer}; 1680]`
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn lint_map_unit_fn() {
    // tests/ui/lint/lint_map_unit_fn.rs
    let source = r#"#![deny(map_unit_fn)]

fn foo(items: &mut Vec<u8>) {
    items.sort();
}

fn main() {
    let mut x: Vec<Vec<u8>> = vec![vec![0, 2, 1], vec![5, 4, 3]];
    x.iter_mut().map(foo);
    //~^ ERROR `Iterator::map` call that discard the iterator's values
    x.iter_mut().map(|items| {
    //~^ ERROR `Iterator::map` call that discard the iterator's values
        items.sort();
    });
    let f = |items: &mut Vec<u8>| {
        items.sort();
    };
    x.iter_mut().map(f);
    //~^ ERROR `Iterator::map` call that discard the iterator's values
}
"#;

    let input = &[Level::ERROR
        .primary_title("`Iterator::map` call that discard the iterator's values")
                .element(
                    Snippet::source(source)
                        .path("$DIR/lint_map_unit_fn.rs")

                        .annotation(AnnotationKind::Context.span(271..278).label(
                            "this function returns `()`, which is likely not what you wanted",
                        ))
                        .annotation(
                            AnnotationKind::Context
                                .span(271..379)
                                .label("called `Iterator::map` with callable that returns `()`"),
                        )
                        .annotation(
                            AnnotationKind::Context
                                .span(267..380)
                                .label("after this call to map, the resulting iterator is `impl Iterator<Item = ()>`, which means the only information carried by the iterator is the number of items")
                        )
                        .annotation(AnnotationKind::Primary.span(267..380)),
                )
                .element(
                    Level::NOTE.message("`Iterator::map`, like many of the methods on `Iterator`, gets executed lazily, meaning that its effects won't be visible until it is iterated")),
            Level::HELP.secondary_title("you might have meant to use `Iterator::for_each`")
                .element(
                    Snippet::source(source)
                        .path("$DIR/lint_map_unit_fn.rs")

                        .patch(Patch::new(267..270, r#"for_each"#)),
                )];

    let expected_ascii = str![[r#"
error: `Iterator::map` call that discard the iterator's values
  --> $DIR/lint_map_unit_fn.rs:11:18
   |
11 |         x.iter_mut().map(|items| {
   |                      ^   -------
   |                      |   |
   |  ____________________|___this function returns `()`, which is likely not what you wanted
   | |  __________________|
   | | |
12 | | |     //~^ ERROR `Iterator::map` call that discard the iterator's values
13 | | |         items.sort();
14 | | |     });
   | | |     -^ after this call to map, the resulting iterator is `impl Iterator<Item = ()>`, which means the only information carried by the iterator is the number of items
   | | |_____||
   | |_______|
   |         called `Iterator::map` with callable that returns `()`
   |
   = note: `Iterator::map`, like many of the methods on `Iterator`, gets executed lazily, meaning that its effects won't be visible until it is iterated
help: you might have meant to use `Iterator::for_each`
   |
11 -     x.iter_mut().map(|items| {
11 +     x.iter_mut().for_each(|items| {
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: `Iterator::map` call that discard the iterator's values
   ╭▸ $DIR/lint_map_unit_fn.rs:11:18
   │
11 │         x.iter_mut().map(|items| {
   │                      ╿   │──────
   │                      │   │
   │ ┌────────────────────│───this function returns `()`, which is likely not what you wanted
   │ │ ┏━━━━━━━━━━━━━━━━━━┙
   │ │ ┃
12 │ │ ┃     //~^ ERROR `Iterator::map` call that discard the iterator's values
13 │ │ ┃         items.sort();
14 │ │ ┃     });
   │ │ ┃     │╿ after this call to map, the resulting iterator is `impl Iterator<Item = ()>`, which means the only information carried by the iterator is the number of items
   │ │ ┗━━━━━││
   │ └───────┤
   │         called `Iterator::map` with callable that returns `()`
   │
   ╰ note: `Iterator::map`, like many of the methods on `Iterator`, gets executed lazily, meaning that its effects won't be visible until it is iterated
help: you might have meant to use `Iterator::for_each`
   ╭╴
11 -     x.iter_mut().map(|items| {
11 +     x.iter_mut().for_each(|items| {
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn bad_char_literals() {
    // tests/ui/parser/bad-char-literals.rs

    let source = r#"// ignore-tidy-cr
// ignore-tidy-tab

fn main() {
    // these literals are just silly.
    ''';
    //~^ ERROR: character constant must be escaped: `'`

    // note that this is a literal "\n" byte
    '
';
    //~^^ ERROR: character constant must be escaped: `\n`

    // note that this is a literal "\r" byte
; //~ ERROR: character constant must be escaped: `\r`

    // note that this is a literal NULL
    '--'; //~ ERROR: character literal may only contain one codepoint

    // note that this is a literal tab character here
    '  ';
    //~^ ERROR: character constant must be escaped: `\t`
}
"#;

    let input = &[
        Level::ERROR
            .primary_title("character constant must be escaped: `\\n`")
            .element(
                Snippet::source(source)
                    .path("$DIR/bad-char-literals.rs")
                    .annotation(AnnotationKind::Primary.span(204..205)),
            ),
        Level::HELP.secondary_title("escape the character").element(
            Snippet::source(source)
                .path("$DIR/bad-char-literals.rs")
                .line_start(1)
                .patch(Patch::new(204..205, r#"\n"#)),
        ),
    ];
    let expected_ascii = str![[r#"
error: character constant must be escaped: `/n`
  --> $DIR/bad-char-literals.rs:10:6
   |
10 |     '
   |      ^
   |
help: escape the character
   |
10 |     '/n
   |      ++
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: character constant must be escaped: `/n`
   ╭▸ $DIR/bad-char-literals.rs:10:6
   │
10 │     '
   │      ━
   ╰╴
help: escape the character
   ╭╴
10 │     '/n
   ╰╴     ++
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn unclosed_1() {
    // tests/ui/frontmatter/unclosed-1.rs

    let source = r#"----cargo
//~^ ERROR: unclosed frontmatter

// This test checks that the #! characters can help us recover a frontmatter
// close. There should not be a "missing `main` function" error as the rest
// are properly parsed.

#![feature(frontmatter)]

fn main() {}
"#;

    let input = &[
        Level::ERROR.primary_title("unclosed frontmatter").element(
            Snippet::source(source)
                .path("$DIR/unclosed-1.rs")
                .annotation(AnnotationKind::Primary.span(0..221)),
        ),
        Level::NOTE
            .secondary_title("frontmatter opening here was not closed")
            .element(
                Snippet::source(source)
                    .path("$DIR/unclosed-1.rs")
                    .annotation(AnnotationKind::Primary.span(0..4)),
            ),
    ];
    let expected_ascii = str![[r#"
error: unclosed frontmatter
 --> $DIR/unclosed-1.rs:1:1
  |
1 | / ----cargo
... |
6 | | // are properly parsed.
  | |_______________________^
  |
note: frontmatter opening here was not closed
 --> $DIR/unclosed-1.rs:1:1
  |
1 | ----cargo
  | ^^^^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: unclosed frontmatter
  ╭▸ $DIR/unclosed-1.rs:1:1
  │
1 │ ┏ ----cargo
  ┆ ┇
6 │ ┃ // are properly parsed.
  │ ┗━━━━━━━━━━━━━━━━━━━━━━━┛
  ╰╴
note: frontmatter opening here was not closed
  ╭▸ $DIR/unclosed-1.rs:1:1
  │
1 │ ----cargo
  ╰╴━━━━
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn unclosed_2() {
    // tests/ui/frontmatter/unclosed-2.rs

    let source = r#"----cargo
//~^ ERROR: unclosed frontmatter
//~| ERROR: frontmatters are experimental

//@ compile-flags: --crate-type lib

// Leading whitespace on the feature line prevents recovery. However
// the dashes quoted will not be used for recovery and the entire file
// should be treated as within the frontmatter block.

 #![feature(frontmatter)]

fn foo() -> &str {
    "----"
}
"#;

    let input = &[
        Level::ERROR.primary_title("unclosed frontmatter").element(
            Snippet::source(source)
                .path("$DIR/unclosed-2.rs")
                .annotation(AnnotationKind::Primary.span(0..377)),
        ),
        Level::NOTE
            .secondary_title("frontmatter opening here was not closed")
            .element(
                Snippet::source(source)
                    .path("$DIR/unclosed-2.rs")
                    .annotation(AnnotationKind::Primary.span(0..4)),
            ),
    ];
    let expected_ascii = str![[r#"
error: unclosed frontmatter
  --> $DIR/unclosed-2.rs:1:1
   |
 1 | / ----cargo
...  |
14 | |     "----"
15 | | }
   | |__^
   |
note: frontmatter opening here was not closed
  --> $DIR/unclosed-2.rs:1:1
   |
 1 | ----cargo
   | ^^^^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: unclosed frontmatter
   ╭▸ $DIR/unclosed-2.rs:1:1
   │
 1 │ ┏ ----cargo
   ┆ ┇
14 │ ┃     "----"
15 │ ┃ }
   │ ┗━━┛
   ╰╴
note: frontmatter opening here was not closed
   ╭▸ $DIR/unclosed-2.rs:1:1
   │
 1 │ ----cargo
   ╰╴━━━━
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn unclosed_3() {
    // tests/ui/frontmatter/unclosed-3.rs

    let source = r#"----cargo
//~^ ERROR: frontmatter close does not match the opening

//@ compile-flags: --crate-type lib

// Unfortunate recovery situation. Not really preventable with improving the
// recovery strategy, but this type of code is rare enough already.

 #![feature(frontmatter)]

fn foo(x: i32) -> i32 {
    ---x
    //~^ ERROR: invalid preceding whitespace for frontmatter close
    //~| ERROR: extra characters after frontmatter close are not allowed
}
//~^ ERROR: unexpected closing delimiter: `}`
"#;

    let input = &[
        Level::ERROR
            .primary_title("invalid preceding whitespace for frontmatter close")
            .element(
                Snippet::source(source)
                    .path("$DIR/unclosed-3.rs")
                    .annotation(AnnotationKind::Primary.span(302..310)),
            ),
        Level::NOTE
            .secondary_title("frontmatter close should not be preceded by whitespace")
            .element(
                Snippet::source(source)
                    .path("$DIR/unclosed-3.rs")
                    .annotation(AnnotationKind::Primary.span(302..306)),
            ),
    ];
    let expected_ascii = str![[r#"
error: invalid preceding whitespace for frontmatter close
  --> $DIR/unclosed-3.rs:12:1
   |
12 |     ---x
   | ^^^^^^^^
   |
note: frontmatter close should not be preceded by whitespace
  --> $DIR/unclosed-3.rs:12:1
   |
12 |     ---x
   | ^^^^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: invalid preceding whitespace for frontmatter close
   ╭▸ $DIR/unclosed-3.rs:12:1
   │
12 │     ---x
   │ ━━━━━━━━
   ╰╴
note: frontmatter close should not be preceded by whitespace
   ╭▸ $DIR/unclosed-3.rs:12:1
   │
12 │     ---x
   ╰╴━━━━
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn unclosed_4() {
    // tests/ui/frontmatter/unclosed-4.rs

    let source = r#"----cargo
//~^ ERROR: unclosed frontmatter

//! Similarly, a module-level content should allow for recovery as well (as
//! per unclosed-1.rs)

#![feature(frontmatter)]

fn main() {}
"#;

    let input = &[
        Level::ERROR.primary_title("unclosed frontmatter").element(
            Snippet::source(source)
                .path("$DIR/unclosed-4.rs")
                .annotation(AnnotationKind::Primary.span(0..43)),
        ),
        Level::NOTE
            .secondary_title("frontmatter opening here was not closed")
            .element(
                Snippet::source(source)
                    .path("$DIR/unclosed-4.rs")
                    .annotation(AnnotationKind::Primary.span(0..4)),
            ),
    ];
    let expected_ascii = str![[r#"
error: unclosed frontmatter
 --> $DIR/unclosed-4.rs:1:1
  |
1 | / ----cargo
2 | | //~^ ERROR: unclosed frontmatter
  | |________________________________^
  |
note: frontmatter opening here was not closed
 --> $DIR/unclosed-4.rs:1:1
  |
1 | ----cargo
  | ^^^^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: unclosed frontmatter
  ╭▸ $DIR/unclosed-4.rs:1:1
  │
1 │ ┏ ----cargo
2 │ ┃ //~^ ERROR: unclosed frontmatter
  │ ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
  ╰╴
note: frontmatter opening here was not closed
  ╭▸ $DIR/unclosed-4.rs:1:1
  │
1 │ ----cargo
  ╰╴━━━━
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn unclosed_5() {
    // tests/ui/frontmatter/unclosed-5.rs

    let source = r#"----cargo
//~^ ERROR: unclosed frontmatter
//~| ERROR: frontmatters are experimental

// Similarly, a use statement should allow for recovery as well (as
// per unclosed-1.rs)

use std::env;

fn main() {}
"#;

    let input = &[
        Level::ERROR.primary_title("unclosed frontmatter").element(
            Snippet::source(source)
                .path("$DIR/unclosed-5.rs")
                .annotation(AnnotationKind::Primary.span(0..176)),
        ),
        Level::NOTE
            .secondary_title("frontmatter opening here was not closed")
            .element(
                Snippet::source(source)
                    .path("$DIR/unclosed-5.rs")
                    .annotation(AnnotationKind::Primary.span(0..4)),
            ),
    ];

    let expected_ascii = str![[r#"
error: unclosed frontmatter
 --> $DIR/unclosed-5.rs:1:1
  |
1 | / ----cargo
... |
6 | | // per unclosed-1.rs)
  | |_____________________^
  |
note: frontmatter opening here was not closed
 --> $DIR/unclosed-5.rs:1:1
  |
1 | ----cargo
  | ^^^^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: unclosed frontmatter
  ╭▸ $DIR/unclosed-5.rs:1:1
  │
1 │ ┏ ----cargo
  ┆ ┇
6 │ ┃ // per unclosed-1.rs)
  │ ┗━━━━━━━━━━━━━━━━━━━━━┛
  ╰╴
note: frontmatter opening here was not closed
  ╭▸ $DIR/unclosed-5.rs:1:1
  │
1 │ ----cargo
  ╰╴━━━━
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn pat_tuple_field_count_cross() {
    // tests/ui/pattern/pat-tuple-field-count-cross.stderr

    let source = r#"//@ aux-build:declarations-for-tuple-field-count-errors.rs

extern crate declarations_for_tuple_field_count_errors;

use declarations_for_tuple_field_count_errors::*;

fn main() {
    match Z0 {
        Z0() => {} //~ ERROR expected tuple struct or tuple variant, found unit struct `Z0`
        Z0(x) => {} //~ ERROR expected tuple struct or tuple variant, found unit struct `Z0`
    }
    match Z1() {
        Z1 => {} //~ ERROR match bindings cannot shadow tuple structs
        Z1(x) => {} //~ ERROR this pattern has 1 field, but the corresponding tuple struct has 0 fields
    }

    match S(1, 2, 3) {
        S() => {} //~ ERROR this pattern has 0 fields, but the corresponding tuple struct has 3 fields
        S(1) => {} //~ ERROR this pattern has 1 field, but the corresponding tuple struct has 3 fields
        S(xyz, abc) => {} //~ ERROR this pattern has 2 fields, but the corresponding tuple struct has 3 fields
        S(1, 2, 3, 4) => {} //~ ERROR this pattern has 4 fields, but the corresponding tuple struct has 3 fields
    }
    match M(1, 2, 3) {
        M() => {} //~ ERROR this pattern has 0 fields, but the corresponding tuple struct has 3 fields
        M(1) => {} //~ ERROR this pattern has 1 field, but the corresponding tuple struct has 3 fields
        M(xyz, abc) => {} //~ ERROR this pattern has 2 fields, but the corresponding tuple struct has 3 fields
        M(1, 2, 3, 4) => {} //~ ERROR this pattern has 4 fields, but the corresponding tuple struct has 3 fields
    }

    match E1::Z0 {
        E1::Z0() => {} //~ ERROR expected tuple struct or tuple variant, found unit variant `E1::Z0`
        E1::Z0(x) => {} //~ ERROR expected tuple struct or tuple variant, found unit variant `E1::Z0`
    }
    match E1::Z1() {
        E1::Z1 => {} //~ ERROR expected unit struct, unit variant or constant, found tuple variant `E1::Z1`
        E1::Z1(x) => {} //~ ERROR this pattern has 1 field, but the corresponding tuple variant has 0 fields
    }
    match E1::S(1, 2, 3) {
        E1::S() => {} //~ ERROR this pattern has 0 fields, but the corresponding tuple variant has 3 fields
        E1::S(1) => {} //~ ERROR this pattern has 1 field, but the corresponding tuple variant has 3 fields
        E1::S(xyz, abc) => {} //~ ERROR this pattern has 2 fields, but the corresponding tuple variant has 3 fields
        E1::S(1, 2, 3, 4) => {} //~ ERROR this pattern has 4 fields, but the corresponding tuple variant has 3 fields
    }

    match E2::S(1, 2, 3) {
        E2::S() => {} //~ ERROR this pattern has 0 fields, but the corresponding tuple variant has 3 fields
        E2::S(1) => {} //~ ERROR this pattern has 1 field, but the corresponding tuple variant has 3 fields
        E2::S(xyz, abc) => {} //~ ERROR this pattern has 2 fields, but the corresponding tuple variant has 3 fields
        E2::S(1, 2, 3, 4) => {} //~ ERROR this pattern has 4 fields, but the corresponding tuple variant has 3 fields
    }
    match E2::M(1, 2, 3) {
        E2::M() => {} //~ ERROR this pattern has 0 fields, but the corresponding tuple variant has 3 fields
        E2::M(1) => {} //~ ERROR this pattern has 1 field, but the corresponding tuple variant has 3 fields
        E2::M(xyz, abc) => {} //~ ERROR this pattern has 2 fields, but the corresponding tuple variant has 3 fields
        E2::M(1, 2, 3, 4) => {} //~ ERROR this pattern has 4 fields, but the corresponding tuple variant has 3 fields
    }
}
"#;
    let source1 = r#"pub struct Z0;
pub struct Z1();

pub struct S(pub u8, pub u8, pub u8);
pub struct M(
    pub u8,
    pub u8,
    pub u8,
);

pub enum E1 { Z0, Z1(), S(u8, u8, u8) }

pub enum E2 {
    S(u8, u8, u8),
    M(
        u8,
        u8,
        u8,
    ),
}
"#;

    let input = &[
        Level::ERROR
            .primary_title(
                "expected unit struct, unit variant or constant, found tuple variant `E1::Z1`",
            )
            .id(r#"E0532"#)
            .element(
                Snippet::source(source)
                    .path("$DIR/pat-tuple-field-count-cross.rs")
                    .annotation(AnnotationKind::Primary.span(1760..1766)),
            )
            .element(
                Snippet::source(source1)
                    .path("$DIR/auxiliary/declarations-for-tuple-field-count-errors.rs")
                    .annotation(
                        AnnotationKind::Context
                            .span(143..145)
                            .label("`E1::Z1` defined here"),
                    )
                    .annotation(
                        AnnotationKind::Context
                            .span(139..141)
                            .label("similarly named unit variant `Z0` defined here"),
                    ),
            ),
        Level::HELP
            .secondary_title("use the tuple variant pattern syntax instead")
            .element(
                Snippet::source(source)
                    .path("$DIR/pat-tuple-field-count-cross.rs")
                    .patch(Patch::new(1760..1766, r#"E1::Z1()"#)),
            ),
        Level::HELP
            .secondary_title("a unit variant with a similar name exists")
            .element(
                Snippet::source(source)
                    .path("$DIR/pat-tuple-field-count-cross.rs")
                    .patch(Patch::new(1764..1766, r#"Z0"#)),
            ),
    ];
    let expected_ascii = str![[r#"
error[E0532]: expected unit struct, unit variant or constant, found tuple variant `E1::Z1`
  --> $DIR/pat-tuple-field-count-cross.rs:35:9
   |
35 |         E1::Z1 => {} //~ ERROR expected unit struct, unit variant or constant, found tuple variant `E1::Z1`
   |         ^^^^^^
   |
  ::: $DIR/auxiliary/declarations-for-tuple-field-count-errors.rs:11:19
   |
11 | pub enum E1 { Z0, Z1(), S(u8, u8, u8) }
   |               --  -- `E1::Z1` defined here
   |               |
   |               similarly named unit variant `Z0` defined here
   |
help: use the tuple variant pattern syntax instead
   |
35 |         E1::Z1() => {} //~ ERROR expected unit struct, unit variant or constant, found tuple variant `E1::Z1`
   |               ++
help: a unit variant with a similar name exists
   |
35 -         E1::Z1 => {} //~ ERROR expected unit struct, unit variant or constant, found tuple variant `E1::Z1`
35 +         E1::Z0 => {} //~ ERROR expected unit struct, unit variant or constant, found tuple variant `E1::Z1`
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0532]: expected unit struct, unit variant or constant, found tuple variant `E1::Z1`
   ╭▸ $DIR/pat-tuple-field-count-cross.rs:35:9
   │
35 │         E1::Z1 => {} //~ ERROR expected unit struct, unit variant or constant, found tuple variant `E1::Z1`
   │         ━━━━━━
   │
   ⸬  $DIR/auxiliary/declarations-for-tuple-field-count-errors.rs:11:19
   │
11 │ pub enum E1 { Z0, Z1(), S(u8, u8, u8) }
   │               ┬─  ── `E1::Z1` defined here
   │               │
   │               similarly named unit variant `Z0` defined here
   ╰╴
help: use the tuple variant pattern syntax instead
   ╭╴
35 │         E1::Z1() => {} //~ ERROR expected unit struct, unit variant or constant, found tuple variant `E1::Z1`
   ╰╴              ++
help: a unit variant with a similar name exists
   ╭╴
35 -         E1::Z1 => {} //~ ERROR expected unit struct, unit variant or constant, found tuple variant `E1::Z1`
35 +         E1::Z0 => {} //~ ERROR expected unit struct, unit variant or constant, found tuple variant `E1::Z1`
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn unterminated_nested_comment() {
    // tests/ui/lexer/unterminated-nested-comment.rs

    let source = r#"/* //~ ERROR E0758
/* */
/*
*/
"#;

    let input = &[Level::ERROR
        .primary_title("unterminated block comment")
        .id("E0758")
        .element(
            Snippet::source(source)
                .path("$DIR/unterminated-nested-comment.rs")
                .annotation(
                    AnnotationKind::Context
                        .span(0..2)
                        .label("unterminated block comment"),
                )
                .annotation(AnnotationKind::Context.span(25..27).label(
                    "...as last nested comment starts here, maybe you want to close this instead?",
                ))
                .annotation(
                    AnnotationKind::Context
                        .span(28..30)
                        .label("...and last nested comment terminates here."),
                )
                .annotation(AnnotationKind::Primary.span(0..31)),
        )];

    let expected_ascii = str![[r#"
error[E0758]: unterminated block comment
 --> $DIR/unterminated-nested-comment.rs:1:1
  |
1 |   /* //~ ERROR E0758
  |   ^-
  |   |
  |  _unterminated block comment
  | |
2 | | /* */
3 | | /*
  | | --
  | | |
  | | ...as last nested comment starts here, maybe you want to close this instead?
4 | | */
  | |_--^
  |   |
  |   ...and last nested comment terminates here.
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0758]: unterminated block comment
  ╭▸ $DIR/unterminated-nested-comment.rs:1:1
  │
1 │   /* //~ ERROR E0758
  │   ╿─
  │   │
  │ ┏━unterminated block comment
  │ ┃
2 │ ┃ /* */
3 │ ┃ /*
  │ ┃ ┬─
  │ ┃ │
  │ ┃ ...as last nested comment starts here, maybe you want to close this instead?
4 │ ┃ */
  │ ┗━┬─┛
  │   │
  ╰╴  ...and last nested comment terminates here.
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn mismatched_types1() {
    // tests/ui/include-macros/mismatched-types.rs

    let file_txt_source = r#""#;

    let rust_source = r#"fn main() {
    let b: &[u8] = include_str!("file.txt");    //~ ERROR mismatched types
    let s: &str = include_bytes!("file.txt");   //~ ERROR mismatched types
}"#;

    let input = &[Level::ERROR
        .primary_title("mismatched types")
        .id("E0308")
        .element(
            Snippet::source(file_txt_source)
                .line_start(3)
                .path("$DIR/file.txt")
                .annotation(
                    AnnotationKind::Primary
                        .span(0..0)
                        .label("expected `&[u8]`, found `&str`"),
                ),
        )
        .element(
            Snippet::source(rust_source)
                .path("$DIR/mismatched-types.rs")
                .annotation(
                    AnnotationKind::Context
                        .span(23..28)
                        .label("expected due to this"),
                )
                .annotation(
                    AnnotationKind::Context
                        .span(31..55)
                        .label("in this macro invocation"),
                ),
        )
        .element(
            Level::NOTE.message("expected reference `&[u8]`\n   found reference `&'static str`"),
        )];

    let expected_ascii = str![[r#"
error[E0308]: mismatched types
 --> $DIR/file.txt:3:1
  |
3 |
  | ^ expected `&[u8]`, found `&str`
  |
 ::: $DIR/mismatched-types.rs:2:12
  |
2 |     let b: &[u8] = include_str!("file.txt");    //~ ERROR mismatched types
  |            -----   ------------------------ in this macro invocation
  |            |
  |            expected due to this
  |
  = note: expected reference `&[u8]`
             found reference `&'static str`
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0308]: mismatched types
  ╭▸ $DIR/file.txt:3:1
  │
3 │
  │ ━ expected `&[u8]`, found `&str`
  │
  ⸬  $DIR/mismatched-types.rs:2:12
  │
2 │     let b: &[u8] = include_str!("file.txt");    //~ ERROR mismatched types
  │            ┬────   ──────────────────────── in this macro invocation
  │            │
  │            expected due to this
  │
  ╰ note: expected reference `&[u8]`
             found reference `&'static str`
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn mismatched_types2() {
    // tests/ui/include-macros/mismatched-types.rs

    let source = r#"fn main() {
    let b: &[u8] = include_str!("file.txt");    //~ ERROR mismatched types
    let s: &str = include_bytes!("file.txt");   //~ ERROR mismatched types
}"#;

    let input = &[Level::ERROR
        .primary_title("mismatched types")
        .id("E0308")
        .element(
            Snippet::source(source)
                .path("$DIR/mismatched-types.rs")
                .annotation(
                    AnnotationKind::Primary
                        .span(105..131)
                        .label("expected `&str`, found `&[u8; 0]`"),
                )
                .annotation(
                    AnnotationKind::Context
                        .span(98..102)
                        .label("expected due to this"),
                ),
        )
        .element(
            Level::NOTE.message("expected reference `&str`\n   found reference `&'static [u8; 0]`"),
        )];

    let expected_ascii = str![[r#"
error[E0308]: mismatched types
 --> $DIR/mismatched-types.rs:3:19
  |
3 |     let s: &str = include_bytes!("file.txt");   //~ ERROR mismatched types
  |            ----   ^^^^^^^^^^^^^^^^^^^^^^^^^^ expected `&str`, found `&[u8; 0]`
  |            |
  |            expected due to this
  |
  = note: expected reference `&str`
             found reference `&'static [u8; 0]`
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0308]: mismatched types
  ╭▸ $DIR/mismatched-types.rs:3:19
  │
3 │     let s: &str = include_bytes!("file.txt");   //~ ERROR mismatched types
  │            ┬───   ━━━━━━━━━━━━━━━━━━━━━━━━━━ expected `&str`, found `&[u8; 0]`
  │            │
  │            expected due to this
  │
  ╰ note: expected reference `&str`
             found reference `&'static [u8; 0]`
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn short_error_format1() {
    // tests/ui/short-error-format.rs

    let source = r#"//@ compile-flags: --error-format=short

fn foo(_: u32) {}

fn main() {
    foo("Bonjour".to_owned());
    let x = 0u32;
    x.salut();
}
"#;

    let input = &[
        Level::ERROR
            .primary_title("mismatched types")
            .id("E0308")
            .element(
                Snippet::source(source)
                    .path("$DIR/short-error-format.rs")
                    .annotation(
                        AnnotationKind::Primary
                            .span(80..100)
                            .label("expected `u32`, found `String`"),
                    )
                    .annotation(
                        AnnotationKind::Context
                            .span(76..79)
                            .label("arguments to this function are incorrect"),
                    ),
            ),
        Level::NOTE
            .secondary_title("function defined here")
            .element(
                Snippet::source(source)
                    .path("$DIR/short-error-format.rs")
                    .annotation(AnnotationKind::Context.span(48..54).label(""))
                    .annotation(AnnotationKind::Primary.span(44..47)),
            ),
    ];

    let expected_ascii = str![[r#"
$DIR/short-error-format.rs:6:9: error[E0308]: mismatched types: expected `u32`, found `String`
"#]];
    let renderer = Renderer::plain().short_message(true);
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![
        "$DIR/short-error-format.rs:6:9: error[E0308]: mismatched types: expected `u32`, found `String`"
    ];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn short_error_format2() {
    // tests/ui/short-error-format.rs

    let source = r#"//@ compile-flags: --error-format=short

fn foo(_: u32) {}

fn main() {
    foo("Bonjour".to_owned());
    let x = 0u32;
    x.salut();
}
"#;

    let input = &[Level::ERROR
        .primary_title("no method named `salut` found for type `u32` in the current scope")
        .id("E0599")
        .element(
            Snippet::source(source)
                .path("$DIR/short-error-format.rs")
                .annotation(
                    AnnotationKind::Primary
                        .span(127..132)
                        .label("method not found in `u32`"),
                ),
        )];

    let expected_ascii = str![[r#"
$DIR/short-error-format.rs:8:7: error[E0599]: no method named `salut` found for type `u32` in the current scope: method not found in `u32`
"#]];
    let renderer = Renderer::plain().short_message(true);
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![
        "$DIR/short-error-format.rs:8:7: error[E0599]: no method named `salut` found for type `u32` in the current scope: method not found in `u32`"
    ];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn rustdoc_ui_diagnostic_width() {
    // tests/rustdoc-ui/diagnostic-width.rs

    let source_0 = r#"//@ compile-flags: --diagnostic-width=10
#![deny(rustdoc::bare_urls)]

/// This is a long line that contains a http://link.com
pub struct Foo; //~^ ERROR
"#;
    let source_1 = r#"/// This is a long line that contains a http://link.com
"#;

    let input = &[
        Level::ERROR
            .primary_title("this URL is not a hyperlink")
            .element(
                Snippet::source(source_0)
                    .path("$DIR/diagnostic-width.rs")
                    .annotation(AnnotationKind::Primary.span(111..126)),
            )
            .element(
                Level::NOTE.message("bare URLs are not automatically turned into clickable links"),
            ),
        Level::NOTE
            .secondary_title("the lint level is defined here")
            .element(
                Snippet::source(source_0)
                    .path("$DIR/diagnostic-width.rs")
                    .annotation(AnnotationKind::Primary.span(49..67)),
            ),
        Level::HELP
            .secondary_title("use an automatic link instead")
            .element(
                Snippet::source(source_1)
                    .path("$DIR/diagnostic-width.rs")
                    .line_start(4)
                    .patch(Patch::new(40..40, "<"))
                    .patch(Patch::new(55..55, ">")),
            ),
    ];

    let expected_ascii = str![[r#"
error: this URL is not a hyperlink
 --> $DIR/diagnostic-width.rs:4:41
  |
4 | ... a http:...k.com
  |       ^^^^^...^^^^^
  |
  = note: bare URLs are not automatically turned into clickable links
note: the lint level is defined here
 --> $DIR/diagnostic-width.rs:2:9
  |
2 | ...ny(rustd..._urls)]
  |       ^^^^^...^^^^^
help: use an automatic link instead
  |
4 | /// This is a long line that contains a <http://link.com>
  |                                         +               +
"#]];
    let renderer = Renderer::plain().term_width(10);
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: this URL is not a hyperlink
  ╭▸ $DIR/diagnostic-width.rs:4:41
  │
4 │ …ns a http:…k.com
  │       ━━━━━…━━━━━
  │
  ╰ note: bare URLs are not automatically turned into clickable links
note: the lint level is defined here
  ╭▸ $DIR/diagnostic-width.rs:2:9
  │
2 │ …deny(rustd…_urls)]
  ╰╴      ━━━━━…━━━━━
help: use an automatic link instead
  ╭╴
4 │ /// This is a long line that contains a <http://link.com>
  ╰╴                                        +               +
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn array_into_iter() {
    let source1 = r#"#![allow(unused)]
fn main() {
[1, 2, 3].into_iter().for_each(|n| { *n; });
}
"#;
    let source2 = r#"[1, 2, 3].into_iter().for_each(|n| { *n; });
"#;

    let long_title1 = "this method call resolves to `<&[T; N] as IntoIterator>::into_iter` (due to backwards compatibility), but will resolve to `<[T; N] as IntoIterator>::into_iter` in Rust 2021";
    let long_title2 = "for more information, see <https://doc.rust-lang.org/nightly/edition-guide/rust-2021/IntoIterator-for-arrays.html>";
    let long_title3 = "or use `IntoIterator::into_iter(..)` instead of `.into_iter()` to explicitly iterate by value";

    let input = &[
        Level::WARNING
            .primary_title(long_title1)
            .element(
                Snippet::source(source1)
                    .path("lint_example.rs")
                    .annotation(AnnotationKind::Primary.span(40..49)),
            )
            .element(Level::WARNING.message("this changes meaning in Rust 2021"))
            .element(Level::NOTE.message(long_title2))
            .element(Level::NOTE.message("`#[warn(array_into_iter)]` on by default")),
        Level::HELP
            .secondary_title("use `.iter()` instead of `.into_iter()` to avoid ambiguity")
            .element(
                Snippet::source(source2)
                    .path("lint_example.rs")
                    .line_start(3)
                    .patch(Patch::new(10..19, "iter")),
            ),
        Level::HELP.secondary_title(long_title3).element(
            Snippet::source(source2)
                .path("lint_example.rs")
                .line_start(3)
                .patch(Patch::new(0..0, "IntoIterator::into_iter("))
                .patch(Patch::new(9..21, ")")),
        ),
    ];

    let expected_ascii = str![[r#"
warning: this method call resolves to `<&[T; N] as IntoIterator>::into_iter` (due to backwards compatibility), but will resolve to `<[T; N] as IntoIterator>::into_iter` in Rust 2021
 --> lint_example.rs:3:11
  |
3 | [1, 2, 3].into_iter().for_each(|n| { *n; });
  |           ^^^^^^^^^
  |
  = warning: this changes meaning in Rust 2021
  = note: for more information, see <https://doc.rust-lang.org/nightly/edition-guide/rust-2021/IntoIterator-for-arrays.html>
  = note: `#[warn(array_into_iter)]` on by default
help: use `.iter()` instead of `.into_iter()` to avoid ambiguity
  |
3 - [1, 2, 3].into_iter().for_each(|n| { *n; });
3 + [1, 2, 3].iter().for_each(|n| { *n; });
  |
help: or use `IntoIterator::into_iter(..)` instead of `.into_iter()` to explicitly iterate by value
  |
3 - [1, 2, 3].into_iter().for_each(|n| { *n; });
3 + IntoIterator::into_iter([1, 2, 3]).for_each(|n| { *n; });
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
warning: this method call resolves to `<&[T; N] as IntoIterator>::into_iter` (due to backwards compatibility), but will resolve to `<[T; N] as IntoIterator>::into_iter` in Rust 2021
  ╭▸ lint_example.rs:3:11
  │
3 │ [1, 2, 3].into_iter().for_each(|n| { *n; });
  │           ━━━━━━━━━
  │
  ├ warning: this changes meaning in Rust 2021
  ├ note: for more information, see <https://doc.rust-lang.org/nightly/edition-guide/rust-2021/IntoIterator-for-arrays.html>
  ╰ note: `#[warn(array_into_iter)]` on by default
help: use `.iter()` instead of `.into_iter()` to avoid ambiguity
  ╭╴
3 - [1, 2, 3].into_iter().for_each(|n| { *n; });
3 + [1, 2, 3].iter().for_each(|n| { *n; });
  ╰╴
help: or use `IntoIterator::into_iter(..)` instead of `.into_iter()` to explicitly iterate by value
  ╭╴
3 - [1, 2, 3].into_iter().for_each(|n| { *n; });
3 + IntoIterator::into_iter([1, 2, 3]).for_each(|n| { *n; });
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn autoderef_box_no_add() {
    // tests/ui/autoref-autoderef/autoderef-box-no-add.rs

    let source = r#"//! Tests that auto-dereferencing does not allow addition of `Box<isize>` values.
//!
//! This test ensures that `Box<isize>` fields in structs (`Clam` and `Fish`) are not
//! automatically dereferenced to `isize` during addition operations, as `Box<isize>`
//! does not implement the `Add` trait.

struct Clam {
    x: Box<isize>,
    y: Box<isize>,
}

struct Fish {
    a: Box<isize>,
}

fn main() {
    let a: Clam = Clam {
        x: Box::new(1),
        y: Box::new(2),
    };
    let b: Clam = Clam {
        x: Box::new(10),
        y: Box::new(20),
    };
    let z: isize = a.x + b.y;
    //~^ ERROR cannot add `Box<isize>` to `Box<isize>`
    println!("{}", z);
    assert_eq!(z, 21);
    let forty: Fish = Fish { a: Box::new(40) };
    let two: Fish = Fish { a: Box::new(2) };
    let answer: isize = forty.a + two.a;
    //~^ ERROR cannot add `Box<isize>` to `Box<isize>`
    println!("{}", answer);
    assert_eq!(answer, 42);
}
"#;
    let input = &[
        Level::ERROR
            .primary_title("cannot add `Box<isize>` to `Box<isize>`")
            .id("E0369")
            .element(
                Snippet::source(source)
                    .path("$DIR/autoderef-box-no-add.rs")
                    .annotation(AnnotationKind::Context.span(583..586).label("Box<isize>"))
                    .annotation(AnnotationKind::Context.span(589..592).label("Box<isize>"))
                    .annotation(AnnotationKind::Primary.span(587..588)),
            ),
        Level::NOTE
            .secondary_title("the foreign item type `Box<isize>` doesn't implement `Add`")
            .element(
                Origin::path("$SRC_DIR/alloc/src/boxed.rs")
                    .line(231)
                    .char_column(0),
            )
            .element(
                Origin::path("$SRC_DIR/alloc/src/boxed.rs")
                    .line(234)
                    .char_column(1),
            )
            .element(Padding)
            .element(Level::NOTE.message("not implement `Add`")),
    ];

    let expected_ascii = str![[r#"
error[E0369]: cannot add `Box<isize>` to `Box<isize>`
  --> $DIR/autoderef-box-no-add.rs:25:24
   |
25 |     let z: isize = a.x + b.y;
   |                    --- ^ --- Box<isize>
   |                    |
   |                    Box<isize>
   |
note: the foreign item type `Box<isize>` doesn't implement `Add`
  --> $SRC_DIR/alloc/src/boxed.rs:231:0
  ::: $SRC_DIR/alloc/src/boxed.rs:234:1
   |
   = note: not implement `Add`
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0369]: cannot add `Box<isize>` to `Box<isize>`
   ╭▸ $DIR/autoderef-box-no-add.rs:25:24
   │
25 │     let z: isize = a.x + b.y;
   │                    ┬── ━ ─── Box<isize>
   │                    │
   │                    Box<isize>
   ╰╴
note: the foreign item type `Box<isize>` doesn't implement `Add`
   ╭▸ $SRC_DIR/alloc/src/boxed.rs:231:0
   ⸬  $SRC_DIR/alloc/src/boxed.rs:234:1
   │
   ╰ note: not implement `Add`
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn dont_project_to_specializable_projection() {
    // tests/ui/async-await/in-trait/dont-project-to-specializable-projection.rs

    let source = r#"//@ edition: 2021
//@ known-bug: #108309

#![feature(min_specialization)]

struct MyStruct;

trait MyTrait<T> {
    async fn foo(_: T) -> &'static str;
}

impl<T> MyTrait<T> for MyStruct {
    default async fn foo(_: T) -> &'static str {
        "default"
    }
}

impl MyTrait<i32> for MyStruct {
    async fn foo(_: i32) -> &'static str {
        "specialized"
    }
}

async fn async_main() {
    assert_eq!(MyStruct::foo(42).await, "specialized");
    assert_eq!(indirection(42).await, "specialized");
}

async fn indirection<T>(x: T) -> &'static str {
    //explicit type coercion is currently necessary
    // because of https://github.com/rust-lang/rust/issues/67918
    <MyStruct as MyTrait<T>>::foo(x).await
}

// ------------------------------------------------------------------------- //
// Implementation Details Below...

use std::pin::{pin, Pin};
use std::task::*;

fn main() {
    let mut fut = pin!(async_main());

    // Poll loop, just to test the future...
    let ctx = &mut Context::from_waker(Waker::noop());

    loop {
        match fut.as_mut().poll(ctx) {
            Poll::Pending => {}
            Poll::Ready(()) => break,
        }
    }
}
"#;

    let title_0 = "no method named `poll` found for struct `Pin<&mut impl Future<Output = ()>>` in the current scope";
    let title_1 = "trait `Future` which provides `poll` is implemented but not in scope; perhaps you want to import it";

    let input = &[
        Level::ERROR.primary_title(title_0).id("E0599").element(
            Snippet::source(source)
                .path("$DIR/dont-project-to-specializable-projection.rs")
                .annotation(
                    AnnotationKind::Primary
                        .span(1071..1075)
                        .label("method not found in `Pin<&mut impl Future<Output = ()>>`"),
                ),
        ),
        Group::with_level(Level::ERROR)
            .element(
                Origin::path("$SRC_DIR/core/src/future/future.rs")
                    .line(104)
                    .char_column(7),
            )
            .element(Padding)
            .element(
                Level::NOTE.message(
                    "the method is available for `Pin<&mut impl Future<Output = ()>>` here",
                ),
            )
            .element(Padding)
            .element(
                Level::HELP.message("items from traits can only be used if the trait is in scope"),
            ),
        Level::HELP.secondary_title(title_1).element(
            Snippet::source("struct MyStruct;\n")
                .path("$DIR/dont-project-to-specializable-projection.rs")
                .line_start(6)
                .patch(Patch::new(
                    0..0,
                    r#"use std::future::Future;
"#,
                )),
        ),
    ];
    let expected_ascii = str![[r#"
error[E0599]: no method named `poll` found for struct `Pin<&mut impl Future<Output = ()>>` in the current scope
  --> $DIR/dont-project-to-specializable-projection.rs:48:28
   |
48 |         match fut.as_mut().poll(ctx) {
   |                            ^^^^ method not found in `Pin<&mut impl Future<Output = ()>>`
   |
  --> $SRC_DIR/core/src/future/future.rs:104:7
   |
   = note: the method is available for `Pin<&mut impl Future<Output = ()>>` here
   |
   = help: items from traits can only be used if the trait is in scope
help: trait `Future` which provides `poll` is implemented but not in scope; perhaps you want to import it
   |
 6 + use std::future::Future;
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0599]: no method named `poll` found for struct `Pin<&mut impl Future<Output = ()>>` in the current scope
   ╭▸ $DIR/dont-project-to-specializable-projection.rs:48:28
   │
48 │         match fut.as_mut().poll(ctx) {
   │                            ━━━━ method not found in `Pin<&mut impl Future<Output = ()>>`
   ╰╴
   ╭▸ $SRC_DIR/core/src/future/future.rs:104:7
   │
   ├ note: the method is available for `Pin<&mut impl Future<Output = ()>>` here
   │
   ╰ help: items from traits can only be used if the trait is in scope
help: trait `Future` which provides `poll` is implemented but not in scope; perhaps you want to import it
   ╭╴
 6 + use std::future::Future;
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn binary_op_not_allowed_issue_125631() {
    // tests/ui/binop/binary-op-not-allowed-issue-125631.rs

    let source = r#"use std::io::{Error, ErrorKind};
use std::thread;

struct T1;
struct T2;

fn main() {
    (Error::new(ErrorKind::Other, "1"), T1, 1) == (Error::new(ErrorKind::Other, "1"), T1, 2);
    //~^ERROR binary operation `==` cannot be applied to type
    (Error::new(ErrorKind::Other, "2"), thread::current())
        == (Error::new(ErrorKind::Other, "2"), thread::current());
    //~^ERROR binary operation `==` cannot be applied to type
    (Error::new(ErrorKind::Other, "4"), thread::current(), T1, T2)
        == (Error::new(ErrorKind::Other, "4"), thread::current(), T1, T2);
    //~^ERROR binary operation `==` cannot be applied to type
}
"#;
    let title_0 = "binary operation `==` cannot be applied to type `(std::io::Error, Thread)`";
    let title_1 =
        "the foreign item types don't implement required traits for this operation to be valid";

    let input = &[
        Level::ERROR.primary_title(title_0).id("E0369").element(
            Snippet::source(source)
                .path("$DIR/binary-op-not-allowed-issue-125631.rs")
                .annotation(
                    AnnotationKind::Context
                        .span(246..300)
                        .label("(std::io::Error, Thread)"),
                )
                .annotation(
                    AnnotationKind::Context
                        .span(312..366)
                        .label("(std::io::Error, Thread)"),
                )
                .annotation(AnnotationKind::Primary.span(309..311)),
        ),
        Level::NOTE
            .secondary_title(title_1)
            .element(
                Origin::path("$SRC_DIR/std/src/io/error.rs")
                    .line(65)
                    .char_column(0),
            )
            .element(Padding)
            .element(Level::NOTE.message("not implement `PartialEq`")),
        Group::with_level(Level::NOTE)
            .element(
                Origin::path("$SRC_DIR/std/src/thread/mod.rs")
                    .line(1415)
                    .char_column(0),
            )
            .element(Padding)
            .element(Level::NOTE.message("not implement `PartialEq`")),
    ];

    let expected_ascii = str![[r#"
error[E0369]: binary operation `==` cannot be applied to type `(std::io::Error, Thread)`
  --> $DIR/binary-op-not-allowed-issue-125631.rs:11:9
   |
10 |     (Error::new(ErrorKind::Other, "2"), thread::current())
   |     ------------------------------------------------------ (std::io::Error, Thread)
11 |         == (Error::new(ErrorKind::Other, "2"), thread::current());
   |         ^^ ------------------------------------------------------ (std::io::Error, Thread)
   |
note: the foreign item types don't implement required traits for this operation to be valid
  --> $SRC_DIR/std/src/io/error.rs:65:0
   |
   = note: not implement `PartialEq`
  --> $SRC_DIR/std/src/thread/mod.rs:1415:0
   |
   = note: not implement `PartialEq`
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0369]: binary operation `==` cannot be applied to type `(std::io::Error, Thread)`
   ╭▸ $DIR/binary-op-not-allowed-issue-125631.rs:11:9
   │
10 │     (Error::new(ErrorKind::Other, "2"), thread::current())
   │     ────────────────────────────────────────────────────── (std::io::Error, Thread)
11 │         == (Error::new(ErrorKind::Other, "2"), thread::current());
   │         ━━ ────────────────────────────────────────────────────── (std::io::Error, Thread)
   ╰╴
note: the foreign item types don't implement required traits for this operation to be valid
   ╭▸ $SRC_DIR/std/src/io/error.rs:65:0
   │
   ╰ note: not implement `PartialEq`
   ╭▸ $SRC_DIR/std/src/thread/mod.rs:1415:0
   │
   ╰ note: not implement `PartialEq`
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn deriving_meta_unknown_trait() {
    // tests/ui/derives/deriving-meta-unknown-trait.rs

    let source = r#"#[derive(Eqr)]
//~^ ERROR cannot find derive macro `Eqr` in this scope
//~| ERROR cannot find derive macro `Eqr` in this scope
struct Foo;

pub fn main() {}
"#;

    let input = &[
        Level::ERROR
            .primary_title("cannot find derive macro `Eqr` in this scope")
            .element(
                Snippet::source(source)
                    .path("$DIR/deriving-meta-unknown-trait.rs")
                    .annotation(
                        AnnotationKind::Primary
                            .span(9..12)
                            .label("help: a derive macro with a similar name exists: `Eq`"),
                    ),
            ),
        Group::with_level(Level::ERROR)
            .element(
                Origin::path("$SRC_DIR/core/src/cmp.rs")
                    .line(356)
                    .char_column(0),
            )
            .element(Padding)
            .element(Level::NOTE.message("similarly named derive macro `Eq` defined here"))
            .element(Padding)
            .element(
                Level::NOTE
                    .message("duplicate diagnostic emitted due to `-Z deduplicate-diagnostics=no`"),
            ),
    ];

    let expected_ascii = str![[r#"
error: cannot find derive macro `Eqr` in this scope
 --> $DIR/deriving-meta-unknown-trait.rs:1:10
  |
1 | #[derive(Eqr)]
  |          ^^^ help: a derive macro with a similar name exists: `Eq`
  |
 --> $SRC_DIR/core/src/cmp.rs:356:0
  |
  = note: similarly named derive macro `Eq` defined here
  |
  = note: duplicate diagnostic emitted due to `-Z deduplicate-diagnostics=no`
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: cannot find derive macro `Eqr` in this scope
  ╭▸ $DIR/deriving-meta-unknown-trait.rs:1:10
  │
1 │ #[derive(Eqr)]
  │          ━━━ help: a derive macro with a similar name exists: `Eq`
  ╰╴
  ╭▸ $SRC_DIR/core/src/cmp.rs:356:0
  │
  ├ note: similarly named derive macro `Eq` defined here
  │
  ╰ note: duplicate diagnostic emitted due to `-Z deduplicate-diagnostics=no`
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn not_repeatable() {
    // tests/ui/proc-macro/quote/not-repeatable.rs

    let source = r#"#![feature(proc_macro_quote)]

extern crate proc_macro;

use proc_macro::quote;

struct Ipv4Addr;

fn main() {
    let ip = Ipv4Addr;
    let _ = quote! { $($ip)* }; //~ ERROR the method `quote_into_iter` exists for struct `Ipv4Addr`, but its trait bounds were not satisfied
}
"#;
    let label_0 = "method `quote_into_iter` not found for this struct because it doesn't satisfy `Ipv4Addr: Iterator`, `Ipv4Addr: ToTokens`, `Ipv4Addr: proc_macro::ext::RepIteratorExt` or `Ipv4Addr: proc_macro::ext::RepToTokensExt`";
    let title_0 = "the method `quote_into_iter` exists for struct `Ipv4Addr`, but its trait bounds were not satisfied";
    let title_1 = r#"the following trait bounds were not satisfied:
`Ipv4Addr: Iterator`
which is required by `Ipv4Addr: proc_macro::ext::RepIteratorExt`
`&Ipv4Addr: Iterator`
which is required by `&Ipv4Addr: proc_macro::ext::RepIteratorExt`
`Ipv4Addr: ToTokens`
which is required by `Ipv4Addr: proc_macro::ext::RepToTokensExt`
`&mut Ipv4Addr: Iterator`
which is required by `&mut Ipv4Addr: proc_macro::ext::RepIteratorExt`"#;

    let input = &[
        Level::ERROR
            .primary_title(title_0)
            .id("E0599")
            .element(
                Snippet::source(source)
                    .path("$DIR/not-repeatable.rs")
                    .annotation(AnnotationKind::Primary.span(146..164).label(
                        "method cannot be called on `Ipv4Addr` due to unsatisfied trait bounds",
                    ))
                    .annotation(AnnotationKind::Context.span(81..96).label(label_0)),
            )
            .element(Level::NOTE.message(title_1)),
        Level::NOTE
            .secondary_title("the traits `Iterator` and `ToTokens` must be implemented")
            .element(
                Origin::path("$SRC_DIR/proc_macro/src/to_tokens.rs")
                    .line(11)
                    .char_column(0),
            ),
        Group::with_level(Level::NOTE).element(
            Origin::path("$SRC_DIR/core/src/iter/traits/iterator.rs")
                .line(39)
                .char_column(0),
        ),
    ];
    let expected_ascii = str![[r#"
error[E0599]: the method `quote_into_iter` exists for struct `Ipv4Addr`, but its trait bounds were not satisfied
  --> $DIR/not-repeatable.rs:11:13
   |
 7 | struct Ipv4Addr;
   | --------------- method `quote_into_iter` not found for this struct because it doesn't satisfy `Ipv4Addr: Iterator`, `Ipv4Addr: ToTokens`, `Ipv4Addr: proc_macro::ext::RepIteratorExt` or `Ipv4Addr: proc_macro::ext::RepToTokensExt`
...
11 |     let _ = quote! { $($ip)* }; //~ ERROR the method `quote_into_iter` exists for struct `Ipv4Addr`, but its trait bounds were not s...
   |             ^^^^^^^^^^^^^^^^^^ method cannot be called on `Ipv4Addr` due to unsatisfied trait bounds
   |
   = note: the following trait bounds were not satisfied:
           `Ipv4Addr: Iterator`
           which is required by `Ipv4Addr: proc_macro::ext::RepIteratorExt`
           `&Ipv4Addr: Iterator`
           which is required by `&Ipv4Addr: proc_macro::ext::RepIteratorExt`
           `Ipv4Addr: ToTokens`
           which is required by `Ipv4Addr: proc_macro::ext::RepToTokensExt`
           `&mut Ipv4Addr: Iterator`
           which is required by `&mut Ipv4Addr: proc_macro::ext::RepIteratorExt`
note: the traits `Iterator` and `ToTokens` must be implemented
  --> $SRC_DIR/proc_macro/src/to_tokens.rs:11:0
  --> $SRC_DIR/core/src/iter/traits/iterator.rs:39:0
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0599]: the method `quote_into_iter` exists for struct `Ipv4Addr`, but its trait bounds were not satisfied
   ╭▸ $DIR/not-repeatable.rs:11:13
   │
 7 │ struct Ipv4Addr;
   │ ─────────────── method `quote_into_iter` not found for this struct because it doesn't satisfy `Ipv4Addr: Iterator`, `Ipv4Addr: ToTokens`, `Ipv4Addr: proc_macro::ext::RepIteratorExt` or `Ipv4Addr: proc_macro::ext::RepToTokensExt`
   ┆
11 │     let _ = quote! { $($ip)* }; //~ ERROR the method `quote_into_iter` exists for struct `Ipv4Addr`, but its trait bounds were not sat…
   │             ━━━━━━━━━━━━━━━━━━ method cannot be called on `Ipv4Addr` due to unsatisfied trait bounds
   │
   ╰ note: the following trait bounds were not satisfied:
           `Ipv4Addr: Iterator`
           which is required by `Ipv4Addr: proc_macro::ext::RepIteratorExt`
           `&Ipv4Addr: Iterator`
           which is required by `&Ipv4Addr: proc_macro::ext::RepIteratorExt`
           `Ipv4Addr: ToTokens`
           which is required by `Ipv4Addr: proc_macro::ext::RepToTokensExt`
           `&mut Ipv4Addr: Iterator`
           which is required by `&mut Ipv4Addr: proc_macro::ext::RepIteratorExt`
note: the traits `Iterator` and `ToTokens` must be implemented
   ─▸ $SRC_DIR/proc_macro/src/to_tokens.rs:11:0
   ─▸ $SRC_DIR/core/src/iter/traits/iterator.rs:39:0
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn not_found_self_type_differs_shadowing_trait_item() {
    // tests/ui/associated-inherent-types/not-found-self-type-differs-shadowing-trait-item.rs

    let source = r#"#![feature(inherent_associated_types)]
#![allow(incomplete_features)]

// Check that it's okay to report “[inherent] associated type […] not found” for inherent associated
// type candidates that are not applicable (due to unsuitable Self type) even if there exists a
// “shadowed” associated type from a trait with the same name since its use would be ambiguous
// anyway if the IAT didn't exist.
// FIXME(inherent_associated_types): Figure out which error would be more helpful here.

//@ revisions: shadowed uncovered

struct S<T>(T);

trait Tr {
    type Pr;
}

impl<T> Tr for S<T> {
    type Pr = ();
}

#[cfg(shadowed)]
impl S<()> {
    type Pr = i32;
}

fn main() {
    let _: S::<bool>::Pr = ();
    //[shadowed]~^ ERROR associated type `Pr` not found
    //[uncovered]~^^ ERROR associated type `Pr` not found
}
"#;

    let input = &[Level::ERROR
        .primary_title("associated type `Pr` not found for `S<bool>` in the current scope")
        .id("E0220")
        .element(
            Snippet::source(source)
                .path("$DIR/not-found-self-type-differs-shadowing-trait-item.rs")
                .annotation(
                    AnnotationKind::Primary
                        .span(705..707)
                        .label("associated item not found in `S<bool>`"),
                )
                .annotation(
                    AnnotationKind::Context
                        .span(532..543)
                        .label("associated type `Pr` not found for this struct"),
                ),
        )
        .element(Level::NOTE.message("the associated type was found for\n"))];

    let expected_ascii = str![[r#"
error[E0220]: associated type `Pr` not found for `S<bool>` in the current scope
  --> $DIR/not-found-self-type-differs-shadowing-trait-item.rs:28:23
   |
12 | struct S<T>(T);
   | ----------- associated type `Pr` not found for this struct
...
28 |     let _: S::<bool>::Pr = ();
   |                       ^^ associated item not found in `S<bool>`
   |
   = note: the associated type was found for
           
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0220]: associated type `Pr` not found for `S<bool>` in the current scope
   ╭▸ $DIR/not-found-self-type-differs-shadowing-trait-item.rs:28:23
   │
12 │ struct S<T>(T);
   │ ─────────── associated type `Pr` not found for this struct
   ┆
28 │     let _: S::<bool>::Pr = ();
   │                       ━━ associated item not found in `S<bool>`
   │
   ╰ note: the associated type was found for
           
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn unsafe_extern_suggestion() {
    // tests/ui/rust-2024/unsafe-extern-blocks/unsafe-extern-suggestion.rs

    let source = r#"//@ run-rustfix

#![deny(missing_unsafe_on_extern)]
#![allow(unused)]

extern "C" {
    //~^ ERROR extern blocks should be unsafe [missing_unsafe_on_extern]
    //~| WARN this is accepted in the current edition (Rust 2015) but is a hard error in Rust 2024!
    static TEST1: i32;
    fn test1(i: i32);
}

unsafe extern "C" {
    static TEST2: i32;
    fn test2(i: i32);
}

fn main() {}
"#;

    let title_0 =
        "this is accepted in the current edition (Rust 2015) but is a hard error in Rust 2024!";
    let title_1 = "for more information, see <https://doc.rust-lang.org/nightly/edition-guide/rust-2024/unsafe-extern.html>";

    let input = &[
        Level::ERROR
            .primary_title("extern blocks should be unsafe")
            .element(
                Snippet::source(source)
                    .path("$DIR/unsafe-extern-suggestion.rs")
                    .annotation(
                        AnnotationKind::Context
                            .span(71..71)
                            .label("help: needs `unsafe` before the extern keyword: `unsafe`"),
                    )
                    .annotation(AnnotationKind::Primary.span(71..303)),
            )
            .element(Level::WARNING.message(title_0))
            .element(Level::NOTE.message(title_1)),
        Level::NOTE
            .secondary_title("the lint level is defined here")
            .element(
                Snippet::source(source)
                    .path("$DIR/unsafe-extern-suggestion.rs")
                    .annotation(AnnotationKind::Primary.span(25..49)),
            ),
    ];

    let expected_ascii = str![[r#"
error: extern blocks should be unsafe
  --> $DIR/unsafe-extern-suggestion.rs:6:1
   |
 6 |   extern "C" {
   |   ^
   |   |
   |  _help: needs `unsafe` before the extern keyword: `unsafe`
   | |
 7 | |     //~^ ERROR extern blocks should be unsafe [missing_unsafe_on_extern]
 8 | |     //~| WARN this is accepted in the current edition (Rust 2015) but is a hard error in Rust 2024!
 9 | |     static TEST1: i32;
10 | |     fn test1(i: i32);
11 | | }
   | |_^
   |
   = warning: this is accepted in the current edition (Rust 2015) but is a hard error in Rust 2024!
   = note: for more information, see <https://doc.rust-lang.org/nightly/edition-guide/rust-2024/unsafe-extern.html>
note: the lint level is defined here
  --> $DIR/unsafe-extern-suggestion.rs:3:9
   |
 3 | #![deny(missing_unsafe_on_extern)]
   |         ^^^^^^^^^^^^^^^^^^^^^^^^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: extern blocks should be unsafe
   ╭▸ $DIR/unsafe-extern-suggestion.rs:6:1
   │
 6 │   extern "C" {
   │   ╿
   │   │
   │ ┏━help: needs `unsafe` before the extern keyword: `unsafe`
   │ ┃
 7 │ ┃     //~^ ERROR extern blocks should be unsafe [missing_unsafe_on_extern]
 8 │ ┃     //~| WARN this is accepted in the current edition (Rust 2015) but is a hard error in Rust 2024!
 9 │ ┃     static TEST1: i32;
10 │ ┃     fn test1(i: i32);
11 │ ┃ }
   │ ┗━┛
   │
   ├ warning: this is accepted in the current edition (Rust 2015) but is a hard error in Rust 2024!
   ╰ note: for more information, see <https://doc.rust-lang.org/nightly/edition-guide/rust-2024/unsafe-extern.html>
note: the lint level is defined here
   ╭▸ $DIR/unsafe-extern-suggestion.rs:3:9
   │
 3 │ #![deny(missing_unsafe_on_extern)]
   ╰╴        ━━━━━━━━━━━━━━━━━━━━━━━━
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn alloc_error_handler_bad_signature_2() {
    // tests/ui/alloc-error/alloc-error-handler-bad-signature-2.rs

    let source = r#"//@ compile-flags:-C panic=abort

#![feature(alloc_error_handler)]
#![no_std]
#![no_main]

struct Layout;

#[alloc_error_handler]
fn oom(
    info: Layout, //~^ ERROR mismatched types
) { //~^^ ERROR mismatched types
    loop {}
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
"#;
    let title_0 =
        "`core::alloc::Layout` and `Layout` have similar names, but are actually distinct types";

    let input = &[
        Level::ERROR
            .primary_title("mismatched types")
            .id("E0308")
            .element(
                Snippet::source(source)
                    .path("$DIR/alloc-error-handler-bad-signature-2.rs")
                    .annotation(
                        AnnotationKind::Primary
                            .span(130..230)
                            .label("expected `Layout`, found `core::alloc::Layout`"),
                    )
                    .annotation(
                        AnnotationKind::Context
                            .span(130..185)
                            .label("arguments to this function are incorrect"),
                    )
                    .annotation(
                        AnnotationKind::Context
                            .span(107..129)
                            .label("in this procedural macro expansion"),
                    ),
            )
            .element(Level::NOTE.message(title_0)),
        Level::NOTE
            .secondary_title("`core::alloc::Layout` is defined in crate `core`")
            .element(
                Origin::path("$SRC_DIR/core/src/alloc/layout.rs")
                    .line(40)
                    .char_column(0),
            ),
        Level::NOTE
            .secondary_title("`Layout` is defined in the current crate")
            .element(
                Snippet::source(source)
                    .path("$DIR/alloc-error-handler-bad-signature-2.rs")
                    .annotation(AnnotationKind::Primary.span(91..104)),
            ),
        Level::NOTE
            .secondary_title("function defined here")
            .element(
                Snippet::source(source)
                    .path("$DIR/alloc-error-handler-bad-signature-2.rs")
                    .annotation(AnnotationKind::Context.span(142..154).label(""))
                    .annotation(AnnotationKind::Primary.span(133..136)),
            ),
    ];
    let expected_ascii = str![[r#"
error[E0308]: mismatched types
  --> $DIR/alloc-error-handler-bad-signature-2.rs:10:1
   |
 9 |    #[alloc_error_handler]
   |    ---------------------- in this procedural macro expansion
10 | // fn oom(
11 | ||     info: Layout, //~^ ERROR mismatched types
12 | || ) { //~^^ ERROR mismatched types
   | ||_- arguments to this function are incorrect
13 | |      loop {}
14 | |  }
   | |__^ expected `Layout`, found `core::alloc::Layout`
   |
   = note: `core::alloc::Layout` and `Layout` have similar names, but are actually distinct types
note: `core::alloc::Layout` is defined in crate `core`
  --> $SRC_DIR/core/src/alloc/layout.rs:40:0
note: `Layout` is defined in the current crate
  --> $DIR/alloc-error-handler-bad-signature-2.rs:7:1
   |
 7 | struct Layout;
   | ^^^^^^^^^^^^^
note: function defined here
  --> $DIR/alloc-error-handler-bad-signature-2.rs:10:4
   |
10 | fn oom(
   |    ^^^
11 |     info: Layout, //~^ ERROR mismatched types
   |     ------------
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0308]: mismatched types
   ╭▸ $DIR/alloc-error-handler-bad-signature-2.rs:10:1
   │
 9 │    #[alloc_error_handler]
   │    ────────────────────── in this procedural macro expansion
10 │ ┏┌ fn oom(
11 │ ┃│     info: Layout, //~^ ERROR mismatched types
12 │ ┃│ ) { //~^^ ERROR mismatched types
   │ ┃└─┘ arguments to this function are incorrect
13 │ ┃      loop {}
14 │ ┃  }
   │ ┗━━┛ expected `Layout`, found `core::alloc::Layout`
   │
   ╰ note: `core::alloc::Layout` and `Layout` have similar names, but are actually distinct types
note: `core::alloc::Layout` is defined in crate `core`
   ─▸ $SRC_DIR/core/src/alloc/layout.rs:40:0
note: `Layout` is defined in the current crate
   ╭▸ $DIR/alloc-error-handler-bad-signature-2.rs:7:1
   │
 7 │ struct Layout;
   ╰╴━━━━━━━━━━━━━
note: function defined here
   ╭▸ $DIR/alloc-error-handler-bad-signature-2.rs:10:4
   │
10 │ fn oom(
   │    ━━━
11 │     info: Layout, //~^ ERROR mismatched types
   ╰╴    ────────────
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn str_escape() {
    // tests/ui/str/str-escape.rs

    let source = r#"//@ check-pass
// ignore-tidy-tab
//@ edition: 2021

fn main() {
    let s = "\

             ";
    //~^^^ WARNING multiple lines skipped by escaped newline
    assert_eq!(s, "");

    let s = c"foo\
             bar
             ";
    //~^^^ WARNING whitespace symbol '\u{a0}' is not skipped
    assert_eq!(s, c"foo           bar\n             ");

    let s = "a\
 b";
    assert_eq!(s, "ab");

    let s = "a\
	b";
    assert_eq!(s, "ab");

    let s = b"a\
    
    b";
    //~^^ WARNING whitespace symbol '\u{c}' is not skipped
    // '\x0c' is ASCII whitespace, but it may not need skipped
    // discussion: https://github.com/rust-lang/rust/pull/108403
    assert_eq!(s, b"a\x0cb");
}
"#;

    let input = &[Level::WARNING
        .primary_title(r#"whitespace symbol '\u{a0}' is not skipped"#)
        .element(
            Snippet::source(source)
                .path("$DIR/str-escape.rs")
                .annotation(
                    AnnotationKind::Context
                        .span(203..205)
                        .label(r#"whitespace symbol '\u{a0}' is not skipped"#),
                )
                .annotation(AnnotationKind::Primary.span(199..205)),
        )];
    let expected_ascii = str![[r#"
warning: whitespace symbol '\u{a0}' is not skipped
  --> $DIR/str-escape.rs:12:18
   |
12 |       let s = c"foo\
   |  __________________^
13 | |              bar
   | |   ^ whitespace symbol '\u{a0}' is not skipped
   | |___|
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii.raw());

    let expected_unicode = str![[r#"
warning: whitespace symbol '\u{a0}' is not skipped
   ╭▸ $DIR/str-escape.rs:12:18
   │
12 │       let s = c"foo\
   │ ┏━━━━━━━━━━━━━━━━━━┛
13 │ ┃              bar
   │ ┃   ╿ whitespace symbol '\u{a0}' is not skipped
   │ ┗━━━│
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode.raw());
}

#[test]
fn origin_path_repeated() {
    // tests/ui/pattern/usefulness/match-privately-empty.rs
    let source = r#"//@ revisions: normal exhaustive_patterns
#![cfg_attr(exhaustive_patterns, feature(exhaustive_patterns))]
#![feature(never_type)]

mod private {
    pub struct Private {
        _bot: !,
        pub misc: bool,
    }
    pub const DATA: Option<Private> = None;
}

fn main() {
    match private::DATA {
        //~^ ERROR non-exhaustive patterns: `Some(Private { misc: true, .. })` not covered
        None => {}
        Some(private::Private { misc: false, .. }) => {}
    }
}
"#;
    let title_0 = "non-exhaustive patterns: `Some(Private { misc: true, .. })` not covered";
    let title_1 = "`Option<Private>` defined here";
    let title_2 = "ensure that all possible cases are being handled by adding a match arm with a wildcard pattern or an explicit pattern as shown";

    let input = &[
        Level::ERROR.primary_title(title_0).id("E0004").element(
            Snippet::source(source)
                .path("$DIR/match-privately-empty.rs")
                .annotation(
                    AnnotationKind::Primary
                        .span(286..299)
                        .label("pattern `Some(Private { misc: true, .. })` not covered"),
                ),
        ),
        Level::NOTE
            .secondary_title(title_1)
            .element(
                Origin::path("$SRC_DIR/core/src/option.rs")
                    .line(593)
                    .char_column(0),
            )
            .element(
                Origin::path("$SRC_DIR/core/src/option.rs")
                    .line(601)
                    .char_column(4),
            )
            .element(Padding)
            .element(Level::NOTE.message("not covered"))
            .element(Level::NOTE.message("the matched value is of type `Option<Private>`")),
        Level::HELP.secondary_title(title_2).element(
            Snippet::source(source)
                .path("$DIR/match-privately-empty.rs")
                .line_start(17)
                .fold(true)
                .patch(Patch::new(
                    468..468,
                    ",
        Some(Private { misc: true, .. }) => todo!()",
                )),
        ),
    ];
    let expected_ascii = str![[r#"
error[E0004]: non-exhaustive patterns: `Some(Private { misc: true, .. })` not covered
   ╭▸ $DIR/match-privately-empty.rs:14:11
   │
14 │     match private::DATA {
   │           ━━━━━━━━━━━━━ pattern `Some(Private { misc: true, .. })` not covered
   ╰╴
note: `Option<Private>` defined here
   ╭▸ $SRC_DIR/core/src/option.rs:593:0
   ⸬  $SRC_DIR/core/src/option.rs:601:4
   │
   ├ note: not covered
   ╰ note: the matched value is of type `Option<Private>`
help: ensure that all possible cases are being handled by adding a match arm with a wildcard pattern or an explicit pattern as shown
   ╭╴
33 ±         Some(private::Private { misc: false, .. }) => {},
34 +         Some(Private { misc: true, .. }) => todo!()
   ╰╴
"#]];
    let renderer = Renderer::plain().decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0004]: non-exhaustive patterns: `Some(Private { misc: true, .. })` not covered
   ╭▸ $DIR/match-privately-empty.rs:14:11
   │
14 │     match private::DATA {
   │           ━━━━━━━━━━━━━ pattern `Some(Private { misc: true, .. })` not covered
   ╰╴
note: `Option<Private>` defined here
   ╭▸ $SRC_DIR/core/src/option.rs:593:0
   ⸬  $SRC_DIR/core/src/option.rs:601:4
   │
   ├ note: not covered
   ╰ note: the matched value is of type `Option<Private>`
help: ensure that all possible cases are being handled by adding a match arm with a wildcard pattern or an explicit pattern as shown
   ╭╴
33 ±         Some(private::Private { misc: false, .. }) => {},
34 +         Some(Private { misc: true, .. }) => todo!()
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn origin_path_repeated_element_between() {
    // tests/ui/dyn-compatibility/bare-trait-dont-suggest-dyn.rs
    let source = r#"//@ revisions: old new
//@[old] edition:2015
//@[new] edition:2021
//@[new] run-rustfix
#![deny(bare_trait_objects)]
fn ord_prefer_dot(s: String) -> Ord {
    //[new]~^ ERROR expected a type, found a trait
    //[old]~^^ ERROR the trait `Ord` is not dyn compatible
    //[old]~| ERROR trait objects without an explicit `dyn` are deprecated
    //[old]~| WARNING this is accepted in the current edition (Rust 2015)
    (s.starts_with("."), s)
}
fn main() {
    let _ = ord_prefer_dot(String::new());
}
"#;
    let title_0 = "for a trait to be dyn compatible it needs to allow building a vtable
for more information, visit <https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility>";

    let input = &[
        Level::ERROR
            .primary_title("the trait `Ord` is not dyn compatible")
            .id("E0038")
            .element(
                Snippet::source(source)
                    .path("$DIR/bare-trait-dont-suggest-dyn.rs")
                    .annotation(
                        AnnotationKind::Primary
                            .span(149..152)
                            .label("`Ord` is not dyn compatible"),
                    ),
            ),
        Level::NOTE
            .secondary_title(title_0)
            .element(
                Origin::path("$SRC_DIR/core/src/cmp.rs")
                    .line(961)
                    .char_column(20),
            )
            .element(Padding)
            .element(Level::NOTE.message(
                "the trait is not dyn compatible because it uses `Self` as a type parameter",
            ))
            .element(
                Origin::path("$SRC_DIR/core/src/cmp.rs")
                    .line(338)
                    .char_column(14),
            )
            .element(Padding)
            .element(Level::NOTE.message(
                "the trait is not dyn compatible because it uses `Self` as a type parameter",
            )),
        Level::HELP
            .secondary_title("consider using an opaque type instead")
            .element(
                Snippet::source(source)
                    .path("$DIR/bare-trait-dont-suggest-dyn.rs")
                    .line_start(6)
                    .fold(true)
                    .patch(Patch::new(149..149, "impl ")),
            ),
    ];
    let expected_ascii = str![[r#"
error[E0038]: the trait `Ord` is not dyn compatible
   ╭▸ $DIR/bare-trait-dont-suggest-dyn.rs:6:33
   │
 6 │ fn ord_prefer_dot(s: String) -> Ord {
   │                                 ━━━ `Ord` is not dyn compatible
   ╰╴
note: for a trait to be dyn compatible it needs to allow building a vtable
      for more information, visit <https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility>
   ╭▸ $SRC_DIR/core/src/cmp.rs:961:20
   │
   ├ note: the trait is not dyn compatible because it uses `Self` as a type parameter
   ⸬  $SRC_DIR/core/src/cmp.rs:338:14
   │
   ╰ note: the trait is not dyn compatible because it uses `Self` as a type parameter
help: consider using an opaque type instead
   ╭╴
11 │ fn ord_prefer_dot(s: String) -> impl Ord {
   ╰╴                                ++++
"#]];
    let renderer = Renderer::plain().decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0038]: the trait `Ord` is not dyn compatible
   ╭▸ $DIR/bare-trait-dont-suggest-dyn.rs:6:33
   │
 6 │ fn ord_prefer_dot(s: String) -> Ord {
   │                                 ━━━ `Ord` is not dyn compatible
   ╰╴
note: for a trait to be dyn compatible it needs to allow building a vtable
      for more information, visit <https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility>
   ╭▸ $SRC_DIR/core/src/cmp.rs:961:20
   │
   ├ note: the trait is not dyn compatible because it uses `Self` as a type parameter
   ⸬  $SRC_DIR/core/src/cmp.rs:338:14
   │
   ╰ note: the trait is not dyn compatible because it uses `Self` as a type parameter
help: consider using an opaque type instead
   ╭╴
11 │ fn ord_prefer_dot(s: String) -> impl Ord {
   ╰╴                                ++++
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn multiple_origin() {
    // tests/ui/binop/binary-op-not-allowed-issue-125631.rs
    let source_0 = r#"use std::io::{Error, ErrorKind};
use std::thread;

struct T1;
struct T2;

fn main() {
    (Error::new(ErrorKind::Other, "1"), T1, 1) == (Error::new(ErrorKind::Other, "1"), T1, 2);
    //~^ERROR binary operation `==` cannot be applied to type
    (Error::new(ErrorKind::Other, "2"), thread::current())
        == (Error::new(ErrorKind::Other, "2"), thread::current());
    //~^ERROR binary operation `==` cannot be applied to type
    (Error::new(ErrorKind::Other, "4"), thread::current(), T1, T2)
        == (Error::new(ErrorKind::Other, "4"), thread::current(), T1, T2);
    //~^ERROR binary operation `==` cannot be applied to type
}
"#;
    let title_0 =
        "the foreign item types don't implement required traits for this operation to be valid";

    let input = &[
        Level::ERROR
            .primary_title(
                "binary operation `==` cannot be applied to type `(std::io::Error, Thread)`",
            )
            .id("E0369")
            .element(
                Snippet::source(source_0)
                    .path("$DIR/binary-op-not-allowed-issue-125631.rs")
                    .annotation(
                        AnnotationKind::Context
                            .span(246..300)
                            .label("(std::io::Error, Thread)"),
                    )
                    .annotation(
                        AnnotationKind::Context
                            .span(312..366)
                            .label("(std::io::Error, Thread)"),
                    )
                    .annotation(AnnotationKind::Primary.span(309..311)),
            ),
        Level::NOTE
            .secondary_title(title_0)
            .element(
                Origin::path("$SRC_DIR/std/src/io/error.rs")
                    .line(65)
                    .char_column(0),
            )
            .element(Padding)
            .element(Level::NOTE.message("not implement `PartialEq`")),
        Group::with_level(Level::NOTE)
            .element(
                Origin::path("$SRC_DIR/std/src/thread/mod.rs")
                    .line(1439)
                    .char_column(0),
            )
            .element(Padding)
            .element(Level::NOTE.message("not implement `PartialEq`")),
    ];
    let expected_ascii = str![[r#"
error[E0369]: binary operation `==` cannot be applied to type `(std::io::Error, Thread)`
   ╭▸ $DIR/binary-op-not-allowed-issue-125631.rs:11:9
   │
10 │     (Error::new(ErrorKind::Other, "2"), thread::current())
   │     ────────────────────────────────────────────────────── (std::io::Error, Thread)
11 │         == (Error::new(ErrorKind::Other, "2"), thread::current());
   │         ━━ ────────────────────────────────────────────────────── (std::io::Error, Thread)
   ╰╴
note: the foreign item types don't implement required traits for this operation to be valid
   ╭▸ $SRC_DIR/std/src/io/error.rs:65:0
   │
   ╰ note: not implement `PartialEq`
   ╭▸ $SRC_DIR/std/src/thread/mod.rs:1439:0
   │
   ╰ note: not implement `PartialEq`
"#]];
    let renderer = Renderer::plain().decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0369]: binary operation `==` cannot be applied to type `(std::io::Error, Thread)`
   ╭▸ $DIR/binary-op-not-allowed-issue-125631.rs:11:9
   │
10 │     (Error::new(ErrorKind::Other, "2"), thread::current())
   │     ────────────────────────────────────────────────────── (std::io::Error, Thread)
11 │         == (Error::new(ErrorKind::Other, "2"), thread::current());
   │         ━━ ────────────────────────────────────────────────────── (std::io::Error, Thread)
   ╰╴
note: the foreign item types don't implement required traits for this operation to be valid
   ╭▸ $SRC_DIR/std/src/io/error.rs:65:0
   │
   ╰ note: not implement `PartialEq`
   ╭▸ $SRC_DIR/std/src/thread/mod.rs:1439:0
   │
   ╰ note: not implement `PartialEq`
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn const_generics_issue_82656() {
    // tests/ui/const-generics/issues/issue-82956.rs
    let source = r#"#![feature(generic_const_exprs)]
#![allow(incomplete_features)]

pub struct ConstCheck<const CHECK: bool>;

pub trait True {}
impl True for ConstCheck<true> {}

pub trait OrdesDec {
    type Newlen;
    type Output;

    fn pop(self) -> (Self::Newlen, Self::Output);
}

impl<T, const N: usize> OrdesDec for [T; N]
where
    ConstCheck<{N > 1}>: True,
    [T; N - 1]: Sized,
{
    type Newlen = [T; N - 1];
    type Output = T;

    fn pop(self) -> (Self::Newlen, Self::Output) {
        let mut iter = IntoIter::new(self);
        //~^ ERROR: failed to resolve: use of undeclared type `IntoIter`
        let end = iter.next_back().unwrap();
        let new = [(); N - 1].map(move |()| iter.next().unwrap());
        (new, end)
    }
}

fn main() {}
"#;

    let input = &[
        Level::ERROR
            .primary_title("failed to resolve: use of undeclared type `IntoIter`")
            .id("E0433")
            .element(
                Snippet::source(source)
                    .path("$DIR/issue-82956.rs")
                    .annotation(
                        AnnotationKind::Primary
                            .span(502..510)
                            .label("use of undeclared type `IntoIter`"),
                    ),
            ),
        Level::HELP
            .secondary_title("consider importing one of these structs")
            .element(
                Snippet::source(source)
                    .path("$DIR/issue-82956.rs")
                    .patch(Patch::new(65..65, "use std::array::IntoIter;\n\n")),
            )
            .element(
                Snippet::source(source)
                    .path("$DIR/issue-82956.rs")
                    .patch(Patch::new(
                        65..65,
                        "use std::collections::binary_heap::IntoIter;\n\n",
                    )),
            )
            .element(
                Snippet::source(source)
                    .path("$DIR/issue-82956.rs")
                    .patch(Patch::new(
                        65..65,
                        "use std::collections::btree_map::IntoIter;\n\n",
                    )),
            )
            .element(
                Snippet::source(source)
                    .path("$DIR/issue-82956.rs")
                    .patch(Patch::new(
                        65..65,
                        "use std::collections::btree_set::IntoIter;\n\n",
                    )),
            )
            .element(Level::NOTE.no_name().message("and 9 other candidates")),
    ];

    let expected_ascii = str![[r#"
error[E0433]: failed to resolve: use of undeclared type `IntoIter`
  --> $DIR/issue-82956.rs:25:24
   |
25 |         let mut iter = IntoIter::new(self);
   |                        ^^^^^^^^ use of undeclared type `IntoIter`
   |
help: consider importing one of these structs
   |
 4 + use std::array::IntoIter;
   |
 4 + use std::collections::binary_heap::IntoIter;
   |
 4 + use std::collections::btree_map::IntoIter;
   |
 4 + use std::collections::btree_set::IntoIter;
   |
   = and 9 other candidates
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0433]: failed to resolve: use of undeclared type `IntoIter`
   ╭▸ $DIR/issue-82956.rs:25:24
   │
25 │         let mut iter = IntoIter::new(self);
   │                        ━━━━━━━━ use of undeclared type `IntoIter`
   ╰╴
help: consider importing one of these structs
   ╭╴
 4 + use std::array::IntoIter;
   ├╴
 4 + use std::collections::binary_heap::IntoIter;
   ├╴
 4 + use std::collections::btree_map::IntoIter;
   ├╴
 4 + use std::collections::btree_set::IntoIter;
   │
   ╰ and 9 other candidates
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn multi_suggestion() {
    // tests/ui/suggestions/multi-suggestion.rs
    let source = r#"//@ revisions: ascii unicode
//@[unicode] compile-flags: -Zunstable-options --error-format=human-unicode

#![allow(dead_code)]
struct U <T> {
    wtf: Option<Box<U<T>>>,
    x: T,
}
fn main() {
    U {
        wtf: Some(Box(U { //[ascii]~ ERROR cannot initialize a tuple struct which contains private fields
            wtf: None,
            x: (),
        })),
        x: ()
    };
    let _ = std::collections::HashMap();
    //[ascii]~^ ERROR expected function, tuple struct or tuple variant, found struct `std::collections::HashMap`
    let _ = std::collections::HashMap {};
    //[ascii]~^ ERROR cannot construct `HashMap<_, _, _>` with struct literal syntax due to private fields
    let _ = Box {}; //[ascii]~ ERROR cannot construct `Box<_, _>` with struct literal syntax due to private fields
}
"#;
    let title_0 = "expected function, tuple struct or tuple variant, found struct `std::collections::HashMap`";

    let input = &[
        Level::ERROR
            .primary_title(title_0)
            .id("E0423")
            .element(
                Snippet::source(source)
                    .path("$DIR/multi-suggestion.rs")
                    .annotation(AnnotationKind::Primary.span(396..423)),
            )
            .element(
                Origin::path("$SRC_DIR/std/src/collections/hash/map.rs")
                    .line(242)
                    .char_column(0),
            )
            .element(Padding)
            .element(Level::NOTE.message("`std::collections::HashMap` defined here"))
            .element(Padding),
        Level::HELP
            .secondary_title(
                "you might have meant to use an associated function to build this type",
            )
            .element(
                Snippet::source(source)
                    .path("$DIR/multi-suggestion.rs")
                    .patch(Patch::new(421..423, "::new()")),
            )
            .element(
                Snippet::source(source)
                    .path("$DIR/multi-suggestion.rs")
                    .patch(Patch::new(421..423, "::with_capacity(_)")),
            )
            .element(
                Snippet::source(source)
                    .path("$DIR/multi-suggestion.rs")
                    .patch(Patch::new(421..423, "::with_hasher(_)")),
            )
            .element(
                Snippet::source(source)
                    .path("$DIR/multi-suggestion.rs")
                    .patch(Patch::new(421..423, "::with_capacity_and_hasher(_, _)")),
            ),
        Level::HELP
            .secondary_title("consider using the `Default` trait")
            .element(
                Snippet::source(source)
                    .path("$DIR/multi-suggestion.rs")
                    .patch(Patch::new(396..396, "<"))
                    .patch(Patch::new(
                        421..423,
                        " as std::default::Default>::default()",
                    )),
            ),
    ];

    let expected_ascii = str![[r#"
error[E0423]: expected function, tuple struct or tuple variant, found struct `std::collections::HashMap`
  --> $DIR/multi-suggestion.rs:17:13
   |
17 |     let _ = std::collections::HashMap();
   |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^
  ::: $SRC_DIR/std/src/collections/hash/map.rs:242:0
   |
   = note: `std::collections::HashMap` defined here
   |
help: you might have meant to use an associated function to build this type
   |
17 |     let _ = std::collections::HashMap::new();
   |                                      +++++
17 -     let _ = std::collections::HashMap();
17 +     let _ = std::collections::HashMap::with_capacity(_);
   |
17 -     let _ = std::collections::HashMap();
17 +     let _ = std::collections::HashMap::with_hasher(_);
   |
17 -     let _ = std::collections::HashMap();
17 +     let _ = std::collections::HashMap::with_capacity_and_hasher(_, _);
   |
help: consider using the `Default` trait
   |
17 |     let _ = <std::collections::HashMap as std::default::Default>::default();
   |             +                          ++++++++++++++++++++++++++++++++++
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0423]: expected function, tuple struct or tuple variant, found struct `std::collections::HashMap`
   ╭▸ $DIR/multi-suggestion.rs:17:13
   │
17 │     let _ = std::collections::HashMap();
   │             ━━━━━━━━━━━━━━━━━━━━━━━━━━━
   ⸬  $SRC_DIR/std/src/collections/hash/map.rs:242:0
   │
   ├ note: `std::collections::HashMap` defined here
   ╰╴
help: you might have meant to use an associated function to build this type
   ╭╴
17 │     let _ = std::collections::HashMap::new();
   ├╴                                     +++++
17 -     let _ = std::collections::HashMap();
17 +     let _ = std::collections::HashMap::with_capacity(_);
   ├╴
17 -     let _ = std::collections::HashMap();
17 +     let _ = std::collections::HashMap::with_hasher(_);
   ├╴
17 -     let _ = std::collections::HashMap();
17 +     let _ = std::collections::HashMap::with_capacity_and_hasher(_, _);
   ╰╴
help: consider using the `Default` trait
   ╭╴
17 │     let _ = <std::collections::HashMap as std::default::Default>::default();
   ╰╴            +                          ++++++++++++++++++++++++++++++++++
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn suggest_box_new() {
    // tests/ui/privacy/suggest-box-new.rs
    let source = r#"//@ revisions: ascii unicode
//@[unicode] compile-flags: -Zunstable-options --error-format=human-unicode

#![allow(dead_code)]
struct U <T> {
    wtf: Option<Box<U<T>>>,
    x: T,
}
fn main() {
    U {
        wtf: Some(Box(U {
            wtf: None,
            x: (),
        })),
        x: ()
    };
    let _ = std::collections::HashMap();
    //[ascii]~^ ERROR expected function, tuple struct or tuple variant, found struct `std::collections::HashMap`
    let _ = std::collections::HashMap {};
    //[ascii]~^ ERROR cannot construct `HashMap<_, _, _>` with struct literal syntax due to private fields
    let _ = Box {}; //[ascii]~ ERROR cannot construct `Box<_, _>` with struct literal syntax due to private fields
}
"#;

    let input = &[
        Level::ERROR
            .primary_title("cannot initialize a tuple struct which contains private fields")
            .id("E0423")
            .element(
                Snippet::source(source)
                    .path("$DIR/suggest-box-new.rs")
                    .annotation(AnnotationKind::Primary.span(220..223)),
            ),
        Level::NOTE
            .secondary_title("constructor is not visible here due to private fields")
            .element(
                Origin::path("$SRC_DIR/alloc/src/boxed.rs")
                    .line(234)
                    .char_column(2),
            )
            .element(Padding)
            .element(Level::NOTE.message("private field"))
            .element(Padding)
            .element(Level::NOTE.message("private field")),
        Level::HELP
            .secondary_title(
                "you might have meant to use an associated function to build this type",
            )
            .element(
                Snippet::source(source)
                    .path("$DIR/suggest-box-new.rs")
                    .patch(Patch::new(223..280, "::new(_)")),
            )
            .element(
                Snippet::source(source)
                    .path("$DIR/suggest-box-new.rs")
                    .patch(Patch::new(223..280, "::new_uninit()")),
            )
            .element(
                Snippet::source(source)
                    .path("$DIR/suggest-box-new.rs")
                    .patch(Patch::new(223..280, "::new_zeroed()")),
            )
            .element(
                Snippet::source(source)
                    .path("$DIR/suggest-box-new.rs")
                    .patch(Patch::new(223..280, "::new_in(_, _)")),
            )
            .element(Level::NOTE.no_name().message("and 12 other candidates")),
        Level::HELP
            .secondary_title("consider using the `Default` trait")
            .element(
                Snippet::source(source)
                    .path("$DIR/suggest-box-new.rs")
                    .patch(Patch::new(220..220, "<"))
                    .patch(Patch::new(
                        223..280,
                        " as std::default::Default>::default()",
                    )),
            ),
    ];

    let expected_ascii = str![[r#"
error[E0423]: cannot initialize a tuple struct which contains private fields
  --> $DIR/suggest-box-new.rs:11:19
   |
11 |         wtf: Some(Box(U {
   |                   ^^^
   |
note: constructor is not visible here due to private fields
  --> $SRC_DIR/alloc/src/boxed.rs:234:2
   |
   = note: private field
   |
   = note: private field
help: you might have meant to use an associated function to build this type
   |
11 -         wtf: Some(Box(U {
12 -             wtf: None,
13 -             x: (),
14 -         })),
11 +         wtf: Some(Box::new(_)),
   |
11 -         wtf: Some(Box(U {
12 -             wtf: None,
13 -             x: (),
14 -         })),
11 +         wtf: Some(Box::new_uninit()),
   |
11 -         wtf: Some(Box(U {
12 -             wtf: None,
13 -             x: (),
14 -         })),
11 +         wtf: Some(Box::new_zeroed()),
   |
11 -         wtf: Some(Box(U {
12 -             wtf: None,
13 -             x: (),
14 -         })),
11 +         wtf: Some(Box::new_in(_, _)),
   |
   = and 12 other candidates
help: consider using the `Default` trait
   |
11 -         wtf: Some(Box(U {
12 -             wtf: None,
13 -             x: (),
14 -         })),
11 +         wtf: Some(<Box as std::default::Default>::default()),
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0423]: cannot initialize a tuple struct which contains private fields
   ╭▸ $DIR/suggest-box-new.rs:11:19
   │
11 │         wtf: Some(Box(U {
   │                   ━━━
   ╰╴
note: constructor is not visible here due to private fields
   ╭▸ $SRC_DIR/alloc/src/boxed.rs:234:2
   │
   ├ note: private field
   │
   ╰ note: private field
help: you might have meant to use an associated function to build this type
   ╭╴
11 -         wtf: Some(Box(U {
12 -             wtf: None,
13 -             x: (),
14 -         })),
11 +         wtf: Some(Box::new(_)),
   ├╴
11 -         wtf: Some(Box(U {
12 -             wtf: None,
13 -             x: (),
14 -         })),
11 +         wtf: Some(Box::new_uninit()),
   ├╴
11 -         wtf: Some(Box(U {
12 -             wtf: None,
13 -             x: (),
14 -         })),
11 +         wtf: Some(Box::new_zeroed()),
   ├╴
11 -         wtf: Some(Box(U {
12 -             wtf: None,
13 -             x: (),
14 -         })),
11 +         wtf: Some(Box::new_in(_, _)),
   │
   ╰ and 12 other candidates
help: consider using the `Default` trait
   ╭╴
11 -         wtf: Some(Box(U {
12 -             wtf: None,
13 -             x: (),
14 -         })),
11 +         wtf: Some(<Box as std::default::Default>::default()),
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn too_many_field_suggestions() {
    // tests/ui/suggestions/too-many-field-suggestions.rs
    let source = r#"struct Thing {
    a0: Foo,
    a1: Foo,
    a2: Foo,
    a3: Foo,
    a4: Foo,
    a5: Foo,
    a6: Foo,
    a7: Foo,
    a8: Foo,
    a9: Foo,
}

struct Foo {
    field: Field,
}

struct Field;

impl Foo {
    fn bar(&self) {}
}

fn bar(t: Thing) {
    t.bar();
    t.field;
}

fn main() {}
"#;

    let input = &[
        Level::ERROR
            .primary_title("no method named `bar` found for struct `Thing` in the current scope")
            .id("E0599")
            .element(
                Snippet::source(source)
                    .path("$DIR/too-many-field-suggestions.rs")
                    .annotation(
                        AnnotationKind::Primary
                            .span(257..260)
                            .label("method not found in `Thing`"),
                    )
                    .annotation(
                        AnnotationKind::Context
                            .span(0..12)
                            .label("method `bar` not found for this struct"),
                    ),
            ),
        Level::HELP
            .secondary_title("some of the expressions' fields have a method of the same name")
            .element(
                Snippet::source(source)
                    .path("$DIR/too-many-field-suggestions.rs")
                    .patch(Patch::new(257..257, "a0.")),
            )
            .element(
                Snippet::source(source)
                    .path("$DIR/too-many-field-suggestions.rs")
                    .patch(Patch::new(257..257, "a1.")),
            )
            .element(
                Snippet::source(source)
                    .path("$DIR/too-many-field-suggestions.rs")
                    .patch(Patch::new(257..257, "a2.")),
            )
            .element(
                Snippet::source(source)
                    .path("$DIR/too-many-field-suggestions.rs")
                    .patch(Patch::new(257..257, "a3.")),
            )
            .element(Level::NOTE.no_name().message("and 6 other candidates")),
    ];

    let expected_ascii = str![[r#"
error[E0599]: no method named `bar` found for struct `Thing` in the current scope
  --> $DIR/too-many-field-suggestions.rs:25:7
   |
 1 | struct Thing {
   | ------------ method `bar` not found for this struct
...
25 |     t.bar();
   |       ^^^ method not found in `Thing`
   |
help: some of the expressions' fields have a method of the same name
   |
25 |     t.a0.bar();
   |       +++
25 |     t.a1.bar();
   |       +++
25 |     t.a2.bar();
   |       +++
25 |     t.a3.bar();
   |       +++
   = and 6 other candidates
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0599]: no method named `bar` found for struct `Thing` in the current scope
   ╭▸ $DIR/too-many-field-suggestions.rs:25:7
   │
 1 │ struct Thing {
   │ ──────────── method `bar` not found for this struct
   ┆
25 │     t.bar();
   │       ━━━ method not found in `Thing`
   ╰╴
help: some of the expressions' fields have a method of the same name
   ╭╴
25 │     t.a0.bar();
   ├╴      +++
25 │     t.a1.bar();
   ├╴      +++
25 │     t.a2.bar();
   ├╴      +++
25 │     t.a3.bar();
   │       +++
   ╰ and 6 other candidates
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn invalid_arguments_unterminated() {
    // tests/ui/check-cfg/invalid-arguments.unterminated.rs
    let input = &[Level::ERROR
        .primary_title("invalid `--check-cfg` argument: `cfg(`")
        .element(
            Level::NOTE
                .message(r#"expected `cfg(name, values("value1", "value2", ... "valueN"))`"#),
        )
        .element(Level::NOTE.message(
            "visit <https://doc.rust-lang.org/nightly/rustc/check-cfg.html> for more details",
        ))];
    let expected_ascii = str![[r#"
error: invalid `--check-cfg` argument: `cfg(`
  |
  = note: expected `cfg(name, values("value1", "value2", ... "valueN"))`
  = note: visit <https://doc.rust-lang.org/nightly/rustc/check-cfg.html> for more details
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: invalid `--check-cfg` argument: `cfg(`
  │
  ├ note: expected `cfg(name, values("value1", "value2", ... "valueN"))`
  ╰ note: visit <https://doc.rust-lang.org/nightly/rustc/check-cfg.html> for more details
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn timeout() {
    // tests/ui/consts/timeout.rs
    let source = r#"//! This test checks that external macros don't hide
//! the const eval timeout lint and then subsequently
//! ICE.

//@ compile-flags: --crate-type=lib -Ztiny-const-eval-limit

static ROOK_ATTACKS_TABLE: () = {
    0_u64.count_ones();
    0_u64.count_ones();
    0_u64.count_ones();
    0_u64.count_ones();
    0_u64.count_ones();
    0_u64.count_ones();
    0_u64.count_ones();
    0_u64.count_ones();
    0_u64.count_ones();
    0_u64.count_ones();
    0_u64.count_ones();
    0_u64.count_ones();
    0_u64.count_ones();
    0_u64.count_ones();
    0_u64.count_ones();
    0_u64.count_ones();
};

//~? ERROR constant evaluation is taking a long time
"#;

    let title_0 =
        "this lint makes sure the compiler doesn't get stuck due to infinite loops in const eval.
If your compilation actually takes a long time, you can safely allow the lint.";
    let title_1 = "this error originates in the macro `uint_impl` (in Nightly builds, run with -Z macro-backtrace for more info)";

    let input = &[
        Level::ERROR
            .primary_title("constant evaluation is taking a long time")
            .element(
                Origin::path("$SRC_DIR/core/src/num/mod.rs")
                    .line(1151)
                    .char_column(4),
            )
            .element(Level::NOTE.message(title_0)),
        Level::HELP
            .secondary_title("the constant being evaluated")
            .element(
                Snippet::source(source)
                    .path("$DIR/timeout.rs")
                    .annotation(AnnotationKind::Primary.span(178..207)),
            )
            .element(Level::NOTE.message("`#[deny(long_running_const_eval)]` on by default"))
            .element(Level::NOTE.message(title_1)),
    ];
    let expected_ascii = str![[r#"
error: constant evaluation is taking a long time
 --> $SRC_DIR/core/src/num/mod.rs:1151:4
  = note: this lint makes sure the compiler doesn't get stuck due to infinite loops in const eval.
          If your compilation actually takes a long time, you can safely allow the lint.
help: the constant being evaluated
 --> $DIR/timeout.rs:7:1
  |
7 | static ROOK_ATTACKS_TABLE: () = {
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  = note: `#[deny(long_running_const_eval)]` on by default
  = note: this error originates in the macro `uint_impl` (in Nightly builds, run with -Z macro-backtrace for more info)
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: constant evaluation is taking a long time
  ╭▸ $SRC_DIR/core/src/num/mod.rs:1151:4
  ╰ note: this lint makes sure the compiler doesn't get stuck due to infinite loops in const eval.
          If your compilation actually takes a long time, you can safely allow the lint.
help: the constant being evaluated
  ╭▸ $DIR/timeout.rs:7:1
  │
7 │ static ROOK_ATTACKS_TABLE: () = {
  │ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  ├ note: `#[deny(long_running_const_eval)]` on by default
  ╰ note: this error originates in the macro `uint_impl` (in Nightly builds, run with -Z macro-backtrace for more info)
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn emitter_overflow_bad_whitespace() {
    // tests/ui/errors/emitter-overflow-bad-whitespace.rs
    let source = r#"                                         fn main() {              return;              }
"#;
    let title_0 = "Unicode character ' ' (No-Break Space) looks like ' ' (Space), but it is not";

    let report = &[
        Group::with_title(Level::ERROR.primary_title("unknown start of token: \u{a0}")).element(
            Snippet::source(source)
                .path("$DIR/emitter-overflow-bad-whitespace.rs")
                .line_start(10)
                .annotation(AnnotationKind::Primary.span(0..2)),
        ),
        Group::with_title(Level::HELP.secondary_title(title_0)).element(
            Snippet::source(source)
                .path("$DIR/emitter-overflow-bad-whitespace.rs")
                .line_start(10)
                .patch(Patch::new(0..2, " ")),
        ),
    ];
    let expected_ascii = str![[r#"
error: unknown start of token:  
  --> $DIR/emitter-overflow-bad-whitespace.rs:10:1
   |
10 |     ...
   | ^
   |
help: Unicode character ' ' (No-Break Space) looks like ' ' (Space), but it is not
   |
10 |                                          fn main() {              return;              }
   | +
"#]];
    let renderer_ascii = Renderer::plain().term_width(1);
    assert_data_eq!(renderer_ascii.render(report), expected_ascii);

    let expected_unicode = str![[r#"
error: unknown start of token:  
   ╭▸ $DIR/emitter-overflow-bad-whitespace.rs:10:1
   │
10 │       …
   │ ━
   ╰╴
help: Unicode character ' ' (No-Break Space) looks like ' ' (Space), but it is not
   ╭╴
10 │                                          fn main() {              return;              }
   ╰╴+
"#]];
    let renderer_unicode = renderer_ascii.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer_unicode.render(report), expected_unicode);
}

#[test]
fn issue_109854() {
    // tests/ui/suggestions/issue-109854.rs
    let source_0 = r##"    String::with_capacity(
    //~^ ERROR this function takes 1 argument but 3 arguments were supplied
    generate_setter,
    r#"
pub(crate) struct Person<T: Clone> {}
"#,
     r#""#,
"##;
    let source_1 = r#"    generate_setter,
"#;
    let title_0 = "expected type `[22;1;35musize[22;39m`
found fn item `[22;1;35mfn() {generate_setter}[22;39m`";
    let source_2 = r##"    generate_setter,
    r#"
pub(crate) struct Person<T: Clone> {}
"#,
     r#""#,
"##;

    let report = &[
        Level::ERROR
            .primary_title("this function takes 1 argument but 3 arguments were supplied")
            .id("E0061")
            .element(
                Snippet::source(source_0)
                    .path("$DIR/issue-109854.rs")
                    .line_start(2)
                    .annotation(AnnotationKind::Primary.span(4..25))
                    .annotation(
                        AnnotationKind::Context
                            .span(128..172)
                            .label("unexpected argument #2 of type `&'static str`"),
                    )
                    .annotation(
                        AnnotationKind::Context
                            .span(179..184)
                            .label("unexpected argument #3 of type `&'static str`"),
                    ),
            ),
        Level::NOTE
            .secondary_title("expected `usize`, found fn item")
            .element(
                Snippet::source(source_1)
                    .path("$DIR/issue-109854.rs")
                    .line_start(4)
                    .annotation(AnnotationKind::Primary.span(4..19)),
            )
            .element(Level::NOTE.message(title_0)),
        Level::NOTE
            .secondary_title("associated function defined here")
            .element(
                Origin::path("$SRC_DIR/alloc/src/string.rs")
                    .line(480)
                    .char_column(11),
            ),
        Level::HELP
            .secondary_title("remove the extra arguments")
            .element(
                Snippet::source(source_2)
                    .path("$DIR/issue-109854.rs")
                    .line_start(4)
                    .patch(Patch::new(4..19, "/* usize */"))
                    .patch(Patch::new(19..69, ""))
                    .patch(Patch::new(69..81, "")),
            ),
    ];
    let expected_ascii = str![[r##"
error[E0061]: this function takes 1 argument but 3 arguments were supplied
 --> $DIR/issue-109854.rs:2:5
  |
2 |       String::with_capacity(
  |       ^^^^^^^^^^^^^^^^^^^^^
...
5 | /     r#"
6 | | pub(crate) struct Person<T: Clone> {}
7 | | "#,
  | |__- unexpected argument #2 of type `&'static str`
8 |        r#""#,
  |        ----- unexpected argument #3 of type `&'static str`
  |
note: expected `usize`, found fn item
 --> $DIR/issue-109854.rs:4:5
  |
4 |     generate_setter,
  |     ^^^^^^^^^^^^^^^
  = note: expected type `[22;1;35musize[22;39m`
          found fn item `[22;1;35mfn() {generate_setter}[22;39m`
note: associated function defined here
 --> $SRC_DIR/alloc/src/string.rs:480:11
help: remove the extra arguments
  |
4 -     generate_setter,
5 -     r#"
6 - pub(crate) struct Person<T: Clone> {}
7 - "#,
8 -      r#""#,
4 +     /* usize */,
  |
"##]];
    let renderer_ascii = Renderer::plain();
    assert_data_eq!(renderer_ascii.render(report), expected_ascii);

    let expected_unicode = str![[r##"
error[E0061]: this function takes 1 argument but 3 arguments were supplied
  ╭▸ $DIR/issue-109854.rs:2:5
  │
2 │       String::with_capacity(
  │       ━━━━━━━━━━━━━━━━━━━━━
  ┆
5 │ ┌     r#"
6 │ │ pub(crate) struct Person<T: Clone> {}
7 │ │ "#,
  │ └──┘ unexpected argument #2 of type `&'static str`
8 │        r#""#,
  │        ───── unexpected argument #3 of type `&'static str`
  ╰╴
note: expected `usize`, found fn item
  ╭▸ $DIR/issue-109854.rs:4:5
  │
4 │     generate_setter,
  │     ━━━━━━━━━━━━━━━
  ╰ note: expected type `[22;1;35musize[22;39m`
          found fn item `[22;1;35mfn() {generate_setter}[22;39m`
note: associated function defined here
  ─▸ $SRC_DIR/alloc/src/string.rs:480:11
help: remove the extra arguments
  ╭╴
4 -     generate_setter,
5 -     r#"
6 - pub(crate) struct Person<T: Clone> {}
7 - "#,
8 -      r#""#,
4 +     /* usize */,
  ╰╴
"##]];
    let renderer_unicode = renderer_ascii.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer_unicode.render(report), expected_unicode);
}

#[test]
fn match_same_arms() {
    // src/tools/clippy/tests/ui/match_same_arms.rs
    let source = r#"        2 => 'b',
        3 => 'b',
        _ => 'b',
"#;

    let report = &[
        Level::ERROR
            .primary_title("these match arms have identical bodies")
            .element(
                Snippet::source(source)
                    .path("tests/ui/match_same_arms.rs")
                    .line_start(20)
                    .annotation(AnnotationKind::Primary.span(8..16))
                    .annotation(AnnotationKind::Primary.span(26..34))
                    .annotation(
                        AnnotationKind::Primary
                            .span(44..52)
                            .label("the wildcard arm"),
                    ),
            )
            .element(
                Level::HELP
                    .message("if this is unintentional make the arms return different values"),
            ),
        Level::HELP
            .secondary_title("otherwise remove the non-wildcard arms")
            .element(
                Snippet::source(source)
                    .path("tests/ui/match_same_arms.rs")
                    .line_start(20)
                    .patch(Patch::new(8..26, ""))
                    .patch(Patch::new(26..44, "")),
            ),
    ];
    let expected_ascii = str![[r#"
error: these match arms have identical bodies
  --> tests/ui/match_same_arms.rs:20:9
   |
20 |         2 => 'b',
   |         ^^^^^^^^
21 |         3 => 'b',
   |         ^^^^^^^^
22 |         _ => 'b',
   |         ^^^^^^^^ the wildcard arm
   |
   = help: if this is unintentional make the arms return different values
help: otherwise remove the non-wildcard arms
   |
20 -         2 => 'b',
21 -         3 => 'b',
   |
"#]];
    let renderer_ascii = Renderer::plain();
    assert_data_eq!(renderer_ascii.render(report), expected_ascii);

    let expected_unicode = str![[r#"
error: these match arms have identical bodies
   ╭▸ tests/ui/match_same_arms.rs:20:9
   │
20 │         2 => 'b',
   │         ━━━━━━━━
21 │         3 => 'b',
   │         ━━━━━━━━
22 │         _ => 'b',
   │         ━━━━━━━━ the wildcard arm
   │
   ╰ help: if this is unintentional make the arms return different values
help: otherwise remove the non-wildcard arms
   ╭╴
20 -         2 => 'b',
21 -         3 => 'b',
   ╰╴
"#]];
    let renderer_unicode = renderer_ascii.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer_unicode.render(report), expected_unicode);
}
