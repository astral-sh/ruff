//! These tests have been adapted from [Rust's parser tests][parser-tests].
//!
//! [parser-tests]: https://github.com/rust-lang/rust/blob/894f7a4ba6554d3797404bbf550d9919df060b97/compiler/rustc_parse/src/parser/tests.rs

use ruff_annotate_snippets::{Level, Renderer, Snippet};

use snapbox::{assert_data_eq, str};

#[test]
fn ends_on_col0() {
    let source = r#"
fn foo() {
}
"#;
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(10..13).label("test")),
    );

    let expected = str![[r#"
error: foo
 --> test.rs:2:10
  |
2 |   fn foo() {
  |  __________^
3 | | }
  | |_^ test
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}
#[test]
fn ends_on_col2() {
    let source = r#"
fn foo() {


  }
"#;
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(10..17).label("test")),
    );

    let expected = str![[r#"
error: foo
 --> test.rs:2:10
  |
2 |   fn foo() {
  |  __________^
3 | |
4 | |
5 | |   }
  | |___^ test
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
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
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(14..32).label("`X` is a good letter"))
            .annotation(
                Level::Warning
                    .span(17..35)
                    .label("`Y` is a good letter too"),
            ),
    );

    let expected = str![[r#"
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
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}
#[test]
fn nested() {
    let source = r#"
fn foo() {
  X0 Y0
  Y1 X1
}
"#;
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(14..27).label("`X` is a good letter"))
            .annotation(
                Level::Warning
                    .span(17..24)
                    .label("`Y` is a good letter too"),
            ),
    );

    let expected = str![[r#"
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
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
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
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(17..38).label("`X` is a good letter"))
            .annotation(
                Level::Warning
                    .span(31..49)
                    .label("`Y` is a good letter too"),
            ),
    );

    let expected = str![[r#"
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
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
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
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(14..38).label("`X` is a good letter"))
            .annotation(
                Level::Warning
                    .span(17..41)
                    .label("`Y` is a good letter too"),
            )
            .annotation(Level::Warning.span(20..44).label("`Z` label")),
    );

    let expected = str![[r#"
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
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
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
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(14..38).label("`X` is a good letter"))
            .annotation(
                Level::Warning
                    .span(14..38)
                    .label("`Y` is a good letter too"),
            )
            .annotation(Level::Warning.span(14..38).label("`Z` label")),
    );

    // This should have a `^` but we currently don't support the idea of a
    // "primary" annotation, which would solve this
    let expected = str![[r#"
error: foo
 --> test.rs:3:3
  |
3 | /   X0 Y0 Z0
4 | |   X1 Y1 Z1
5 | |   X2 Y2 Z2
  | |    -
  | |____|
  |      `X` is a good letter
  |      `Y` is a good letter too
  |      `Z` label
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
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
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(17..27).label("`X` is a good letter"))
            .annotation(
                Level::Warning
                    .span(28..44)
                    .label("`Y` is a good letter too"),
            )
            .annotation(Level::Warning.span(36..52).label("`Z`")),
    );

    let expected = str![[r#"
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
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
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
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(14..27).label("`X` is a good letter"))
            .annotation(
                Level::Warning
                    .span(39..55)
                    .label("`Y` is a good letter too"),
            ),
    );

    let expected = str![[r#"
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
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
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
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(17..27).label("`X` is a good letter"))
            .annotation(
                Level::Warning
                    .span(31..55)
                    .label("`Y` is a good letter too"),
            ),
    );

    let expected = str![[r#"
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
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}
#[test]
fn multiple_labels_primary_without_message() {
    let source = r#"
fn foo() {
  a { b { c } d }
}
"#;
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(18..25).label(""))
            .annotation(Level::Warning.span(14..27).label("`a` is a good letter"))
            .annotation(Level::Warning.span(22..23).label("")),
    );

    let expected = str![[r#"
error: foo
 --> test.rs:3:7
  |
3 |   a { b { c } d }
  |   ----^^^^-^^-- `a` is a good letter
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}
#[test]
fn multiple_labels_secondary_without_message() {
    let source = r#"
fn foo() {
  a { b { c } d }
}
"#;
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(14..27).label("`a` is a good letter"))
            .annotation(Level::Warning.span(18..25).label("")),
    );

    let expected = str![[r#"
error: foo
 --> test.rs:3:3
  |
3 |   a { b { c } d }
  |   ^^^^-------^^ `a` is a good letter
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}
#[test]
fn multiple_labels_primary_without_message_2() {
    let source = r#"
fn foo() {
  a { b { c } d }
}
"#;
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(18..25).label("`b` is a good letter"))
            .annotation(Level::Warning.span(14..27).label(""))
            .annotation(Level::Warning.span(22..23).label("")),
    );

    let expected = str![[r#"
error: foo
 --> test.rs:3:7
  |
3 |   a { b { c } d }
  |   ----^^^^-^^--
  |       |
  |       `b` is a good letter
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}
#[test]
fn multiple_labels_secondary_without_message_2() {
    let source = r#"
fn foo() {
  a { b { c } d }
}
"#;
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(14..27).label(""))
            .annotation(Level::Warning.span(18..25).label("`b` is a good letter")),
    );

    let expected = str![[r#"
error: foo
 --> test.rs:3:3
  |
3 |   a { b { c } d }
  |   ^^^^-------^^
  |       |
  |       `b` is a good letter
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}
#[test]
fn multiple_labels_secondary_without_message_3() {
    let source = r#"
fn foo() {
  a  bc  d
}
"#;
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(14..18).label("`a` is a good letter"))
            .annotation(Level::Warning.span(18..22).label("")),
    );

    let expected = str![[r#"
error: foo
 --> test.rs:3:3
  |
3 |   a  bc  d
  |   ^^^^----
  |   |
  |   `a` is a good letter
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}
#[test]
fn multiple_labels_without_message() {
    let source = r#"
fn foo() {
  a { b { c } d }
}
"#;
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(14..27).label(""))
            .annotation(Level::Warning.span(18..25).label("")),
    );

    let expected = str![[r#"
error: foo
 --> test.rs:3:3
  |
3 |   a { b { c } d }
  |   ^^^^-------^^
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}
#[test]
fn multiple_labels_without_message_2() {
    let source = r#"
fn foo() {
  a { b { c } d }
}
"#;
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(18..25).label(""))
            .annotation(Level::Warning.span(14..27).label(""))
            .annotation(Level::Warning.span(22..23).label("")),
    );

    let expected = str![[r#"
error: foo
 --> test.rs:3:7
  |
3 |   a { b { c } d }
  |   ----^^^^-^^--
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}
#[test]
fn multiple_labels_with_message() {
    let source = r#"
fn foo() {
  a { b { c } d }
}
"#;
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(14..27).label("`a` is a good letter"))
            .annotation(Level::Warning.span(18..25).label("`b` is a good letter")),
    );

    let expected = str![[r#"
error: foo
 --> test.rs:3:3
  |
3 |   a { b { c } d }
  |   ^^^^-------^^
  |   |   |
  |   |   `b` is a good letter
  |   `a` is a good letter
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}
#[test]
fn ingle_label_with_message() {
    let source = r#"
fn foo() {
  a { b { c } d }
}
"#;
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(14..27).label("`a` is a good letter")),
    );

    let expected = str![[r#"
error: foo
 --> test.rs:3:3
  |
3 |   a { b { c } d }
  |   ^^^^^^^^^^^^^ `a` is a good letter
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}
#[test]
fn single_label_without_message() {
    let source = r#"
fn foo() {
  a { b { c } d }
}
"#;
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(14..27).label("")),
    );

    let expected = str![[r#"
error: foo
 --> test.rs:3:3
  |
3 |   a { b { c } d }
  |   ^^^^^^^^^^^^^
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
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
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(17..27).label("`X` is a good letter"))
            .annotation(
                Level::Warning
                    .span(31..76)
                    .label("`Y` is a good letter too"),
            ),
    );

    let expected = str![[r#"
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
...   |
15 |  |   X2 Y2 Z2
16 |  |   X3 Y3 Z3
   |  |__________- `Y` is a good letter too
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
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
    let input = Level::Error.title("foo").snippet(
        Snippet::source(source)
            .line_start(1)
            .origin("test.rs")
            .fold(true)
            .annotation(Level::Error.span(17..73).label("`Y` is a good letter"))
            .annotation(
                Level::Warning
                    .span(37..56)
                    .label("`Z` is a good letter too"),
            ),
    );

    let expected = str![[r#"
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
12 | |  7
...  |
15 | |  10
16 | |    X3 Y3 Z3
   | |________^ `Y` is a good letter
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}
