use annotate_snippets::{
    Annotation, AnnotationKind, Group, Level, Origin, Padding, Patch, Renderer, Snippet,
};

use annotate_snippets::renderer::DecorStyle;
use snapbox::{assert_data_eq, str};

#[test]
fn test_i_29() {
    let input = &[Level::ERROR.primary_title("oops").element(
        Snippet::source("First line\r\nSecond oops line")
            .path("<current file>")
            .annotation(AnnotationKind::Primary.span(19..23).label("oops")),
    )];
    let expected_ascii = str![[r#"
error: oops
 --> <current file>:2:8
  |
2 | Second oops line
  |        ^^^^ oops
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: oops
  ╭▸ <current file>:2:8
  │
2 │ Second oops line
  ╰╴       ━━━━ oops
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn test_point_to_double_width_characters() {
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source("こんにちは、世界")
            .path("<current file>")
            .annotation(AnnotationKind::Primary.span(18..24).label("world")),
    )];

    let expected_ascii = str![[r#"
error: 
 --> <current file>:1:7
  |
1 | こんにちは、世界
  |             ^^^^ world
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ <current file>:1:7
  │
1 │ こんにちは、世界
  ╰╴            ━━━━ world
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn test_point_to_double_width_characters_across_lines() {
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source("おはよう\nございます")
            .path("<current file>")
            .annotation(AnnotationKind::Primary.span(6..22).label("Good morning")),
    )];

    let expected_ascii = str![[r#"
error: 
 --> <current file>:1:3
  |
1 |   おはよう
  |  _____^
2 | | ございます
  | |______^ Good morning
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ <current file>:1:3
  │
1 │   おはよう
  │ ┏━━━━━┛
2 │ ┃ ございます
  ╰╴┗━━━━━━┛ Good morning
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn test_point_to_double_width_characters_multiple() {
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source("お寿司\n食べたい🍣")
            .path("<current file>")
            .annotation(AnnotationKind::Primary.span(0..9).label("Sushi1"))
            .annotation(AnnotationKind::Context.span(16..22).label("Sushi2")),
    )];

    let expected_ascii = str![[r#"
error: 
 --> <current file>:1:1
  |
1 | お寿司
  | ^^^^^^ Sushi1
2 | 食べたい🍣
  |     ---- Sushi2
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ <current file>:1:1
  │
1 │ お寿司
  │ ━━━━━━ Sushi1
2 │ 食べたい🍣
  ╰╴    ──── Sushi2
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn test_point_to_double_width_characters_mixed() {
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source("こんにちは、新しいWorld！")
            .path("<current file>")
            .annotation(AnnotationKind::Primary.span(18..32).label("New world")),
    )];

    let expected_ascii = str![[r#"
error: 
 --> <current file>:1:7
  |
1 | こんにちは、新しいWorld！
  |             ^^^^^^^^^^^ New world
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ <current file>:1:7
  │
1 │ こんにちは、新しいWorld！
  ╰╴            ━━━━━━━━━━━ New world
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn test_format_title() {
    let input = &[Group::with_title(
        Level::ERROR.primary_title("This is a title").id("E0001"),
    )];

    let expected_ascii = str![r#"error[E0001]: This is a title"#];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str!["error[E0001]: This is a title"];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

/// Tests that we can format a message *without* a header.
///
/// This uses `Level::None`, which is somewhat of a hacky API addition I made
/// to our vendored copy of `annotate-snippets` in order to do exactly what
/// this test asserts: skip the header.
#[test]
fn test_format_skip_title() {
    let source =
        "# Docstring followed by a newline\n\ndef foobar(foot, bar={}):\n    \"\"\"\n    \"\"\"\n";
    let src_annotation = AnnotationKind::Primary.span(56..58).label("B006");
    let snippet = Snippet::source(source)
        .line_start(1)
        .annotation(src_annotation)
        .fold(false);
    let message = Group::with_level(Level::ERROR).element(snippet);

    let expected = str![[r#"
  |
1 | # Docstring followed by a newline
2 |
3 | def foobar(foot, bar={}):
  |                      ^^ B006
4 |     """
5 |     """
  |
"#]];
    assert_data_eq!(Renderer::plain().render(&[message]).clone(), expected);
}

#[test]
fn test_format_no_severity() {
    let input = &[Group::with_title(
        Level::ERROR
            .no_name()
            .primary_title("This is a title")
            .id("E0001"),
    )];

    let expected_ascii = str!["E0001 This is a title"];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str!["E0001 This is a title"];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn test_format_no_id() {
    let input = &[Group::with_title(
        Level::ERROR.primary_title("This is a title"),
    )];

    let expected_ascii = str!["error: This is a title"];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str!["error: This is a title"];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn test_format_no_severity_or_id() {
    let input = &[Group::with_title(
        Level::ERROR.no_name().primary_title("This is a title"),
    )];

    let expected_ascii = str!["This is a title"];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str!["This is a title"];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn test_format_snippet_only() {
    let source = "This is line 1\nThis is line 2";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::<Annotation<'_>>::source(source)
            .line_start(5402)
            .fold(false),
    )];

    let expected_ascii = str![[r#"
error: 
     |
5402 | This is line 1
5403 | This is line 2
     |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
     ╭▸ 
5402 │ This is line 1
5403 │ This is line 2
     ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn test_format_snippets_continuation() {
    let src_0 = "This is slice 1";
    let src_1 = "This is slice 2";
    let input = &[Level::ERROR
        .primary_title("")
        .element(
            Snippet::<Annotation<'_>>::source(src_0)
                .line_start(5402)
                .path("file1.rs")
                .fold(false),
        )
        .element(
            Snippet::<Annotation<'_>>::source(src_1)
                .line_start(2)
                .path("file2.rs")
                .fold(false),
        )];
    let expected_ascii = str![[r#"
error: 
    --> file1.rs
     |
5402 | This is slice 1
     |
    ::: file2.rs:2
     |
   2 | This is slice 2
     |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
     ╭▸ file1.rs
     │
5402 │ This is slice 1
     │
     ⸬  file2.rs:2
     │
   2 │ This is slice 2
     ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn test_format_snippet_annotation_standalone() {
    let line_1 = "This is line 1";
    let line_2 = "This is line 2";
    let source = [line_1, line_2].join("\n");
    // In line 2
    let range = 22..24;
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(&source)
            .line_start(5402)
            .fold(false)
            .annotation(
                AnnotationKind::Context
                    .span(range.clone())
                    .label("Test annotation"),
            ),
    )];
    let expected_ascii = str![[r#"
error: 
     |
5402 | This is line 1
5403 | This is line 2
     |        -- Test annotation
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
     ╭▸ 
5402 │ This is line 1
5403 │ This is line 2
     ╰╴       ── Test annotation
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn test_format_footer_title() {
    let input = &[Level::ERROR
        .primary_title("")
        .element(Level::ERROR.message("This __is__ a title"))];
    let expected_ascii = str![[r#"
error: 
  |
  = error: This __is__ a title
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  │
  ╰ error: This __is__ a title
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn test_multi_group_no_snippet() {
    let input = &[
        Group::with_title(Level::ERROR.primary_title("the core problem")),
        Group::with_title(Level::NOTE.secondary_title("more information")),
        Group::with_title(Level::HELP.secondary_title("a way to fix this")),
    ];
    let expected_ascii = str![[r#"
error: the core problem
  |
note: more information
help: a way to fix this
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: the core problem
  ╰╴
note: more information
help: a way to fix this
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn test_multi_secondary_group_no_snippet() {
    let input = &[
        Group::with_title(Level::ERROR.secondary_title("the core problem")),
        Group::with_title(Level::NOTE.secondary_title("more information")),
        Group::with_title(Level::HELP.secondary_title("a way to fix this")),
    ];
    let expected_ascii = str![[r#"
error: the core problem
note: more information
help: a way to fix this
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: the core problem
note: more information
help: a way to fix this
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
#[should_panic]
fn test_i26() {
    let source = "short";
    let label = "label";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source).line_start(0).annotation(
            AnnotationKind::Primary
                .span(0..source.len() + 2)
                .label(label),
        ),
    )];
    let renderer = Renderer::plain();
    let _ = renderer.render(input);
}

#[test]
fn test_source_content() {
    let source = "This is an example\nof content lines";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::<Annotation<'_>>::source(source)
            .line_start(56)
            .fold(false),
    )];
    let expected_ascii = str![[r#"
error: 
   |
56 | This is an example
57 | of content lines
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
   ╭▸ 
56 │ This is an example
57 │ of content lines
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn test_source_annotation_standalone_singleline() {
    let source = "tests";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .line_start(1)
            .annotation(AnnotationKind::Context.span(0..5).label("Example string")),
    )];
    let expected_ascii = str![[r#"
error: 
  |
1 | tests
  | ----- Example string
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ 
1 │ tests
  ╰╴───── Example string
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn test_source_annotation_standalone_multiline() {
    let source = "tests";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .line_start(1)
            .annotation(AnnotationKind::Context.span(0..5).label("Example string"))
            .annotation(AnnotationKind::Context.span(0..5).label("Second line")),
    )];
    let expected_ascii = str![[r#"
error: 
  |
1 | tests
  | -----
  | |
  | Example string
  | Second line
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ 
1 │ tests
  │ ┬────
  │ │
  │ Example string
  ╰╴Second line
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn test_only_source() {
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::<Annotation<'_>>::source("")
            .path("file.rs")
            .fold(false),
    )];
    let expected_ascii = str![[r#"
error: 
 --> file.rs
  |
1 |
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ file.rs
  │
1 │
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn test_anon_lines() {
    let source = "This is an example\nof content lines\n\nabc";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::<Annotation<'_>>::source(source)
            .line_start(56)
            .fold(false),
    )];
    let expected_ascii = str![[r#"
error: 
   |
56 | This is an example
57 | of content lines
58 |
59 | abc
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
   ╭▸ 
56 │ This is an example
57 │ of content lines
58 │
59 │ abc
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn issue_130() {
    let input = &[Level::ERROR.primary_title("dummy").element(
        Snippet::source("foo\nbar\nbaz")
            .path("file/path")
            .line_start(3)
            .annotation(AnnotationKind::Primary.span(4..11)),
        // bar\nbaz
    )];

    let expected_ascii = str![[r#"
error: dummy
 --> file/path:4:1
  |
4 | / bar
5 | | baz
  | |___^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: dummy
  ╭▸ file/path:4:1
  │
4 │ ┏ bar
5 │ ┃ baz
  ╰╴┗━━━┛
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn unterminated_string_multiline() {
    let source = "\
a\"
// ...
";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .path("file/path")
            .line_start(3)
            .annotation(AnnotationKind::Primary.span(0..10)),
        // 1..10 works
    )];
    let expected_ascii = str![[r#"
error: 
 --> file/path:3:1
  |
3 | / a"
4 | | // ...
  | |_______^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ file/path:3:1
  │
3 │ ┏ a"
4 │ ┃ // ...
  ╰╴┗━━━━━━━┛
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn char_and_nl_annotate_char() {
    let source = "a\r\nb";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .path("file/path")
            .line_start(3)
            .fold(false)
            .annotation(AnnotationKind::Primary.span(0..2)),
        // a\r
    )];
    let expected_ascii = str![[r#"
error: 
 --> file/path:3:1
  |
3 | a
  | ^
4 | b
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ file/path:3:1
  │
3 │ a
  │ ━
4 │ b
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn char_eol_annotate_char() {
    let source = "a\r\nb";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .path("file/path")
            .line_start(3)
            .annotation(AnnotationKind::Primary.span(0..3)),
        // a\r\n
    )];
    let expected_ascii = str![[r#"
error: 
 --> file/path:3:1
  |
3 | / a
4 | | b
  | |_^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ file/path:3:1
  │
3 │ ┏ a
4 │ ┃ b
  ╰╴┗━┛
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn char_eol_annotate_char_double_width() {
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source("こん\r\nにちは\r\n世界")
            .path("<current file>")
            .fold(false)
            .annotation(AnnotationKind::Primary.span(3..8)),
        // ん\r\n
    )];

    let expected_ascii = str![[r#"
error: 
 --> <current file>:1:2
  |
1 |   こん
  |  ___^
2 | | にちは
  | |_^
3 |   世界
  |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ <current file>:1:2
  │
1 │   こん
  │ ┏━━━┛
2 │ ┃ にちは
  │ ┗━┛
3 │   世界
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn annotate_newline_empty_span() {
    let input = &[Level::ERROR.primary_title("bad").element(
        Snippet::source("\n\n\n\n\n\n\n")
            .path("test.txt")
            .annotation(AnnotationKind::Primary.span(0..0)),
    )];

    let expected_ascii = str![[r#"
error: bad
 --> test.txt:1:1
  |
1 |
  | ^
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: bad
  ╭▸ test.txt:1:1
  │
1 │
  ╰╴━
"#]];

    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn annotate_eol() {
    let source = "a\r\nb";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .path("file/path")
            .line_start(3)
            .fold(false)
            .annotation(AnnotationKind::Primary.span(1..2)),
        // \r
    )];
    let expected_ascii = str![[r#"
error: 
 --> file/path:3:2
  |
3 | a
  |  ^
4 | b
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ file/path:3:2
  │
3 │ a
  │  ━
4 │ b
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn annotate_eol2() {
    let source = "a\r\nb";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .path("file/path")
            .line_start(3)
            .annotation(AnnotationKind::Primary.span(1..3)),
        // \r\n
    )];
    let expected_ascii = str![[r#"
error: 
 --> file/path:3:2
  |
3 |   a
  |  __^
4 | | b
  | |_^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ file/path:3:2
  │
3 │   a
  │ ┏━━┛
4 │ ┃ b
  ╰╴┗━┛
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn annotate_eol3() {
    let source = "a\r\nb";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .path("file/path")
            .line_start(3)
            .annotation(AnnotationKind::Primary.span(2..3)),
        // \n
    )];
    let expected_ascii = str![[r#"
error: 
 --> file/path:3:3
  |
3 |   a
  |  __^
4 | | b
  | |_^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ file/path:3:3
  │
3 │   a
  │ ┏━━┛
4 │ ┃ b
  ╰╴┗━┛
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn annotate_eol4() {
    let source = "a\r\nb";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .path("file/path")
            .line_start(3)
            .fold(false)
            .annotation(AnnotationKind::Primary.span(2..2)),
        // \n
    )];
    let expected_ascii = str![[r#"
error: 
 --> file/path:3:3
  |
3 | a
  |  ^
4 | b
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ file/path:3:3
  │
3 │ a
  │  ━
4 │ b
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn annotate_eol_double_width() {
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source("こん\r\nにちは\r\n世界")
            .path("<current file>")
            .fold(false)
            .annotation(AnnotationKind::Primary.span(7..8)),
        // \n
    )];

    let expected_ascii = str![[r#"
error: 
 --> <current file>:1:4
  |
1 |   こん
  |  _____^
2 | | にちは
  | |_^
3 |   世界
  |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ <current file>:1:4
  │
1 │   こん
  │ ┏━━━━━┛
2 │ ┃ にちは
  │ ┗━┛
3 │   世界
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn multiline_eol_start() {
    let source = "a\r\nb";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .path("file/path")
            .line_start(3)
            .annotation(AnnotationKind::Primary.span(1..4)),
        // \r\nb
    )];
    let expected_ascii = str![[r#"
error: 
 --> file/path:3:2
  |
3 |   a
  |  __^
4 | | b
  | |_^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ file/path:3:2
  │
3 │   a
  │ ┏━━┛
4 │ ┃ b
  ╰╴┗━┛
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn multiline_eol_start2() {
    let source = "a\r\nb";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .path("file/path")
            .line_start(3)
            .annotation(AnnotationKind::Primary.span(2..4)),
        // \nb
    )];
    let expected_ascii = str![[r#"
error: 
 --> file/path:3:3
  |
3 |   a
  |  __^
4 | | b
  | |_^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ file/path:3:3
  │
3 │   a
  │ ┏━━┛
4 │ ┃ b
  ╰╴┗━┛
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn multiline_eol_start3() {
    let source = "a\nb";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .path("file/path")
            .line_start(3)
            .annotation(AnnotationKind::Primary.span(1..3)),
        // \nb
    )];
    let expected_ascii = str![[r#"
error: 
 --> file/path:3:2
  |
3 |   a
  |  __^
4 | | b
  | |_^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ file/path:3:2
  │
3 │   a
  │ ┏━━┛
4 │ ┃ b
  ╰╴┗━┛
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn multiline_eol_start_double_width() {
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source("こん\r\nにちは\r\n世界")
            .path("<current file>")
            .fold(false)
            .annotation(AnnotationKind::Primary.span(7..11)),
        // \r\nに
    )];

    let expected_ascii = str![[r#"
error: 
 --> <current file>:1:4
  |
1 |   こん
  |  _____^
2 | | にちは
  | |__^
3 |   世界
  |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ <current file>:1:4
  │
1 │   こん
  │ ┏━━━━━┛
2 │ ┃ にちは
  │ ┗━━┛
3 │   世界
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn multiline_eol_start_eol_end() {
    let source = "a\nb\nc";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .path("file/path")
            .line_start(3)
            .annotation(AnnotationKind::Primary.span(1..4)),
        // \nb\n
    )];
    let expected_ascii = str![[r#"
error: 
 --> file/path:3:2
  |
3 |   a
  |  __^
4 | | b
5 | | c
  | |_^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ file/path:3:2
  │
3 │   a
  │ ┏━━┛
4 │ ┃ b
5 │ ┃ c
  ╰╴┗━┛
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn multiline_eol_start_eol_end2() {
    let source = "a\r\nb\r\nc";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .path("file/path")
            .line_start(3)
            .fold(false)
            .annotation(AnnotationKind::Primary.span(2..5)),
        // \nb\r
    )];
    let expected_ascii = str![[r#"
error: 
 --> file/path:3:3
  |
3 |   a
  |  __^
4 | | b
  | |__^
5 |   c
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ file/path:3:3
  │
3 │   a
  │ ┏━━┛
4 │ ┃ b
  │ ┗━━┛
5 │   c
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn multiline_eol_start_eol_end3() {
    let source = "a\r\nb\r\nc";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .path("file/path")
            .line_start(3)
            .annotation(AnnotationKind::Primary.span(2..6)),
        // \nb\r\n
    )];
    let expected_ascii = str![[r#"
error: 
 --> file/path:3:3
  |
3 |   a
  |  __^
4 | | b
5 | | c
  | |_^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ file/path:3:3
  │
3 │   a
  │ ┏━━┛
4 │ ┃ b
5 │ ┃ c
  ╰╴┗━┛
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn multiline_eol_start_eof_end() {
    let source = "a\r\nb";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .path("file/path")
            .line_start(3)
            .annotation(AnnotationKind::Primary.span(1..5)),
        // \r\nb(EOF)
    )];
    let expected_ascii = str![[r#"
error: 
 --> file/path:3:2
  |
3 |   a
  |  __^
4 | | b
  | |__^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ file/path:3:2
  │
3 │   a
  │ ┏━━┛
4 │ ┃ b
  ╰╴┗━━┛
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn multiline_eol_start_eof_end_double_width() {
    let source = "ん\r\nに";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .path("file/path")
            .line_start(3)
            .annotation(AnnotationKind::Primary.span(3..9)),
        // \r\nに(EOF)
    )];
    let expected_ascii = str![[r#"
error: 
 --> file/path:3:2
  |
3 |   ん
  |  ___^
4 | | に
  | |___^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ file/path:3:2
  │
3 │   ん
  │ ┏━━━┛
4 │ ┃ に
  ╰╴┗━━━┛
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn two_single_line_same_line() {
    let source = r#"bar = { version = "0.1.0", optional = true }"#;
    let input = &[Level::ERROR
        .primary_title("unused optional dependency")
        .element(
            Snippet::source(source)
                .path("Cargo.toml")
                .line_start(4)
                .annotation(
                    AnnotationKind::Primary
                        .span(0..3)
                        .label("I need this to be really long so I can test overlaps"),
                )
                .annotation(
                    AnnotationKind::Context
                        .span(27..42)
                        .label("This should also be long but not too long"),
                ),
        )];
    let expected_ascii = str![[r#"
error: unused optional dependency
 --> Cargo.toml:4:1
  |
4 | bar = { version = "0.1.0", optional = true }
  | ^^^                        --------------- This should also be long but not too long
  | |
  | I need this to be really long so I can test overlaps
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: unused optional dependency
  ╭▸ Cargo.toml:4:1
  │
4 │ bar = { version = "0.1.0", optional = true }
  │ ┯━━                        ─────────────── This should also be long but not too long
  │ │
  ╰╴I need this to be really long so I can test overlaps
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn multi_and_single() {
    let source = r#"bar = { version = "0.1.0", optional = true }
this is another line
so is this
bar = { version = "0.1.0", optional = true }
"#;
    let input = &[Level::ERROR
        .primary_title("unused optional dependency")
        .element(
            Snippet::source(source)
                .line_start(4)
                .annotation(
                    AnnotationKind::Primary
                        .span(41..119)
                        .label("I need this to be really long so I can test overlaps"),
                )
                .annotation(
                    AnnotationKind::Context
                        .span(27..42)
                        .label("This should also be long but not too long"),
                ),
        )];
    let expected_ascii = str![[r#"
error: unused optional dependency
  |
4 |   bar = { version = "0.1.0", optional = true }
  |  ____________________________--------------^
  | |                            |
  | |                            This should also be long but not too long
5 | | this is another line
6 | | so is this
7 | | bar = { version = "0.1.0", optional = true }
  | |__________________________________________^ I need this to be really long so I can test overlaps
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: unused optional dependency
  ╭▸ 
4 │   bar = { version = "0.1.0", optional = true }
  │ ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━┬─────────────┛
  │ ┃                            │
  │ ┃                            This should also be long but not too long
5 │ ┃ this is another line
6 │ ┃ so is this
7 │ ┃ bar = { version = "0.1.0", optional = true }
  ╰╴┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛ I need this to be really long so I can test overlaps
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn two_multi_and_single() {
    let source = r#"bar = { version = "0.1.0", optional = true }
this is another line
so is this
bar = { version = "0.1.0", optional = true }
"#;
    let input = &[Level::ERROR
        .primary_title("unused optional dependency")
        .element(
            Snippet::source(source)
                .line_start(4)
                .annotation(
                    AnnotationKind::Primary
                        .span(41..119)
                        .label("I need this to be really long so I can test overlaps"),
                )
                .annotation(
                    AnnotationKind::Primary
                        .span(8..102)
                        .label("I need this to be really long so I can test overlaps"),
                )
                .annotation(
                    AnnotationKind::Context
                        .span(27..42)
                        .label("This should also be long but not too long"),
                ),
        )];
    let expected_ascii = str![[r#"
error: unused optional dependency
  |
4 |    bar = { version = "0.1.0", optional = true }
  |  __________^__________________--------------^
  | |          |                  |
  | | _________|                  This should also be long but not too long
  | ||
5 | || this is another line
6 | || so is this
7 | || bar = { version = "0.1.0", optional = true }
  | ||_________________________^________________^ I need this to be really long so I can test overlaps
  |  |_________________________|
  |                            I need this to be really long so I can test overlaps
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: unused optional dependency
  ╭▸ 
4 │    bar = { version = "0.1.0", optional = true }
  │ ┏━━━━━━━━━━╿━━━━━━━━━━━━━━━━━━┬─────────────┛
  │ ┃          │                  │
  │ ┃┏━━━━━━━━━┙                  This should also be long but not too long
  │ ┃┃
5 │ ┃┃ this is another line
6 │ ┃┃ so is this
7 │ ┃┃ bar = { version = "0.1.0", optional = true }
  │ ┗┃━━━━━━━━━━━━━━━━━━━━━━━━━╿━━━━━━━━━━━━━━━━┛ I need this to be really long so I can test overlaps
  │  ┗━━━━━━━━━━━━━━━━━━━━━━━━━┥
  ╰╴                           I need this to be really long so I can test overlaps
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn three_multi_and_single() {
    let source = r#"bar = { version = "0.1.0", optional = true }
this is another line
so is this
bar = { version = "0.1.0", optional = true }
this is another line
"#;
    let input = &[Level::ERROR
        .primary_title("unused optional dependency")
        .element(
            Snippet::source(source)
                .line_start(4)
                .annotation(
                    AnnotationKind::Primary
                        .span(41..119)
                        .label("I need this to be really long so I can test overlaps"),
                )
                .annotation(
                    AnnotationKind::Primary
                        .span(8..102)
                        .label("I need this to be really long so I can test overlaps"),
                )
                .annotation(
                    AnnotationKind::Primary
                        .span(48..126)
                        .label("I need this to be really long so I can test overlaps"),
                )
                .annotation(
                    AnnotationKind::Context
                        .span(27..42)
                        .label("This should also be long but not too long"),
                ),
        )];
    let expected_ascii = str![[r#"
error: unused optional dependency
  |
4 |     bar = { version = "0.1.0", optional = true }
  |  ___________^__________________--------------^
  | |           |                  |
  | | __________|                  This should also be long but not too long
  | ||
5 | ||  this is another line
  | || ____^
6 | ||| so is this
7 | ||| bar = { version = "0.1.0", optional = true }
  | |||_________________________^________________^ I need this to be really long so I can test overlaps
  |  ||_________________________|
  |   |                         I need this to be really long so I can test overlaps
8 |   | this is another line
  |   |____^ I need this to be really long so I can test overlaps
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: unused optional dependency
  ╭▸ 
4 │     bar = { version = "0.1.0", optional = true }
  │ ┏━━━━━━━━━━━╿━━━━━━━━━━━━━━━━━━┬─────────────┛
  │ ┃           │                  │
  │ ┃┏━━━━━━━━━━┙                  This should also be long but not too long
  │ ┃┃
5 │ ┃┃  this is another line
  │ ┃┃┏━━━━┛
6 │ ┃┃┃ so is this
7 │ ┃┃┃ bar = { version = "0.1.0", optional = true }
  │ ┗┃┃━━━━━━━━━━━━━━━━━━━━━━━━━╿━━━━━━━━━━━━━━━━┛ I need this to be really long so I can test overlaps
  │  ┗┃━━━━━━━━━━━━━━━━━━━━━━━━━┥
  │   ┃                         I need this to be really long so I can test overlaps
8 │   ┃ this is another line
  ╰╴  ┗━━━━┛ I need this to be really long so I can test overlaps
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn origin_correct_start_line() {
    let source = "aaa\nbbb\nccc\nddd\n";
    let input = &[Level::ERROR.primary_title("title").element(
        Snippet::source(source)
            .path("origin.txt")
            .fold(false)
            .annotation(AnnotationKind::Primary.span(8..8 + 3).label("annotation")),
    )];

    let expected_ascii = str![[r#"
error: title
 --> origin.txt:3:1
  |
1 | aaa
2 | bbb
3 | ccc
  | ^^^ annotation
4 | ddd
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: title
  ╭▸ origin.txt:3:1
  │
1 │ aaa
2 │ bbb
3 │ ccc
  │ ━━━ annotation
4 │ ddd
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn origin_correct_mid_line() {
    let source = "aaa\nbbb\nccc\nddd\n";
    let input = &[Level::ERROR.primary_title("title").element(
        Snippet::source(source)
            .path("origin.txt")
            .fold(false)
            .annotation(
                AnnotationKind::Primary
                    .span(8 + 1..8 + 3)
                    .label("annotation"),
            ),
    )];

    let expected_ascii = str![[r#"
error: title
 --> origin.txt:3:2
  |
1 | aaa
2 | bbb
3 | ccc
  |  ^^ annotation
4 | ddd
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: title
  ╭▸ origin.txt:3:2
  │
1 │ aaa
2 │ bbb
3 │ ccc
  │  ━━ annotation
4 │ ddd
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn two_suggestions_same_span() {
    let source = r#"    A.foo();"#;
    let input = &[
        Level::ERROR
            .primary_title("expected value, found enum `A`")
            .id("E0423")
            .element(Snippet::source(source).annotation(AnnotationKind::Primary.span(4..5))),
        Level::HELP
            .secondary_title("you might have meant to use one of the following enum variants")
            .element(Snippet::source(source).patch(Patch::new(4..5, "(A::Tuple())")))
            .element(Snippet::source(source).patch(Patch::new(4..5, "A::Unit"))),
    ];

    let expected_ascii = str![[r#"
error[E0423]: expected value, found enum `A`
  |
1 |     A.foo();
  |     ^
  |
help: you might have meant to use one of the following enum variants
  |
1 -     A.foo();
1 +     (A::Tuple()).foo();
  |
1 |     A::Unit.foo();
  |      ++++++
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0423]: expected value, found enum `A`
  ╭▸ 
1 │     A.foo();
  │     ━
  ╰╴
help: you might have meant to use one of the following enum variants
  ╭╴
1 -     A.foo();
1 +     (A::Tuple()).foo();
  ├╴
1 │     A::Unit.foo();
  ╰╴     ++++++
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn two_suggestions_same_span2() {
    let source = r#"
mod banana {
    pub struct Chaenomeles;

    pub trait Apple {
        fn pick(&self) {}
    }
    impl Apple for Chaenomeles {}

    pub trait Peach {
        fn pick(&self, a: &mut ()) {}
    }
    impl<Mango: Peach> Peach for Box<Mango> {}
    impl Peach for Chaenomeles {}
}

fn main() {
    banana::Chaenomeles.pick()
}"#;
    let input =
        &[Level::ERROR
            .primary_title("no method named `pick` found for struct `Chaenomeles` in the current scope")
            .id("E0599").element(
                    Snippet::source(source)
                        .line_start(1)

                        .annotation(
                            AnnotationKind::Context
                                .span(18..40)
                                .label("method `pick` not found for this struct"),
                        )
                        .annotation(
                            AnnotationKind::Primary
                                .span(318..322)
                                .label("method not found in `Chaenomeles`"),
                        ),
                ),
                Level::HELP.secondary_title(
                        "the following traits which provide `pick` are implemented but not in scope; perhaps you want to import one of them",
                    )
                    .element(
                        Snippet::source(source)

                            .patch(Patch::new(1..1, "use banana::Apple;\n")),
                    )
                    .element(
                        Snippet::source(source)

                            .patch(Patch::new(1..1, "use banana::Peach;\n")),
                   )];
    let expected_ascii = str![[r#"
error[E0599]: no method named `pick` found for struct `Chaenomeles` in the current scope
   |
 3 |     pub struct Chaenomeles;
   |     ---------------------- method `pick` not found for this struct
...
18 |     banana::Chaenomeles.pick()
   |                         ^^^^ method not found in `Chaenomeles`
   |
help: the following traits which provide `pick` are implemented but not in scope; perhaps you want to import one of them
   |
 2 + use banana::Apple;
   |
 2 + use banana::Peach;
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0599]: no method named `pick` found for struct `Chaenomeles` in the current scope
   ╭▸ 
 3 │     pub struct Chaenomeles;
   │     ────────────────────── method `pick` not found for this struct
   ┆
18 │     banana::Chaenomeles.pick()
   │                         ━━━━ method not found in `Chaenomeles`
   ╰╴
help: the following traits which provide `pick` are implemented but not in scope; perhaps you want to import one of them
   ╭╴
 2 + use banana::Apple;
   ├╴
 2 + use banana::Peach;
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn single_line_non_overlapping_suggestions() {
    let source = r#"    A.foo();"#;

    let input = &[
        Level::ERROR
            .primary_title("expected value, found enum `A`")
            .id("E0423")
            .element(
                Snippet::source(source)
                    .line_start(1)
                    .annotation(AnnotationKind::Primary.span(4..5)),
            ),
        Level::HELP
            .secondary_title("make these changes and things will work")
            .element(
                Snippet::source(source)
                    .patch(Patch::new(4..5, "(A::Tuple())"))
                    .patch(Patch::new(6..9, "bar")),
            ),
    ];

    let expected_ascii = str![[r#"
error[E0423]: expected value, found enum `A`
  |
1 |     A.foo();
  |     ^
  |
help: make these changes and things will work
  |
1 -     A.foo();
1 +     (A::Tuple()).bar();
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0423]: expected value, found enum `A`
  ╭▸ 
1 │     A.foo();
  │     ━
  ╰╴
help: make these changes and things will work
  ╭╴
1 -     A.foo();
1 +     (A::Tuple()).bar();
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn single_line_non_overlapping_suggestions2() {
    let source = r#"    ThisIsVeryLong.foo();"#;
    let input = &[
        Level::ERROR
            .primary_title("Found `ThisIsVeryLong`")
            .id("E0423")
            .element(
                Snippet::source(source)
                    .line_start(1)
                    .annotation(AnnotationKind::Primary.span(4..18)),
            ),
        Level::HELP
            .secondary_title("make these changes and things will work")
            .element(
                Snippet::source(source)
                    .patch(Patch::new(4..18, "(A::Tuple())"))
                    .patch(Patch::new(19..22, "bar")),
            ),
    ];

    let expected_ascii = str![[r#"
error[E0423]: Found `ThisIsVeryLong`
  |
1 |     ThisIsVeryLong.foo();
  |     ^^^^^^^^^^^^^^
  |
help: make these changes and things will work
  |
1 -     ThisIsVeryLong.foo();
1 +     (A::Tuple()).bar();
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0423]: Found `ThisIsVeryLong`
  ╭▸ 
1 │     ThisIsVeryLong.foo();
  │     ━━━━━━━━━━━━━━
  ╰╴
help: make these changes and things will work
  ╭╴
1 -     ThisIsVeryLong.foo();
1 +     (A::Tuple()).bar();
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn multiple_replacements() {
    let source = r#"
    let y = || {
        self.bar();
    };
    self.qux();
    y();
"#;

    let input = &[
        Level::ERROR
            .primary_title(
                "cannot borrow `*self` as mutable because it is also borrowed as immutable",
            )
            .id("E0502")
            .element(
                Snippet::source(source)
                    .line_start(1)
                    .annotation(
                        AnnotationKind::Primary
                            .span(49..59)
                            .label("mutable borrow occurs here"),
                    )
                    .annotation(
                        AnnotationKind::Primary
                            .span(13..15)
                            .label("immutable borrow occurs here"),
                    )
                    .annotation(
                        AnnotationKind::Primary
                            .span(26..30)
                            .label("first borrow occurs due to use of `*self` in closure"),
                    )
                    .annotation(
                        AnnotationKind::Primary
                            .span(65..66)
                            .label("immutable borrow later used here"),
                    ),
            ),
        Level::HELP
            .secondary_title("try explicitly pass `&Self` into the Closure as an argument")
            .element(
                Snippet::source(source)
                    .patch(Patch::new(14..14, "this: &Self"))
                    .patch(Patch::new(26..30, "this"))
                    .patch(Patch::new(66..68, "(self)")),
            ),
    ];
    let expected_ascii = str![[r#"
error[E0502]: cannot borrow `*self` as mutable because it is also borrowed as immutable
  |
2 |     let y = || {
  |             ^^ immutable borrow occurs here
3 |         self.bar();
  |         ^^^^ first borrow occurs due to use of `*self` in closure
4 |     };
5 |     self.qux();
  |     ^^^^^^^^^^ mutable borrow occurs here
6 |     y();
  |     ^ immutable borrow later used here
  |
help: try explicitly pass `&Self` into the Closure as an argument
  |
2 ~     let y = |this: &Self| {
3 ~         this.bar();
4 |     };
5 |     self.qux();
6 ~     y(self);
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0502]: cannot borrow `*self` as mutable because it is also borrowed as immutable
  ╭▸ 
2 │     let y = || {
  │             ━━ immutable borrow occurs here
3 │         self.bar();
  │         ━━━━ first borrow occurs due to use of `*self` in closure
4 │     };
5 │     self.qux();
  │     ━━━━━━━━━━ mutable borrow occurs here
6 │     y();
  │     ━ immutable borrow later used here
  ╰╴
help: try explicitly pass `&Self` into the Closure as an argument
  ╭╴
2 ±     let y = |this: &Self| {
3 ±         this.bar();
4 │     };
5 │     self.qux();
6 ±     y(self);
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn multiple_replacements2() {
    let source = r#"
fn test1() {
    let mut chars = "Hello".chars();
    for _c in chars.by_ref() {
        chars.next();
    }
}

fn main() {
    test1();
}"#;

    let input = &[
        Level::ERROR
            .primary_title("cannot borrow `chars` as mutable more than once at a time")
            .id("E0499")
            .element(
                Snippet::source(source)
                    .line_start(1)
                    .annotation(
                        AnnotationKind::Context
                            .span(65..70)
                            .label("first mutable borrow occurs here"),
                    )
                    .annotation(
                        AnnotationKind::Primary
                            .span(90..95)
                            .label("second mutable borrow occurs here"),
                    )
                    .annotation(
                        AnnotationKind::Context
                            .span(65..79)
                            .label("first borrow later used here"),
                    ),
            ),
        Level::HELP.secondary_title(
            "if you want to call `next` on a iterator within the loop, consider using `while let`",
        )
        .element(
            Snippet::source(source)
                .patch(Patch::new(
                    55..59,
                    "let iter = chars.by_ref();\n    while let Some(",
                ))
                .patch(Patch::new(61..79, ") = iter.next()"))
                .patch(Patch::new(90..95, "iter")),
        ),
    ];

    let expected_ascii = str![[r#"
error[E0499]: cannot borrow `chars` as mutable more than once at a time
  |
4 |     for _c in chars.by_ref() {
  |               --------------
  |               |
  |               first mutable borrow occurs here
  |               first borrow later used here
5 |         chars.next();
  |         ^^^^^ second mutable borrow occurs here
  |
help: if you want to call `next` on a iterator within the loop, consider using `while let`
  |
4 ~     let iter = chars.by_ref();
5 ~     while let Some(_c) = iter.next() {
6 ~         iter.next();
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0499]: cannot borrow `chars` as mutable more than once at a time
  ╭▸ 
4 │     for _c in chars.by_ref() {
  │               ┬─────────────
  │               │
  │               first mutable borrow occurs here
  │               first borrow later used here
5 │         chars.next();
  │         ━━━━━ second mutable borrow occurs here
  ╰╴
help: if you want to call `next` on a iterator within the loop, consider using `while let`
  ╭╴
4 ±     let iter = chars.by_ref();
5 ±     while let Some(_c) = iter.next() {
6 ±         iter.next();
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn diff_format() {
    let source = r#"
use st::cell::Cell;

mod bar {
    pub fn bar() { bar::baz(); }

    fn baz() {}
}

use bas::bar;

struct Foo {
    bar: st::cell::Cell<bool>
}

fn main() {}"#;

    let input = &[
        Level::ERROR
            .primary_title("failed to resolve: use of undeclared crate or module `st`")
            .id("E0433")
            .element(
                Snippet::source(source).line_start(1).annotation(
                    AnnotationKind::Primary
                        .span(122..124)
                        .label("use of undeclared crate or module `st`"),
                ),
            ),
        Level::HELP
            .secondary_title("there is a crate or module with a similar name")
            .element(Snippet::source(source).patch(Patch::new(122..124, "std"))),
        Level::HELP
            .secondary_title("consider importing this module")
            .element(Snippet::source(source).patch(Patch::new(1..1, "use std::cell;\n"))),
        Level::HELP
            .secondary_title("if you import `cell`, refer to it directly")
            .element(Snippet::source(source).patch(Patch::new(122..126, ""))),
    ];
    let expected_ascii = str![[r#"
error[E0433]: failed to resolve: use of undeclared crate or module `st`
   |
13 |     bar: st::cell::Cell<bool>
   |          ^^ use of undeclared crate or module `st`
   |
help: there is a crate or module with a similar name
   |
13 |     bar: std::cell::Cell<bool>
   |            +
help: consider importing this module
   |
 2 + use std::cell;
   |
help: if you import `cell`, refer to it directly
   |
13 -     bar: st::cell::Cell<bool>
13 +     bar: cell::Cell<bool>
   |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0433]: failed to resolve: use of undeclared crate or module `st`
   ╭▸ 
13 │     bar: st::cell::Cell<bool>
   │          ━━ use of undeclared crate or module `st`
   ╰╴
help: there is a crate or module with a similar name
   ╭╴
13 │     bar: std::cell::Cell<bool>
   ╰╴           +
help: consider importing this module
   ╭╴
 2 + use std::cell;
   ╰╴
help: if you import `cell`, refer to it directly
   ╭╴
13 -     bar: st::cell::Cell<bool>
13 +     bar: cell::Cell<bool>
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn multiline_removal() {
    let source = r#"
struct Wrapper<T>(T);

fn foo<T>(foo: Wrapper<T>)

where
    T
    :
    ?
    Sized
{
    //
}

fn main() {}"#;

    let input = &[
        Level::ERROR
            .primary_title("the size for values of type `T` cannot be known at compilation time")
            .id("E0277")
            .element(
                Snippet::source(source)
                    .line_start(1)
                    .annotation(
                        AnnotationKind::Primary
                            .span(39..49)
                            .label("doesn't have a size known at compile-time"),
                    )
                    .annotation(
                        AnnotationKind::Context
                            .span(31..32)
                            .label("this type parameter needs to be `Sized`"),
                    ),
            ),
        Level::HELP
            .secondary_title(
                "consider removing the `?Sized` bound to make the type parameter `Sized`",
            )
            .element(Snippet::source(source).patch(Patch::new(52..85, ""))),
    ];
    let expected_ascii = str![[r#"
error[E0277]: the size for values of type `T` cannot be known at compilation time
   |
 4 | fn foo<T>(foo: Wrapper<T>)
   |        -       ^^^^^^^^^^ doesn't have a size known at compile-time
   |        |
   |        this type parameter needs to be `Sized`
   |
help: consider removing the `?Sized` bound to make the type parameter `Sized`
   |
 6 - where
 7 -     T
 8 -     :
 9 -     ?
10 -     Sized
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0277]: the size for values of type `T` cannot be known at compilation time
   ╭▸ 
 4 │ fn foo<T>(foo: Wrapper<T>)
   │        ┬       ━━━━━━━━━━ doesn't have a size known at compile-time
   │        │
   │        this type parameter needs to be `Sized`
   ╰╴
help: consider removing the `?Sized` bound to make the type parameter `Sized`
   ╭╴
 6 - where
 7 -     T
 8 -     :
 9 -     ?
10 -     Sized
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn multiline_replacement() {
    let source = r#"
struct Wrapper<T>(T);

fn foo<T>(foo: Wrapper<T>)

and where
    T
    :
    ?
    Sized
{
    //
}

fn main() {}"#;
    let input = &[
        Level::ERROR
            .primary_title("the size for values of type `T` cannot be known at compilation time")
            .id("E0277").element(Snippet::source(source)
                .line_start(1)
                .path("$DIR/removal-of-multiline-trait-bound-in-where-clause.rs")

                .annotation(
                    AnnotationKind::Primary
                        .span(39..49)
                        .label("doesn't have a size known at compile-time"),
                )
                .annotation(
                    AnnotationKind::Context
                        .span(31..32)
                        .label("this type parameter needs to be `Sized`"),
                )
            ),
        Level::NOTE
            .secondary_title("required by an implicit `Sized` bound in `Wrapper`")
            .element(
                Snippet::source(source)
                    .line_start(1)
                    .path("$DIR/removal-of-multiline-trait-bound-in-where-clause.rs")

                    .annotation(
                        AnnotationKind::Primary
                            .span(16..17)
                            .label("required by the implicit `Sized` requirement on this type parameter in `Wrapper`"),
                    )
            ),
        Level::HELP
            .secondary_title("you could relax the implicit `Sized` bound on `T` if it were used through indirection like `&T` or `Box<T>`")
            .element(
                Snippet::source(source)
                    .line_start(1)
                    .path("$DIR/removal-of-multiline-trait-bound-in-where-clause.rs")

                    .annotation(
                        AnnotationKind::Primary
                            .span(16..17)
                            .label("this could be changed to `T: ?Sized`..."),
                    )
                    .annotation(
                        AnnotationKind::Context
                            .span(19..20)
                            .label("...if indirection were used here: `Box<T>`"),
                    )
            ),
        Level::HELP
            .secondary_title("consider removing the `?Sized` bound to make the type parameter `Sized`")
            .element(
                Snippet::source(source)

                    .patch(Patch::new(56..89, ""))
                    .patch(Patch::new(89..89, "+ Send"))
                    ,
            )
    ];
    let expected_ascii = str![[r#"
error[E0277]: the size for values of type `T` cannot be known at compilation time
  --> $DIR/removal-of-multiline-trait-bound-in-where-clause.rs:4:16
   |
 4 | fn foo<T>(foo: Wrapper<T>)
   |        -       ^^^^^^^^^^ doesn't have a size known at compile-time
   |        |
   |        this type parameter needs to be `Sized`
   |
note: required by an implicit `Sized` bound in `Wrapper`
  --> $DIR/removal-of-multiline-trait-bound-in-where-clause.rs:2:16
   |
 2 | struct Wrapper<T>(T);
   |                ^ required by the implicit `Sized` requirement on this type parameter in `Wrapper`
help: you could relax the implicit `Sized` bound on `T` if it were used through indirection like `&T` or `Box<T>`
  --> $DIR/removal-of-multiline-trait-bound-in-where-clause.rs:2:16
   |
 2 | struct Wrapper<T>(T);
   |                ^  - ...if indirection were used here: `Box<T>`
   |                |
   |                this could be changed to `T: ?Sized`...
help: consider removing the `?Sized` bound to make the type parameter `Sized`
   |
 6 - and where
 7 -     T
 8 -     :
 9 -     ?
10 -     Sized
 6 + and + Send
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0277]: the size for values of type `T` cannot be known at compilation time
   ╭▸ $DIR/removal-of-multiline-trait-bound-in-where-clause.rs:4:16
   │
 4 │ fn foo<T>(foo: Wrapper<T>)
   │        ┬       ━━━━━━━━━━ doesn't have a size known at compile-time
   │        │
   │        this type parameter needs to be `Sized`
   ╰╴
note: required by an implicit `Sized` bound in `Wrapper`
   ╭▸ $DIR/removal-of-multiline-trait-bound-in-where-clause.rs:2:16
   │
 2 │ struct Wrapper<T>(T);
   ╰╴               ━ required by the implicit `Sized` requirement on this type parameter in `Wrapper`
help: you could relax the implicit `Sized` bound on `T` if it were used through indirection like `&T` or `Box<T>`
   ╭▸ $DIR/removal-of-multiline-trait-bound-in-where-clause.rs:2:16
   │
 2 │ struct Wrapper<T>(T);
   │                ┯  ─ ...if indirection were used here: `Box<T>`
   │                │
   ╰╴               this could be changed to `T: ?Sized`...
help: consider removing the `?Sized` bound to make the type parameter `Sized`
   ╭╴
 6 - and where
 7 -     T
 8 -     :
 9 -     ?
10 -     Sized
 6 + and + Send
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn multiline_removal2() {
    let source = r#"
cargo
fuzzy
pizza
jumps
crazy
quack
zappy
"#;

    let input = &[
        Group::with_title(
            Level::ERROR
                .primary_title(
                    "the size for values of type `T` cannot be known at compilation time",
                )
                .id("E0277"),
        ),
        // We need an empty group here to ensure the HELP line is rendered correctly
        Level::HELP
            .secondary_title(
                "consider removing the `?Sized` bound to make the type parameter `Sized`",
            )
            .element(
                Snippet::source(source)
                    .line_start(7)
                    .patch(Patch::new(3..21, ""))
                    .patch(Patch::new(22..40, "")),
            ),
    ];
    let expected_ascii = str![[r#"
error[E0277]: the size for values of type `T` cannot be known at compilation time
   |
help: consider removing the `?Sized` bound to make the type parameter `Sized`
   |
 8 - cargo
 9 - fuzzy
10 - pizza
11 - jumps
12 - crazy
13 - quack
14 - zappy
 8 + campy
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0277]: the size for values of type `T` cannot be known at compilation time
   ╰╴
help: consider removing the `?Sized` bound to make the type parameter `Sized`
   ╭╴
 8 - cargo
 9 - fuzzy
10 - pizza
11 - jumps
12 - crazy
13 - quack
14 - zappy
 8 + campy
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn e0271() {
    let source = r#"
trait Future {
    type Error;
}

impl<T, E> Future for Result<T, E> {
    type Error = E;
}

impl<T> Future for Option<T> {
    type Error = ();
}

struct Foo;

fn foo() -> Box<dyn Future<Error=Foo>> {
    Box::new(
        Ok::<_, ()>(
            Err::<(), _>(
                Ok::<_, ()>(
                    Err::<(), _>(
                        Ok::<_, ()>(
                            Err::<(), _>(Some(5))
                        )
                    )
                )
            )
        )
    )
}
fn main() {
}
"#;

    let input = &[
        Level::ERROR
            .primary_title("type mismatch resolving `<Result<Result<(), Result<Result<(), Result<Result<(), Option<{integer}>>, ...>>, ...>>, ...> as Future>::Error == Foo`")
            .id("E0271")
            .element(Snippet::source(source)
                .line_start(4)
                .path("$DIR/E0271.rs")
                .annotation(
                    AnnotationKind::Primary
                        .span(208..510)
                        .label("type mismatch resolving `<Result<Result<(), Result<Result<(), ...>, ...>>, ...> as Future>::Error == Foo`"),
                )
            ),
        Level::NOTE.secondary_title("expected this to be `Foo`")
            .element(
                Snippet::source(source)
                    .line_start(4)
                    .path("$DIR/E0271.rs")
                    .annotation(AnnotationKind::Primary.span(89..90))
            )
            .element(
                Level::NOTE
                    .message("required for the cast from `Box<Result<Result<(), Result<Result<(), Result<Result<(), Option<{integer}>>, ()>>, ()>>, ()>>` to `Box<(dyn Future<Error = Foo> + 'static)>`")
            )
        ];

    let expected_ascii = str![[r#"
error[E0271]: type mismatch resolving `<Result<Result<(), Result<Result<(), Result<Result<(), Option<{integer}>>, ...>>, ...>>, ...> as Future>::Error == Foo`
  --> $DIR/E0271.rs:20:5
   |
20 | /     Box::new(
21 | |         Ok::<_, ()>(
22 | |             Err::<(), _>(
23 | |                 Ok::<_, ()>(
...  |
32 | |     )
   | |_____^ type mismatch resolving `<Result<Result<(), Result<Result<(), ...>, ...>>, ...> as Future>::Error == Foo`
   |
note: expected this to be `Foo`
  --> $DIR/E0271.rs:10:18
   |
10 |     type Error = E;
   |                  ^
   = note: required for the cast from `Box<Result<Result<(), Result<Result<(), Result<Result<(), Option<{integer}>>, ()>>, ()>>, ()>>` to `Box<(dyn Future<Error = Foo> + 'static)>`
"#]];
    let renderer = Renderer::plain().term_width(40);
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0271]: type mismatch resolving `<Result<Result<(), Result<Result<(), Result<Result<(), Option<{integer}>>, ...>>, ...>>, ...> as Future>::Error == Foo`
   ╭▸ $DIR/E0271.rs:20:5
   │
20 │ ┏     Box::new(
21 │ ┃         Ok::<_, ()>(
22 │ ┃             Err::<(), _>(
23 │ ┃                 Ok::<_, ()>(
   ┆ ┇
32 │ ┃     )
   │ ┗━━━━━┛ type mismatch resolving `<Result<Result<(), Result<Result<(), ...>, ...>>, ...> as Future>::Error == Foo`
   ╰╴
note: expected this to be `Foo`
   ╭▸ $DIR/E0271.rs:10:18
   │
10 │     type Error = E;
   │                  ━
   ╰ note: required for the cast from `Box<Result<Result<(), Result<Result<(), Result<Result<(), Option<{integer}>>, ()>>, ()>>, ()>>` to `Box<(dyn Future<Error = Foo> + 'static)>`
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn e0271_2() {
    let source = r#"
trait Future {
    type Error;
}

impl<T, E> Future for Result<T, E> {
    type Error = E;
}

impl<T> Future for Option<T> {
    type Error = ();
}

struct Foo;

fn foo() -> Box<dyn Future<Error=Foo>> {
    Box::new(
        Ok::<_, ()>(
            Err::<(), _>(
                Ok::<_, ()>(
                    Err::<(), _>(
                        Ok::<_, ()>(
                            Err::<(), _>(Some(5))
                        )
                    )
                )
            )
        )
    )
}
fn main() {
}
"#;

    let input = &[
        Level::ERROR
            .primary_title("type mismatch resolving `<Result<Result<(), Result<Result<(), Result<Result<(), Option<{integer}>>, ...>>, ...>>, ...> as Future>::Error == Foo`")
            .id("E0271")
            .element(Snippet::source(source)
                .line_start(4)
                .path("$DIR/E0271.rs")

                .annotation(
                    AnnotationKind::Primary
                        .span(208..510)
                        .label("type mismatch resolving `<Result<Result<(), Result<Result<(), ...>, ...>>, ...> as Future>::Error == Foo`"),
                )
            ),
        Level::NOTE.secondary_title("expected this to be `Foo`")
            .element(
                Snippet::source(source)
                    .line_start(4)
                    .path("$DIR/E0271.rs")

                    .annotation(AnnotationKind::Primary.span(89..90))
            ).element(
                Level::NOTE
                    .message("required for the cast from `Box<Result<Result<(), Result<Result<(), Result<Result<(), Option<{integer}>>, ()>>, ()>>, ()>>` to `Box<(dyn Future<Error = Foo> + 'static)>`")
            ).element(
                Level::NOTE.message("a second note"),
            )
    ];

    let expected_ascii = str![[r#"
error[E0271]: type mismatch resolving `<Result<Result<(), Result<Result<(), Result<Result<(), Option<{integer}>>, ...>>, ...>>, ...> as Future>::Error == Foo`
  --> $DIR/E0271.rs:20:5
   |
20 | /     Box::new(
21 | |         Ok::<_, ()>(
22 | |             Err::<(), _>(
23 | |                 Ok::<_, ()>(
...  |
32 | |     )
   | |_____^ type mismatch resolving `<Result<Result<(), Result<Result<(), ...>, ...>>, ...> as Future>::Error == Foo`
   |
note: expected this to be `Foo`
  --> $DIR/E0271.rs:10:18
   |
10 |     type Error = E;
   |                  ^
   = note: required for the cast from `Box<Result<Result<(), Result<Result<(), Result<Result<(), Option<{integer}>>, ()>>, ()>>, ()>>` to `Box<(dyn Future<Error = Foo> + 'static)>`
   = note: a second note
"#]];
    let renderer = Renderer::plain().term_width(40);
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0271]: type mismatch resolving `<Result<Result<(), Result<Result<(), Result<Result<(), Option<{integer}>>, ...>>, ...>>, ...> as Future>::Error == Foo`
   ╭▸ $DIR/E0271.rs:20:5
   │
20 │ ┏     Box::new(
21 │ ┃         Ok::<_, ()>(
22 │ ┃             Err::<(), _>(
23 │ ┃                 Ok::<_, ()>(
   ┆ ┇
32 │ ┃     )
   │ ┗━━━━━┛ type mismatch resolving `<Result<Result<(), Result<Result<(), ...>, ...>>, ...> as Future>::Error == Foo`
   ╰╴
note: expected this to be `Foo`
   ╭▸ $DIR/E0271.rs:10:18
   │
10 │     type Error = E;
   │                  ━
   ├ note: required for the cast from `Box<Result<Result<(), Result<Result<(), Result<Result<(), Option<{integer}>>, ()>>, ()>>, ()>>` to `Box<(dyn Future<Error = Foo> + 'static)>`
   ╰ note: a second note
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn long_e0308() {
    let source = r#"
mod a {
    // Force the "short path for unique types" machinery to trip up
    pub struct Atype;
    pub struct Btype;
    pub struct Ctype;
}

mod b {
    pub struct Atype<T, K>(T, K);
    pub struct Btype<T, K>(T, K);
    pub struct Ctype<T, K>(T, K);
}

use b::*;

fn main() {
    let x: Atype<
      Btype<
        Ctype<
          Atype<
            Btype<
              Ctype<
                Atype<
                  Btype<
                    Ctype<i32, i32>,
                    i32
                  >,
                  i32
                >,
                i32
              >,
              i32
            >,
            i32
          >,
          i32
        >,
        i32
      >,
      i32
    > = Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(
        Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(
            Ok("")
        ))))))))))))))))))))))))))))))
    )))))))))))))))))))))))))))))];
    //~^^^^^ ERROR E0308

    let _ = Some(Ok(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(
        Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(
            Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(Some(
                Some(Some(Some(Some(Some(Some(Some(Some(Some("")))))))))
            )))))))))))))))))
        ))))))))))))))))))
    ))))))))))))))))) == Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(
        Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(
            Ok(Ok(Ok(Ok(Ok(Ok(Ok("")))))))
        ))))))))))))))))))))))))))))))
    )))))))))))))))))))))))];
    //~^^^^^ ERROR E0308

    let x: Atype<
      Btype<
        Ctype<
          Atype<
            Btype<
              Ctype<
                Atype<
                  Btype<
                    Ctype<i32, i32>,
                    i32
                  >,
                  i32
                >,
                i32
              >,
              i32
            >,
            i32
          >,
          i32
        >,
        i32
      >,
      i32
    > = ();
    //~^ ERROR E0308

    let _: () = Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(
        Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(
            Ok(Ok(Ok(Ok(Ok(Ok(Ok("")))))))
        ))))))))))))))))))))))))))))))
    )))))))))))))))))))))))];
    //~^^^^^ ERROR E0308
}
"#;

    let input = &[Level::ERROR
        .primary_title("mismatched types")
        .id("E0308")
        .element(
            Snippet::source(source)
                .line_start(7)
                .path("$DIR/long-E0308.rs")

                .annotation(
                    AnnotationKind::Primary
                        .span(719..1001)
                        .label("expected `Atype<Btype<Ctype<..., i32>, i32>, i32>`, found `Result<Result<Result<..., _>, _>, _>`"),
                )
                .annotation(
                    AnnotationKind::Context
                        .span(293..716)
                        .label("expected due to this"),
                )
        ).element(
            Level::NOTE
                .message("expected struct `Atype<Btype<..., i32>, i32>`\n     found enum `Result<Result<..., _>, _>`")
        ).element(
            Level::NOTE
                .message("the full name for the type has been written to '$TEST_BUILD_DIR/$FILE.long-type-hash.txt'")
        ).element(
            Level::NOTE
                .message("consider using `--verbose` to print the full type name to the console")
                ,
        )];

    let expected_ascii = str![[r#"
error[E0308]: mismatched types
  --> $DIR/long-E0308.rs:48:9
   |
24 |        let x: Atype<
   |  _____________-
25 | |        Btype<
26 | |          Ctype<
27 | |            Atype<
...  |
47 | |        i32
48 | |      > = Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok...
   | | _____-___^
   | ||_____|
   |  |     expected due to this
49 |  |         Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok...
50 |  |             Ok("")
51 |  |         ))))))))))))))))))))))))))))))
52 |  |     )))))))))))))))))))))))))))))];
   |  |__________________________________^ expected `Atype<Btype<Ctype<..., i32>, i32>, i32>`, found `Result<Result<Result<..., _>, _>, _>`
   |
   = note: expected struct `Atype<Btype<..., i32>, i32>`
                found enum `Result<Result<..., _>, _>`
   = note: the full name for the type has been written to '$TEST_BUILD_DIR/$FILE.long-type-hash.txt'
   = note: consider using `--verbose` to print the full type name to the console
"#]];
    let renderer = Renderer::plain().term_width(60);
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0308]: mismatched types
   ╭▸ $DIR/long-E0308.rs:48:9
   │
24 │        let x: Atype<
   │ ┌─────────────┘
25 │ │        Btype<
26 │ │          Ctype<
27 │ │            Atype<
   ┆ ┆
47 │ │        i32
48 │ │      > = Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(O…
   │ │┏━━━━━│━━━┛
   │ └┃─────┤
   │  ┃     expected due to this
49 │  ┃         Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(Ok(O…
50 │  ┃             Ok("")
51 │  ┃         ))))))))))))))))))))))))))))))
52 │  ┃     )))))))))))))))))))))))))))))];
   │  ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛ expected `Atype<Btype<Ctype<..., i32>, i32>, i32>`, found `Result<Result<Result<..., _>, _>, _>`
   │
   ├ note: expected struct `Atype<Btype<..., i32>, i32>`
   │            found enum `Result<Result<..., _>, _>`
   ├ note: the full name for the type has been written to '$TEST_BUILD_DIR/$FILE.long-type-hash.txt'
   ╰ note: consider using `--verbose` to print the full type name to the console
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn highlighting() {
    let source = r#"
use core::pin::Pin;
use core::future::Future;
use core::any::Any;

fn query(_: fn(Box<(dyn Any + Send + '_)>) -> Pin<Box<(
    dyn Future<Output = Result<Box<(dyn Any + 'static)>, String>> + Send + 'static
)>>) {}

fn wrapped_fn<'a>(_: Box<(dyn Any + Send)>) -> Pin<Box<(
    dyn Future<Output = Result<Box<(dyn Any + 'static)>, String>> + Send + 'static
)>> {
    Box::pin(async { Err("nope".into()) })
}

fn main() {
    query(wrapped_fn);
}
"#;

    let input = &[Level::ERROR
        .primary_title("mismatched types")
        .id("E0308")
        .element(
            Snippet::source(source)
                .line_start(7)
                .path("$DIR/unicode-output.rs")

                .annotation(
                    AnnotationKind::Primary
                        .span(430..440)
                        .label("one type is more general than the other"),
                )
                .annotation(
                    AnnotationKind::Context
                        .span(424..429)
                        .label("arguments to this function are incorrect"),
                ),
        ).element(
            Level::NOTE
                .message("expected fn pointer `for<'a> fn(Box<(dyn Any + Send + 'a)>) -> Pin<_>`\n      found fn item `fn(Box<(dyn Any + Send + 'static)>) -> Pin<_> {wrapped_fn}`")
                ,
        ),
        Level::NOTE.secondary_title("function defined here")
            .element(
                Snippet::source(source)
                    .line_start(7)
                    .path("$DIR/unicode-output.rs")

                    .annotation(AnnotationKind::Primary.span(77..210))
                    .annotation(AnnotationKind::Context.span(71..76)),
            )];

    let expected_ascii = str![[r#"
error[E0308]: mismatched types
  --> $DIR/unicode-output.rs:23:11
   |
23 |     query(wrapped_fn);
   |     ----- ^^^^^^^^^^ one type is more general than the other
   |     |
   |     arguments to this function are incorrect
   |
   = note: expected fn pointer `for<'a> fn(Box<(dyn Any + Send + 'a)>) -> Pin<_>`
                 found fn item `fn(Box<(dyn Any + Send + 'static)>) -> Pin<_> {wrapped_fn}`
note: function defined here
  --> $DIR/unicode-output.rs:12:10
   |
12 |   fn query(_: fn(Box<(dyn Any + Send + '_)>) -> Pin<Box<(
   |  ____-----_^
13 | |     dyn Future<Output = Result<Box<(dyn Any + 'static)>, String>> + Send + 'static
14 | | )>>) {}
   | |___^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0308]: mismatched types
   ╭▸ $DIR/unicode-output.rs:23:11
   │
23 │     query(wrapped_fn);
   │     ┬──── ━━━━━━━━━━ one type is more general than the other
   │     │
   │     arguments to this function are incorrect
   │
   ╰ note: expected fn pointer `for<'a> fn(Box<(dyn Any + Send + 'a)>) -> Pin<_>`
                 found fn item `fn(Box<(dyn Any + Send + 'static)>) -> Pin<_> {wrapped_fn}`
note: function defined here
   ╭▸ $DIR/unicode-output.rs:12:10
   │
12 │   fn query(_: fn(Box<(dyn Any + Send + '_)>) -> Pin<Box<(
   │ ┏━━━━─────━┛
13 │ ┃     dyn Future<Output = Result<Box<(dyn Any + 'static)>, String>> + Send + 'static
14 │ ┃ )>>) {}
   ╰╴┗━━━┛
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

// This tests that an ellipsis is not inserted into Unicode text when a line
// wasn't actually trimmed.
//
// This is a regression test where `...` was inserted because the code wasn't
// properly accounting for the *rendered* length versus the length in bytes in
// all cases.
#[test]
fn unicode_cut_handling() {
    let source = "version = \"0.1.0\"\n# Ensure that the spans from toml handle utf-8 correctly\nauthors = [\n    { name = \"Z\u{351}\u{36b}\u{343}\u{36a}\u{302}\u{36b}\u{33d}\u{34f}\u{334}\u{319}\u{324}\u{31e}\u{349}\u{35a}\u{32f}\u{31e}\u{320}\u{34d}A\u{36b}\u{357}\u{334}\u{362}\u{335}\u{31c}\u{330}\u{354}L\u{368}\u{367}\u{369}\u{358}\u{320}G\u{311}\u{357}\u{30e}\u{305}\u{35b}\u{341}\u{334}\u{33b}\u{348}\u{34d}\u{354}\u{339}O\u{342}\u{30c}\u{30c}\u{358}\u{328}\u{335}\u{339}\u{33b}\u{31d}\u{333}\", email = 1 }\n]\n";
    let input = &[Level::ERROR.primary_title("title").element(
        Snippet::source(source)
            .fold(false)
            .annotation(AnnotationKind::Primary.span(85..228).label("annotation")),
    )];
    let expected_ascii = str![[r#"
error: title
  |
1 |   version = "0.1.0"
2 |   # Ensure that the spans from toml handle utf-8 correctly
3 |   authors = [
  |  ___________^
4 | |     { name = "Z͑ͫ̓ͪ̂ͫ̽͏̴̙̤̞͉͚̯̞̠͍A̴̵̜̰͔ͫ͗͢L̠ͨͧͩ͘G̴̻͈͍͔̹̑͗̎̅͛́Ǫ̵̹̻̝̳͂̌̌͘", email = 1 }
5 | | ]
  | |_^ annotation
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: title
  ╭▸ 
1 │   version = "0.1.0"
2 │   # Ensure that the spans from toml handle utf-8 correctly
3 │   authors = [
  │ ┏━━━━━━━━━━━┛
4 │ ┃     { name = "Z͑ͫ̓ͪ̂ͫ̽͏̴̙̤̞͉͚̯̞̠͍A̴̵̜̰͔ͫ͗͢L̠ͨͧͩ͘G̴̻͈͍͔̹̑͗̎̅͛́Ǫ̵̹̻̝̳͂̌̌͘", email = 1 }
5 │ ┃ ]
  ╰╴┗━┛ annotation
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn trim_unicode_annotate_ascii_end_with_label() {
    let source = "/*这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。*/?";
    let input = &[Group::with_level(Level::ERROR).element(
        Snippet::source(source).annotation(
            AnnotationKind::Primary
                .span(499..500)
                .label("expected item"),
        ),
    )];

    let expected_ascii = str![[r#"
  |
1 | ... 的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。*/?
  |                                                             ^ expected item
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
  ╭▸ 
1 │ … 宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。*/?
  ╰╴                                                            ━ expected item
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn trim_unicode_annotate_ascii_end_no_label() {
    let source = "/*这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。*/?";
    let input = &[Group::with_level(Level::ERROR)
        .element(Snippet::source(source).annotation(AnnotationKind::Primary.span(499..500)))];

    let expected_ascii = str![[r#"
  |
1 | ... 这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。*/?
  |                                                                   ^
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
  ╭▸ 
1 │ … 。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。*/?
  ╰╴                                                                  ━
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn trim_unicode_annotate_unicode_end_with_label() {
    let source = "/*这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。*/好";
    let input = &[Group::with_level(Level::ERROR).element(
        Snippet::source(source).annotation(
            AnnotationKind::Primary
                .span(499..502)
                .label("expected item"),
        ),
    )];

    let expected_ascii = str![[r#"
  |
1 | ... 的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。*/好
  |                                                             ^^ expected item
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
  ╭▸ 
1 │ … 宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。*/好
  ╰╴                                                            ━━ expected item
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn trim_unicode_annotate_unicode_end_no_label() {
    let source = "/*这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。*/好";
    let input = &[Group::with_level(Level::ERROR)
        .element(Snippet::source(source).annotation(AnnotationKind::Primary.span(499..502)))];

    let expected_ascii = str![[r#"
  |
1 | ... 这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。*/好
  |                                                                   ^^
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
  ╭▸ 
1 │ … 。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。*/好
  ╰╴                                                                  ━━
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn trim_unicode_annotate_unicode_middle_with_label() {
    let source = "/*这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。*/?";
    let input = &[Group::with_level(Level::ERROR).element(
        Snippet::source(source).annotation(
            AnnotationKind::Primary
                .span(251..254)
                .label("expected item"),
        ),
    )];

    let expected_ascii = str![[r#"
  |
1 | ... 这是宽的。这是宽的。这是宽的。...
  |           ^^ expected item
"#]];

    let renderer = Renderer::plain().term_width(43);
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
  ╭▸ 
1 │ … 。这是宽的。这是宽的。这是宽的。这…
  ╰╴          ━━ expected item
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn trim_unicode_annotate_unicode_middle_no_label() {
    let source = "/*这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。这是宽的。*/?";
    let input = &[Group::with_level(Level::ERROR)
        .element(Snippet::source(source).annotation(AnnotationKind::Primary.span(251..254)))];

    let expected_ascii = str![[r#"
  |
1 | ... 是宽的。这是宽的。这是宽的。这...
  |                   ^^
"#]];

    let renderer = Renderer::plain().term_width(43);
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
  ╭▸ 
1 │ … 这是宽的。这是宽的。这是宽的。这是…
  ╰╴                  ━━
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn trim_ascii_annotate_ascii_end_with_label() {
    let source = "/*aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa*/?";
    let input = &[Group::with_level(Level::ERROR).element(
        Snippet::source(source).annotation(
            AnnotationKind::Primary
                .span(334..335)
                .label("expected item"),
        ),
    )];

    let expected_ascii = str![[r#"
  |
1 | ...aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa*/?
  |                                                             ^ expected item
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
  ╭▸ 
1 │ …aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa*/?
  ╰╴                                                            ━ expected item
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn trim_ascii_annotate_ascii_end_no_label() {
    let source = "/*aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa*/?";
    let input = &[Group::with_level(Level::ERROR)
        .element(Snippet::source(source).annotation(AnnotationKind::Primary.span(334..335)))];

    let expected_ascii = str![[r#"
  |
1 | ...aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa*/?
  |                                                                    ^
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
  ╭▸ 
1 │ …aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa*/?
  ╰╴                                                                   ━
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn diagnostic_width() {
    let source = r##"// ignore-tidy-linelength

fn main() {
    let _: &str = "🦀☀☁☂☃☄★☆☇☈☉☊☋☌☍☎☏☐☑☒☓  ☖☗☘☙☚☛☜☝☞☟☠☡☢☣☤☥☦☧☨☩☪☫☬☭☮☯☰☱☲☳☴☵☶☷☸☹☺☻☼☽☾☿♀♁♂♃♄♅♆♇♏♔♕♖♗♘♙♚♛♜♝♞♟♠♡♢♣♤♥♦♧♨♩♪♫♬♭♮♯♰♱♲♳♴♵♶♷♸♹♺♻♼♽♾♿⚀⚁⚂⚃⚄⚅⚆⚈⚉4🦀☀☁☂☃☄★☆☇☈☉☊☋☌☍☎☏☐☑☒☓☖☗☘☙☚☛☜☝☞☟☠☡☢☣☤☥☦☧☨☩☪☫☬☭☮☯☰☱☲☳☴☵☶☷☸☹☺☻☼☽☾☿♀♁♂♃♄♅♆♇♏♔♕♖♗♘♙♚♛♜♝♞♟♠♡♢♣♤♥♦♧♨♩♪♫♬♭♮♯♰♱♲♳♴♵♶♷♸♹♺♻♼♽♾♿⚀⚁⚂⚃⚄⚅⚆⚈⚉4🦀🦀☁☂☃☄★☆☇☈☉☊☋☌☍☎☏☐☑☒☓☖☗☘☙☚☛☜☝☞☟☠☡☢☣☤☥☦☧☨☩☪☫☬☭☮☯☰☱☲☳☴☵☶☷☸☹☺☻☼☽☾☿♀♁♂♃♄♅♆♇♏♔♕♖♗♘♙♚♛♜♝♞♟♠♡♢♣♤♥♦♧♨♩♪♫♬♭♮♯♰♱♲♳♴♵♶♷♸♹♺♻♼♽♾♿⚀⚁⚂⚃⚄⚅⚆⚈⚉4"; let _: () = 42;  let _: &str = "🦀☀☁☂☃☄★☆☇☈☉☊☋☌☍☎☏☐☑☒☓  ☖☗☘☙☚☛☜☝☞☟☠☡☢☣☤☥☦☧☨☩☪☫☬☭☮☯☰☱☲☳☴☵☶☷☸☹☺☻☼☽☾☿♀♁♂♃♄♅♆♇♏♔♕♖♗♘♙♚♛♜♝♞♟♠♡♢♣♤♥♦♧♨♩♪♫♬♭♮♯♰♱♲♳♴♵♶♷♸♹♺♻♼♽♾♿⚀⚁⚂⚃⚄⚅⚆⚈⚉4🦀☀☁☂☃☄★☆☇☈☉☊☋☌☍☎☏☐☑☒☓☖☗☘☙☚☛☜☝☞☟☠☡☢☣☤☥☦☧☨☩☪☫☬☭☮☯☰☱☲☳☴☵☶☷☸☹☺☻☼☽☾☿♀♁♂♃♄♅♆♇♏♔♕♖♗♘♙♚♛♜♝♞♟♠♡♢♣♤♥♦♧♨♩♪♫♬♭♮♯♰♱♲♳♴♵♶♷♸♹♺♻♼♽♾♿⚀⚁⚂⚃⚄⚅⚆⚈⚉4🦀🦀☁☂☃☄★☆☇☈☉☊☋☌☍☎☏☐☑☒☓☖☗☘☙☚☛☜☝☞☟☠☡☢☣☤☥☦☧☨☩☪☫☬☭☮☯☰☱☲☳☴☵☶☷☸☹☺☻☼☽☾☿♀♁♂♃♄♅♆♇♏♔♕♖♗♘♙♚♛♜♝♞♟♠♡♢♣♤♥♦♧♨♩♪♫♬♭♮♯♰♱♲♳♴♵♶♷♸♹♺♻♼♽♾♿⚀⚁⚂⚃⚄⚅⚆⚈⚉4";
//~^ ERROR mismatched types
}
"##;
    let input = &[Level::ERROR
        .primary_title("mismatched types")
        .id("E0308")
        .element(
            Snippet::source(source)
                .path("$DIR/non-whitespace-trimming-unicode.rs")
                .annotation(
                    AnnotationKind::Primary
                        .span(1207..1209)
                        .label("expected `()`, found integer"),
                )
                .annotation(
                    AnnotationKind::Context
                        .span(1202..1204)
                        .label("expected due to this"),
                ),
        )];

    let expected_ascii = str![[r#"
error[E0308]: mismatched types
 --> $DIR/non-whitespace-trimming-unicode.rs:4:415
  |
4 | ...♦♧♨♩♪♫♬♭♮♯♰♱♲♳♴♵♶♷♸♹♺♻♼♽♾♿⚀⚁⚂⚃⚄⚅⚆⚈⚉4"; let _: () = 42;  let _: &str = "🦀☀☁☂☃☄★☆☇☈☉☊☋☌☍☎☏☐☑☒☓  ☖☗☘☙☚☛☜☝☞☟☠☡☢☣☤☥☦☧☨☩☪☫☬☭☮☯☰☱☲...
  |                                                   --   ^^ expected `()`, found integer
  |                                                   |
  |                                                   expected due to this
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0308]: mismatched types
  ╭▸ $DIR/non-whitespace-trimming-unicode.rs:4:415
  │
4 │ …♤♥♦♧♨♩♪♫♬♭♮♯♰♱♲♳♴♵♶♷♸♹♺♻♼♽♾♿⚀⚁⚂⚃⚄⚅⚆⚈⚉4"; let _: () = 42;  let _: &str = "🦀☀☁☂☃☄★☆☇☈☉☊☋☌☍☎☏☐☑☒☓  ☖☗☘☙☚☛☜☝☞☟☠☡☢☣☤☥☦☧☨☩☪☫☬☭☮☯☰☱☲☳…
  │                                                   ┬─   ━━ expected `()`, found integer
  │                                                   │
  ╰╴                                                  expected due to this
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn diagnostic_width2() {
    let source = r##"//@ revisions: ascii unicode
//@[unicode] compile-flags: -Zunstable-options --error-format=human-unicode
// ignore-tidy-linelength

fn main() {
    let unicode_is_fun = "؁‱ஹ௸௵꧄.ဪ꧅⸻𒈙𒐫﷽𒌄𒈟𒍼𒁎𒀱𒌧𒅃 𒈓𒍙𒊎𒄡𒅌𒁏𒀰𒐪𒐩𒈙𒐫𪚥";
    let _ = "ༀ༁༂༃༄༅༆༇༈༉༊་༌།༎༏༐༑༒༓༔༕༖༗༘༙༚༛༜༝༞༟༠༡༢༣༤༥༦༧༨༩༪༫༬༭༮༯༰༱༲༳༴༵༶༷༸༹༺༻༼༽༾༿ཀཁགགྷངཅཆཇ཈ཉཊཋཌཌྷཎཏཐདདྷནཔཕབབྷམཙཚཛཛྷཝཞཟའཡརལཤཥསཧཨཀྵཪཫཬ཭཮཯཰ཱཱཱིིུུྲྀཷླྀཹེཻོཽཾཿ྄ཱྀྀྂྃ྅྆྇ྈྉྊྋྌྍྎྏྐྑྒྒྷྔྕྖྗ྘ྙྚྛྜྜྷྞྟྠྡྡྷྣྤྥྦྦྷྨྩྪྫྫྷྭྮྯྰྱྲླྴྵྶྷྸྐྵྺྻྼ྽྾྿࿀࿁࿂࿃࿄࿅࿆࿇࿈࿉࿊࿋࿌࿍࿎࿏࿐࿑࿒࿓࿔࿕࿖࿗࿘࿙࿚"; let _a = unicode_is_fun + " really fun!";
    //[ascii]~^ ERROR cannot add `&str` to `&str`
}
"##;
    let input = &[
        Level::ERROR
            .primary_title("cannot add `&str` to `&str`")
            .id("E0369")
            .element(
                Snippet::source(source)
                    .path("$DIR/non-1-width-unicode-multiline-label.rs")
                    .annotation(AnnotationKind::Context.span(970..984).label("&str"))
                    .annotation(AnnotationKind::Context.span(987..1001).label("&str"))
                    .annotation(
                        AnnotationKind::Primary
                            .span(985..986)
                            .label("`+` cannot be used to concatenate two `&str` strings"),
                    ),
            )
            .element(
                Level::NOTE.message("string concatenation requires an owned `String` on the left"),
            ),
        Level::HELP
            .secondary_title("create an owned `String` from a string reference")
            .element(
                Snippet::source(source)
                    .path("$DIR/non-1-width-unicode-multiline-label.rs")
                    .patch(Patch::new(984..984, ".to_owned()")),
            ),
    ];

    let expected_ascii = str![[r#"
error[E0369]: cannot add `&str` to `&str`
 --> $DIR/non-1-width-unicode-multiline-label.rs:7:260
  |
7 | ...࿉࿊࿋࿌࿍࿎࿏࿐࿑࿒࿓࿔࿕࿖࿗࿘࿙࿚"; let _a = unicode_is_fun + " really fun!";
  |                                  -------------- ^ -------------- &str
  |                                  |              |
  |                                  |              `+` cannot be used to concatenate two `&str` strings
  |                                  &str
  |
  = note: string concatenation requires an owned `String` on the left
help: create an owned `String` from a string reference
  |
7 |     let _ = "ༀ༁༂༃༄༅༆༇༈༉༊་༌།༎༏༐༑༒༓༔༕༖༗༘༙༚༛༜༝༞༟༠༡༢༣༤༥༦༧༨༩༪༫༬༭༮༯༰༱༲༳༴༵༶༷༸༹༺༻༼༽༾༿ཀཁགགྷངཅཆཇ཈ཉཊཋཌཌྷཎཏཐདདྷནཔཕབབྷམཙཚཛཛྷཝཞཟའཡརལཤཥསཧཨཀྵཪཫཬ཭཮཯཰ཱཱཱིིུུྲྀཷླྀཹེཻོཽཾཿ྄ཱྀྀྂྃ྅྆྇ྈྉྊྋྌྍྎྏྐྑྒྒྷྔྕྖྗ྘ྙྚྛྜྜྷྞྟྠྡྡྷྣྤྥྦྦྷྨྩྪྫྫྷྭྮྯྰྱྲླྴྵྶྷྸྐྵྺྻྼ྽྾྿࿀࿁࿂࿃࿄࿅࿆࿇࿈࿉࿊࿋࿌࿍࿎࿏࿐࿑࿒࿓࿔࿕࿖࿗࿘࿙࿚"; let _a = unicode_is_fun.to_owned() + " really fun!";
  |                                                                                                                                                                                         +++++++++++
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0369]: cannot add `&str` to `&str`
  ╭▸ $DIR/non-1-width-unicode-multiline-label.rs:7:260
  │
7 │ …࿆࿇࿈࿉࿊࿋࿌࿍࿎࿏࿐࿑࿒࿓࿔࿕࿖࿗࿘࿙࿚"; let _a = unicode_is_fun + " really fun!";
  │                                  ┬───────────── ┯ ────────────── &str
  │                                  │              │
  │                                  │              `+` cannot be used to concatenate two `&str` strings
  │                                  &str
  │
  ╰ note: string concatenation requires an owned `String` on the left
help: create an owned `String` from a string reference
  ╭╴
7 │     let _ = "ༀ༁༂༃༄༅༆༇༈༉༊་༌།༎༏༐༑༒༓༔༕༖༗༘༙༚༛༜༝༞༟༠༡༢༣༤༥༦༧༨༩༪༫༬༭༮༯༰༱༲༳༴༵༶༷༸༹༺༻༼༽༾༿ཀཁགགྷངཅཆཇ཈ཉཊཋཌཌྷཎཏཐདདྷནཔཕབབྷམཙཚཛཛྷཝཞཟའཡརལཤཥསཧཨཀྵཪཫཬ཭཮཯཰ཱཱཱིིུུྲྀཷླྀཹེཻོཽཾཿ྄ཱྀྀྂྃ྅྆྇ྈྉྊྋྌྍྎྏྐྑྒྒྷྔྕྖྗ྘ྙྚྛྜྜྷྞྟྠྡྡྷྣྤྥྦྦྷྨྩྪྫྫྷྭྮྯྰྱྲླྴྵྶྷྸྐྵྺྻྼ྽྾྿࿀࿁࿂࿃࿄࿅࿆࿇࿈࿉࿊࿋࿌࿍࿎࿏࿐࿑࿒࿓࿔࿕࿖࿗࿘࿙࿚"; let _a = unicode_is_fun.to_owned() + " really fun!";
  ╰╴                                                                                                                                                                                        +++++++++++
"#]];

    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn macros_not_utf8() {
    let source = r##"//@ error-pattern: did not contain valid UTF-8
//@ reference: input.encoding.utf8
//@ reference: input.encoding.invalid

fn foo() {
    include!("not-utf8.bin");
}
"##;
    let bin_source = "�|�\u{0002}!5�cc\u{0015}\u{0002}�Ӻi��WWj�ȥ�'�}�\u{0012}�J�ȉ��W�\u{001e}O�@����\u{001c}w�V���LO����\u{0014}[ \u{0003}_�'���SQ�~ذ��ų&��-\t��lN~��!@␌ _#���kQ��h�\u{001d}�:�\u{001c}\u{0007}�";
    let input = &[Level::ERROR
        .primary_title("couldn't read `$DIR/not-utf8.bin`: stream did not contain valid UTF-8").element(
                Snippet::source(source)
                    .path("$DIR/not-utf8.rs")

                    .annotation(AnnotationKind::Primary.span(136..160)),
            ),
            Level::NOTE.secondary_title("byte `193` is not valid utf-8")
                .element(
                    Snippet::source(bin_source)
                        .path("$DIR/not-utf8.bin")

                        .annotation(AnnotationKind::Primary.span(0..0)),
                )
                .element(Level::NOTE.message("this error originates in the macro `include` (in Nightly builds, run with -Z macro-backtrace for more info)")),
       ];

    let expected_ascii = str![[r#"
error: couldn't read `$DIR/not-utf8.bin`: stream did not contain valid UTF-8
 --> $DIR/not-utf8.rs:6:5
  |
6 |     include!("not-utf8.bin");
  |     ^^^^^^^^^^^^^^^^^^^^^^^^
  |
note: byte `193` is not valid utf-8
 --> $DIR/not-utf8.bin:1:1
  |
1 | �|�␂!5�cc␕␂�Ӻi��WWj�ȥ�'�}�␒�J�ȉ��W�␞O�@����␜w�V���LO����␔[ ␃_�'���SQ�~ذ��ų&��-    ��lN~��!@␌ _#���kQ��h�␝�:�␜␇�
  | ^
  = note: this error originates in the macro `include` (in Nightly builds, run with -Z macro-backtrace for more info)
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: couldn't read `$DIR/not-utf8.bin`: stream did not contain valid UTF-8
  ╭▸ $DIR/not-utf8.rs:6:5
  │
6 │     include!("not-utf8.bin");
  │     ━━━━━━━━━━━━━━━━━━━━━━━━
  ╰╴
note: byte `193` is not valid utf-8
  ╭▸ $DIR/not-utf8.bin:1:1
  │
1 │ �|�␂!5�cc␕␂�Ӻi��WWj�ȥ�'�}�␒�J�ȉ��W�␞O�@����␜w�V���LO����␔[ ␃_�'���SQ�~ذ��ų&��-    ��lN~��!@␌ _#���kQ��h�␝�:�␜␇�
  │ ━
  ╰ note: this error originates in the macro `include` (in Nightly builds, run with -Z macro-backtrace for more info)
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn secondary_title_no_level_text() {
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
            Level::NOTE
                .no_name()
                .message("expected reference `&str`\nfound reference `&'static [u8; 0]`"),
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
  = expected reference `&str`
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
  ╰ expected reference `&str`
    found reference `&'static [u8; 0]`
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn secondary_title_custom_level_text() {
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
            Level::NOTE
                .with_name(Some("custom"))
                .message("expected reference `&str`\nfound reference `&'static [u8; 0]`"),
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
  = custom: expected reference `&str`
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
  ╰ custom: expected reference `&str`
            found reference `&'static [u8; 0]`
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn id_on_title() {
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
            .with_name(Some("suggestion"))
            .secondary_title("use `break` on its own without a value inside this `while` loop")
            .id("S0123")
            .element(
                Snippet::source(source)
                    .line_start(1)
                    .path("$DIR/issue-114529-illegal-break-with-value.rs")
                    .patch(Patch::new(483..581, "break")),
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
suggestion[S0123]: use `break` on its own without a value inside this `while` loop
   |
22 -         break (|| { //~ ERROR `break` with value from a `while` loop
23 -             let local = 9;
24 -         });
22 +         break;
   |
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
suggestion[S0123]: use `break` on its own without a value inside this `while` loop
   ╭╴
22 -         break (|| { //~ ERROR `break` with value from a `while` loop
23 -             let local = 9;
24 -         });
22 +         break;
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn max_line_num_no_fold() {
    let source = r#"cargo
fuzzy
pizza
jumps
crazy
quack
zappy
"#;

    let input = &[Level::ERROR
        .primary_title("the size for values of type `T` cannot be known at compilation time")
        .id("E0277")
        .element(
            Snippet::source(source)
                .line_start(8)
                .fold(false)
                .annotation(AnnotationKind::Primary.span(6..11)),
        )];
    let expected_ascii = str![[r#"
error[E0277]: the size for values of type `T` cannot be known at compilation time
   |
 8 | cargo
 9 | fuzzy
   | ^^^^^
10 | pizza
11 | jumps
12 | crazy
13 | quack
14 | zappy
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0277]: the size for values of type `T` cannot be known at compilation time
   ╭▸ 
 8 │ cargo
 9 │ fuzzy
   │ ━━━━━
10 │ pizza
11 │ jumps
12 │ crazy
13 │ quack
14 │ zappy
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn empty_span_start_line() {
    let source = "#: E112\nif False:\nprint()\n#: E113\nprint()\n";
    let input = &[Group::with_level(Level::ERROR).element(
        Snippet::source(source)
            .line_start(7)
            .fold(false)
            .annotation(AnnotationKind::Primary.span(18..18).label("E112")),
    )];

    let expected_ascii = str![[r#"
   |
 7 | #: E112
 8 | if False:
 9 | print()
   | ^ E112
10 | #: E113
11 | print()
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
   ╭▸ 
 7 │ #: E112
 8 │ if False:
 9 │ print()
   │ ━ E112
10 │ #: E113
11 │ print()
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn suggestion_span_line_end() {
    let source = r#"#![allow(unused)]
fn main() {
[1, 2, 3].into_iter().for_each(|n| { *n; });
}
"#;

    let long_title1 = "this method call resolves to `<&[T; N] as IntoIterator>::into_iter` (due to backwards compatibility), but will resolve to `<[T; N] as IntoIterator>::into_iter` in Rust 2021";
    let long_title2 = "for more information, see <https://doc.rust-lang.org/nightly/edition-guide/rust-2021/IntoIterator-for-arrays.html>";
    let long_title3 = "or use `IntoIterator::into_iter(..)` instead of `.into_iter()` to explicitly iterate by value";

    let input = &[
        Level::WARNING
            .primary_title(long_title1)
            .element(
                Snippet::source(source)
                    .path("lint_example.rs")
                    .annotation(AnnotationKind::Primary.span(40..49)),
            )
            .element(Level::WARNING.message("this changes meaning in Rust 2021"))
            .element(Level::NOTE.message(long_title2))
            .element(Level::NOTE.message("`#[warn(array_into_iter)]` on by default")),
        Level::HELP
            .secondary_title("use `.iter()` instead of `.into_iter()` to avoid ambiguity")
            .element(
                Snippet::source(source)
                    .path("lint_example.rs")
                    .line_start(3)
                    .patch(Patch::new(40..49, "iter")),
            ),
        Level::HELP.secondary_title(long_title3).element(
            Snippet::source(source)
                .path("lint_example.rs")
                .line_start(3)
                .patch(Patch::new(74..74, " // Span after line end")),
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
5 - [1, 2, 3].into_iter().for_each(|n| { *n; });
5 + [1, 2, 3].iter().for_each(|n| { *n; });
  |
help: or use `IntoIterator::into_iter(..)` instead of `.into_iter()` to explicitly iterate by value
  |
5 | [1, 2, 3].into_iter().for_each(|n| { *n; }); // Span after line end
  |                                              ++++++++++++++++++++++
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
5 - [1, 2, 3].into_iter().for_each(|n| { *n; });
5 + [1, 2, 3].iter().for_each(|n| { *n; });
  ╰╴
help: or use `IntoIterator::into_iter(..)` instead of `.into_iter()` to explicitly iterate by value
  ╭╴
5 │ [1, 2, 3].into_iter().for_each(|n| { *n; }); // Span after line end
  ╰╴                                             ++++++++++++++++++++++
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn suggestion_span_source_end() {
    let snippet_source = r#"#![allow(unused)]
fn main() {
[1, 2, 3].into_iter().for_each(|n| { *n; });
}
"#;

    let suggestion_source = r#"[1, 2, 3].into_iter().for_each(|n| { *n; });
"#;

    let long_title1 = "this method call resolves to `<&[T; N] as IntoIterator>::into_iter` (due to backwards compatibility), but will resolve to `<[T; N] as IntoIterator>::into_iter` in Rust 2021";
    let long_title2 = "for more information, see <https://doc.rust-lang.org/nightly/edition-guide/rust-2021/IntoIterator-for-arrays.html>";
    let long_title3 = "or use `IntoIterator::into_iter(..)` instead of `.into_iter()` to explicitly iterate by value";

    let input = &[
        Level::WARNING
            .primary_title(long_title1)
            .element(
                Snippet::source(snippet_source)
                    .path("lint_example.rs")
                    .annotation(AnnotationKind::Primary.span(40..49)),
            )
            .element(Level::WARNING.message("this changes meaning in Rust 2021"))
            .element(Level::NOTE.message(long_title2))
            .element(Level::NOTE.message("`#[warn(array_into_iter)]` on by default")),
        Level::HELP
            .secondary_title("use `.iter()` instead of `.into_iter()` to avoid ambiguity")
            .element(
                Snippet::source(suggestion_source)
                    .path("lint_example.rs")
                    .line_start(3)
                    .patch(Patch::new(10..19, "iter")),
            ),
        Level::HELP.secondary_title(long_title3).element(
            Snippet::source(suggestion_source)
                .path("lint_example.rs")
                .line_start(3)
                .patch(Patch::new(
                    suggestion_source.len()..suggestion_source.len(),
                    " // Span after line end",
                )),
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
3 | [1, 2, 3].into_iter().for_each(|n| { *n; }); // Span after line end
  |                                              ++++++++++++++++++++++
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
3 │ [1, 2, 3].into_iter().for_each(|n| { *n; }); // Span after line end
  ╰╴                                             ++++++++++++++++++++++
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn suggestion_span_one_bigger_than_source() {
    let snippet_source = r#"#![allow(unused)]
fn main() {
[1, 2, 3].into_iter().for_each(|n| { *n; });
}
"#;

    let suggestion_source = r#"[1, 2, 3].into_iter().for_each(|n| { *n; });
"#;

    let long_title1 = "this method call resolves to `<&[T; N] as IntoIterator>::into_iter` (due to backwards compatibility), but will resolve to `<[T; N] as IntoIterator>::into_iter` in Rust 2021";
    let long_title2 = "for more information, see <https://doc.rust-lang.org/nightly/edition-guide/rust-2021/IntoIterator-for-arrays.html>";
    let long_title3 = "or use `IntoIterator::into_iter(..)` instead of `.into_iter()` to explicitly iterate by value";

    let input = &[
        Level::WARNING
            .primary_title(long_title1)
            .element(
                Snippet::source(snippet_source)
                    .path("lint_example.rs")
                    .annotation(AnnotationKind::Primary.span(40..49)),
            )
            .element(Level::WARNING.message("this changes meaning in Rust 2021"))
            .element(Level::NOTE.message(long_title2))
            .element(Level::NOTE.message("`#[warn(array_into_iter)]` on by default")),
        Level::HELP
            .secondary_title("use `.iter()` instead of `.into_iter()` to avoid ambiguity")
            .element(
                Snippet::source(suggestion_source)
                    .path("lint_example.rs")
                    .line_start(3)
                    .patch(Patch::new(10..19, "iter")),
            ),
        Level::HELP.secondary_title(long_title3).element(
            Snippet::source(suggestion_source)
                .path("lint_example.rs")
                .line_start(3)
                .patch(Patch::new(
                    suggestion_source.len() + 1..suggestion_source.len() + 1,
                    " // Span after line end",
                )),
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
3 | [1, 2, 3].into_iter().for_each(|n| { *n; }); // Span after line end
  |                                              ++++++++++++++++++++++
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
3 │ [1, 2, 3].into_iter().for_each(|n| { *n; }); // Span after line end
  ╰╴                                             ++++++++++++++++++++++
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
#[should_panic = "Patch span `47..47` is beyond the end of buffer `45`"]
fn suggestion_span_bigger_than_source() {
    let snippet_source = r#"#![allow(unused)]
fn main() {
[1, 2, 3].into_iter().for_each(|n| { *n; });
}
"#;
    let suggestion_source = r#"[1, 2, 3].into_iter().for_each(|n| { *n; });
"#;

    let long_title1 = "this method call resolves to `<&[T; N] as IntoIterator>::into_iter` (due to backwards compatibility), but will resolve to `<[T; N] as IntoIterator>::into_iter` in Rust 2021";
    let long_title2 = "for more information, see <https://doc.rust-lang.org/nightly/edition-guide/rust-2021/IntoIterator-for-arrays.html>";
    let long_title3 = "or use `IntoIterator::into_iter(..)` instead of `.into_iter()` to explicitly iterate by value";

    let input = &[
        Level::WARNING
            .primary_title(long_title1)
            .element(
                Snippet::source(snippet_source)
                    .path("lint_example.rs")
                    .annotation(AnnotationKind::Primary.span(40..49)),
            )
            .element(Level::WARNING.message("this changes meaning in Rust 2021"))
            .element(Level::NOTE.message(long_title2))
            .element(Level::NOTE.message("`#[warn(array_into_iter)]` on by default")),
        Level::HELP
            .secondary_title("use `.iter()` instead of `.into_iter()` to avoid ambiguity")
            .element(
                Snippet::source(suggestion_source)
                    .path("lint_example.rs")
                    .line_start(3)
                    .patch(Patch::new(10..19, "iter")),
            ),
        Level::HELP.secondary_title(long_title3).element(
            Snippet::source(suggestion_source)
                .path("lint_example.rs")
                .line_start(3)
                .patch(Patch::new(
                    suggestion_source.len() + 2..suggestion_source.len() + 2,
                    " // Span after line end",
                )),
        ),
    ];

    let renderer = Renderer::plain();
    renderer.render(input);
}

#[test]
fn snippet_no_path() {
    // Taken from: https://docs.python.org/3/library/typing.html#annotating-callable-objects

    let source = "def __call__(self, *vals: bytes, maxlen: int | None = None) -> list[bytes]: ...";
    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source).annotation(AnnotationKind::Primary.span(4..12).label("annotation")),
    )];

    let expected_ascii = str![[r#"
error: 
  |
1 | def __call__(self, *vals: bytes, maxlen: int | None = None) -> list[bytes]: ...
  |     ^^^^^^^^ annotation
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ 
1 │ def __call__(self, *vals: bytes, maxlen: int | None = None) -> list[bytes]: ...
  ╰╴    ━━━━━━━━ annotation
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn multiple_snippet_no_path() {
    // Taken from: https://docs.python.org/3/library/typing.html#annotating-callable-objects

    let source = "def __call__(self, *vals: bytes, maxlen: int | None = None) -> list[bytes]: ...";
    let input = &[Level::ERROR
        .primary_title("")
        .element(
            Snippet::source(source)
                .annotation(AnnotationKind::Primary.span(4..12).label("annotation")),
        )
        .element(
            Snippet::source(source)
                .annotation(AnnotationKind::Primary.span(4..12).label("annotation")),
        )];

    let expected_ascii = str![[r#"
error: 
  |
1 | def __call__(self, *vals: bytes, maxlen: int | None = None) -> list[bytes]: ...
  |     ^^^^^^^^ annotation
  |
 ::: 
1 | def __call__(self, *vals: bytes, maxlen: int | None = None) -> list[bytes]: ...
  |     ^^^^^^^^ annotation
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: 
  ╭▸ 
1 │ def __call__(self, *vals: bytes, maxlen: int | None = None) -> list[bytes]: ...
  │     ━━━━━━━━ annotation
  │
  ⸬  
1 │ def __call__(self, *vals: bytes, maxlen: int | None = None) -> list[bytes]: ...
  ╰╴    ━━━━━━━━ annotation
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn padding_last_in_group() {
    let source = r#"// When the type of a method call's receiver is unknown, the span should point
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
                .path("$DIR/issue-42234-unknown-receiver-type.rs")
                .annotation(AnnotationKind::Primary.span(449..452).label(
                    "cannot infer type of the type parameter `S` declared on the method `sum`",
                )),
        )
        .element(Padding)];

    let expected_ascii = str![[r#"
error[E0282]: type annotations needed
  --> $DIR/issue-42234-unknown-receiver-type.rs:12:10
   |
12 |         .sum::<_>() //~ ERROR type annotations needed
   |          ^^^ cannot infer type of the type parameter `S` declared on the method `sum`
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0282]: type annotations needed
   ╭▸ $DIR/issue-42234-unknown-receiver-type.rs:12:10
   │
12 │         .sum::<_>() //~ ERROR type annotations needed
   │          ━━━ cannot infer type of the type parameter `S` declared on the method `sum`
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn padding_last_in_group_with_group_after() {
    let source = r#"// When the type of a method call's receiver is unknown, the span should point
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

    let input = &[
        Level::ERROR
            .primary_title("type annotations needed")
            .id("E0282")
            .element(
                Snippet::source(source)
                    .path("$DIR/issue-42234-unknown-receiver-type.rs")
                    .annotation(AnnotationKind::Primary.span(449..452).label(
                        "cannot infer type of the type parameter `S` declared on the method `sum`",
                    )),
            )
            .element(Padding),
        Level::HELP
            .secondary_title("consider specifying the generic argument")
            .element(
                Snippet::source(source)
                    .path("$DIR/issue-42234-unknown-receiver-type.rs")
                    .line_start(12)
                    .fold(true)
                    .patch(Patch::new(452..457, "::<GENERIC_ARG>")),
            ),
    ];

    let expected_ascii = str![[r#"
error[E0282]: type annotations needed
  --> $DIR/issue-42234-unknown-receiver-type.rs:12:10
   |
12 |         .sum::<_>() //~ ERROR type annotations needed
   |          ^^^ cannot infer type of the type parameter `S` declared on the method `sum`
   |
help: consider specifying the generic argument
   |
23 -         .sum::<_>() //~ ERROR type annotations needed
23 +         .sum::<GENERIC_ARG>() //~ ERROR type annotations needed
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0282]: type annotations needed
   ╭▸ $DIR/issue-42234-unknown-receiver-type.rs:12:10
   │
12 │         .sum::<_>() //~ ERROR type annotations needed
   │          ━━━ cannot infer type of the type parameter `S` declared on the method `sum`
   ╰╴
help: consider specifying the generic argument
   ╭╴
23 -         .sum::<_>() //~ ERROR type annotations needed
23 +         .sum::<GENERIC_ARG>() //~ ERROR type annotations needed
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn suggestion_same_as_source() {
    let source = r#"// When the type of a method call's receiver is unknown, the span should point
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

    let input = &[
        Level::ERROR
            .primary_title("type annotations needed")
            .id("E0282")
            .element(
                Snippet::source(source)
                    .path("$DIR/issue-42234-unknown-receiver-type.rs")
                    .annotation(AnnotationKind::Primary.span(449..452).label(
                        "cannot infer type of the type parameter `S` declared on the method `sum`",
                    )),
            ),
        Level::HELP
            .secondary_title("consider specifying the generic argument")
            .element(
                Snippet::source(source)
                    .path("$DIR/issue-42234-unknown-receiver-type.rs")
                    .line_start(12)
                    .fold(true)
                    .patch(Patch::new(452..457, "::<_>")),
            ),
    ];
    let expected_ascii = str![[r#"
error[E0282]: type annotations needed
  --> $DIR/issue-42234-unknown-receiver-type.rs:12:10
   |
12 |         .sum::<_>() //~ ERROR type annotations needed
   |          ^^^ cannot infer type of the type parameter `S` declared on the method `sum`
   |
help: consider specifying the generic argument
   |
23 |         .sum::<_>() //~ ERROR type annotations needed
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0282]: type annotations needed
   ╭▸ $DIR/issue-42234-unknown-receiver-type.rs:12:10
   │
12 │         .sum::<_>() //~ ERROR type annotations needed
   │          ━━━ cannot infer type of the type parameter `S` declared on the method `sum`
   ╰╴
help: consider specifying the generic argument
   ╭╴
23 │         .sum::<_>() //~ ERROR type annotations needed
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn keep_lines1() {
    let source = r#"
cargo
fuzzy
pizza
jumps
crazy
quack
zappy
"#;

    let input = &[Level::ERROR
        .primary_title("the size for values of type `T` cannot be known at compilation time")
        .id("E0277")
        .element(
            Snippet::source(source)
                .line_start(11)
                .annotation(AnnotationKind::Primary.span(1..6))
                .annotation(AnnotationKind::Visible.span(37..41)),
        )];
    let expected_ascii = str![[r#"
error[E0277]: the size for values of type `T` cannot be known at compilation time
   |
12 | cargo
   | ^^^^^
...
18 | zappy
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0277]: the size for values of type `T` cannot be known at compilation time
   ╭▸ 
12 │ cargo
   │ ━━━━━
   ┆
18 │ zappy
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn keep_lines2() {
    let source = r#"
cargo
fuzzy
pizza
jumps
crazy
quack
zappy
"#;

    let input = &[Level::ERROR
        .primary_title("the size for values of type `T` cannot be known at compilation time")
        .id("E0277")
        .element(
            Snippet::source(source)
                .line_start(11)
                .annotation(AnnotationKind::Primary.span(1..6))
                .annotation(AnnotationKind::Visible.span(16..18)),
        )];
    let expected_ascii = str![[r#"
error[E0277]: the size for values of type `T` cannot be known at compilation time
   |
12 | cargo
   | ^^^^^
13 | fuzzy
14 | pizza
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0277]: the size for values of type `T` cannot be known at compilation time
   ╭▸ 
12 │ cargo
   │ ━━━━━
13 │ fuzzy
14 │ pizza
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn message_before_primary_snippet() {
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

    let input = &[Level::ERROR
        .primary_title("no field `field` on type `Thing`")
        .id("E0609")
        .element(Level::NOTE.message("a `Title` then a `Message`!?!?"))
        .element(
            Snippet::source(source)
                .path("$DIR/too-many-field-suggestions.rs")
                .annotation(
                    AnnotationKind::Primary
                        .span(270..275)
                        .label("unknown field"),
                ),
        )];

    let expected_ascii = str![[r#"
error[E0609]: no field `field` on type `Thing`
   |
   = note: a `Title` then a `Message`!?!?
  --> $DIR/too-many-field-suggestions.rs:26:7
   |
26 |     t.field;
   |       ^^^^^ unknown field
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0609]: no field `field` on type `Thing`
   │
   ├ note: a `Title` then a `Message`!?!?
   ├▸ $DIR/too-many-field-suggestions.rs:26:7
   │
26 │     t.field;
   ╰╴      ━━━━━ unknown field
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn multiple_line_num_widths() {
    let source = r#"
                cargo-features = ["path-bases"]

                [package]
                name = "foo"
                version = "0.5.0"
                authors = ["wycats@example.com"]

                [dependencies]
                bar = { base = '^^not-valid^^', path = 'bar' }
            "#;

    let title = "invalid character `^` in path base name: `^^not-valid^^`, the first character must be a Unicode XID start character (most letters or `_`)";

    let input = &[Level::ERROR.primary_title(title).element(
        Snippet::source(source)
            .path("Cargo.toml")
            .annotation(AnnotationKind::Primary.span(243..282))
            .annotation(AnnotationKind::Visible.span(206..219)),
    )];

    let expected_ascii = str![[r#"
error: invalid character `^` in path base name: `^^not-valid^^`, the first character must be a Unicode XID start character (most letters or `_`)
  --> Cargo.toml:10:24
   |
 9 |                 [dependencies]
10 |                 bar = { base = '^^not-valid^^', path = 'bar' }
   |                        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: invalid character `^` in path base name: `^^not-valid^^`, the first character must be a Unicode XID start character (most letters or `_`)
   ╭▸ Cargo.toml:10:24
   │
 9 │                 [dependencies]
10 │                 bar = { base = '^^not-valid^^', path = 'bar' }
   ╰╴                       ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn tab() {
    let source = "
 t
\tt
";

    let title = "showing how tabs are rendered";

    let input = &[Level::ERROR.primary_title(title).element(
        Snippet::source(source)
            .path("tabbed.txt")
            .annotation(AnnotationKind::Primary.span(2..3))
            .annotation(AnnotationKind::Context.span(5..6)),
    )];

    let expected_ascii = str![[r#"
error: showing how tabs are rendered
 --> tabbed.txt:2:2
  |
2 |  t
  |  ^
3 |     t
  |     -
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: showing how tabs are rendered
  ╭▸ tabbed.txt:2:2
  │
2 │  t
  │  ━
3 │     t
  ╰╴    ─
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn duplicate_annotations() {
    let source = r#"foobar

            foobar 🚀
"#;
    let report = &[Level::WARNING.primary_title("whatever").element(
        Snippet::source(source)
            .path("whatever")
            .annotation(AnnotationKind::Primary.span(0..source.len()).label("blah"))
            .annotation(AnnotationKind::Primary.span(0..source.len()).label("blah")),
    )];

    let expected_ascii = str![[r#"
warning: whatever
 --> whatever:1:1
  |
1 | / foobar
2 | |
3 | |             foobar 🚀
  | |                      ^
  | |                      |
  | |______________________blah
  |                        blah
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(report), expected_ascii);

    let expected_unicode = str![[r#"
warning: whatever
  ╭▸ whatever:1:1
  │
1 │ ┏ foobar
2 │ ┃
3 │ ┃             foobar 🚀
  │ ┃                      ╿
  │ ┃                      │
  │ ┗━━━━━━━━━━━━━━━━━━━━━━blah
  ╰╴                       blah
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(report), expected_unicode);
}

#[test]
fn alignment() {
    let source = "SELECT bar";

    let title = "ensure single line at line 0 rendered correctly with group line lined up";

    let input = &[Level::ERROR.primary_title(title).element(
        Snippet::source(source)
            .path("Cargo.toml")
            .line_start(0)
            .annotation(
                AnnotationKind::Primary
                    .span(7..10)
                    .label("unexpected token"),
            )
            .annotation(
                AnnotationKind::Visible
                    .span(0..10)
                    .label("while parsing statement"),
            ),
    )];

    let expected_ascii = str![[r#"
error: ensure single line at line 0 rendered correctly with group line lined up
 --> Cargo.toml:0:8
  |
0 | SELECT bar
  |        ^^^ unexpected token
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: ensure single line at line 0 rendered correctly with group line lined up
  ╭▸ Cargo.toml:0:8
  │
0 │ SELECT bar
  ╰╴       ━━━ unexpected token
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn trimmed_multiline_suggestion() {
    let source = r#"fn function_with_lots_of_arguments(a: i32, b: char, c: i32, d: i32, e: i32, f: i32) {}

fn main() {
    let variable_name = 42;
    function_with_lots_of_arguments(
        variable_name,
        variable_name,
        variable_name,
        variable_name,
        variable_name,
    );
    //~^^^^^^^ ERROR this function takes 6 arguments but 5 arguments were supplied [E0061]
}
"#;
    let path = "$DIR/trimmed_multiline_suggestion.rs";

    let input = &[
        Group::with_title(
            Level::ERROR
                .primary_title("this function takes 6 arguments but 5 arguments were supplied")
                .id("E0061"),
        )
        .element(
            Snippet::source(source)
                .path(path)
                .annotation(
                    AnnotationKind::Context
                        .span(196..209)
                        .label("argument #2 of type `char` is missing"),
                )
                .annotation(AnnotationKind::Primary.span(132..163)),
        ),
        Group::with_title(Level::NOTE.secondary_title("function defined here")).element(
            Snippet::source(source)
                .path(path)
                .annotation(AnnotationKind::Context.span(43..50).label(""))
                .annotation(AnnotationKind::Primary.span(3..34)),
        ),
        Group::with_title(Level::HELP.secondary_title("provide the argument")).element(
            Snippet::source(source).path(path).patch(Patch::new(
                163..285,
                "(
        variable_name,
        /* char */,
        variable_name,
        variable_name,
        variable_name,
        variable_name,
    )",
            )),
        ),
    ];

    let expected_ascii = str![[r#"
error[E0061]: this function takes 6 arguments but 5 arguments were supplied
 --> $DIR/trimmed_multiline_suggestion.rs:5:5
  |
5 |     function_with_lots_of_arguments(
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
6 |         variable_name,
7 |         variable_name,
  |         ------------- argument #2 of type `char` is missing
  |
note: function defined here
 --> $DIR/trimmed_multiline_suggestion.rs:1:4
  |
1 | fn function_with_lots_of_arguments(a: i32, b: char, c: i32, d: i32, e: i32, f: i32) {}
  |    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^         -------
help: provide the argument
  |
5 |     function_with_lots_of_arguments(
6 |         variable_name,
7 ~         /* char */,
8 ~         variable_name,
  |
"#]];
    let renderer_ascii = Renderer::plain();
    assert_data_eq!(renderer_ascii.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0061]: this function takes 6 arguments but 5 arguments were supplied
  ╭▸ $DIR/trimmed_multiline_suggestion.rs:5:5
  │
5 │     function_with_lots_of_arguments(
  │     ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
6 │         variable_name,
7 │         variable_name,
  │         ───────────── argument #2 of type `char` is missing
  ╰╴
note: function defined here
  ╭▸ $DIR/trimmed_multiline_suggestion.rs:1:4
  │
1 │ fn function_with_lots_of_arguments(a: i32, b: char, c: i32, d: i32, e: i32, f: i32) {}
  ╰╴   ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━         ───────
help: provide the argument
  ╭╴
5 │     function_with_lots_of_arguments(
6 │         variable_name,
7 ±         /* char */,
8 ±         variable_name,
  ╰╴
"#]];
    let renderer_unicode = renderer_ascii.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer_unicode.render(input), expected_unicode);
}

#[test]
fn trimmed_multiline_suggestion_elided_lines() {
    let source_0 = r#"    nums.iter().for_each(|x| {
        if *x > 0 {
            println!("Positive number");
        } else {
            println!("Negative number");
        }
    })
"#;
    let source_1 = r#"#![deny(clippy::semicolon_if_nothing_returned)]
"#;

    let input = &[
        Group::with_title(Level::ERROR.primary_title(
            "consider adding a `;` to the last statement for consistent formatting",
        ))
        .element(
            Snippet::source(source_0)
                .path("tests/ui/semicolon_if_nothing_returned_testing.rs")
                .line_start(4)
                .annotation(AnnotationKind::Primary.span(4..166)),
        ),
        Group::with_title(Level::NOTE.secondary_title("the lint level is defined here")).element(
            Snippet::source(source_1)
                .path("tests/ui/semicolon_if_nothing_returned_testing.rs")
                .line_start(2)
                .annotation(AnnotationKind::Primary.span(8..45)),
        ),
        Group::with_title(Level::HELP.secondary_title("add a `;` here")).element(
            Snippet::source(source_0)
                .path("tests/ui/semicolon_if_nothing_returned_testing.rs")
                .line_start(4)
                .fold(true)
                .patch(Patch::new(
                    4..166,
                    r#"nums.iter().for_each(|x| {
        if *x > 0 {
            println!("Positive number");
        } else {
            println!("Negative number");
        }
    });"#,
                )),
        ),
    ];

    let expected_ascii = str![[r#"
error: consider adding a `;` to the last statement for consistent formatting
  --> tests/ui/semicolon_if_nothing_returned_testing.rs:4:5
   |
 4 | /     nums.iter().for_each(|x| {
 5 | |         if *x > 0 {
 6 | |             println!("Positive number");
 7 | |         } else {
...  |
10 | |     })
   | |______^
   |
note: the lint level is defined here
  --> tests/ui/semicolon_if_nothing_returned_testing.rs:2:9
   |
 2 | #![deny(clippy::semicolon_if_nothing_returned)]
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: add a `;` here
   |
 4 |     nums.iter().for_each(|x| {
...
 9 |         }
10 ~     });
   |
"#]];
    let renderer_ascii = Renderer::plain();
    assert_data_eq!(renderer_ascii.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: consider adding a `;` to the last statement for consistent formatting
   ╭▸ tests/ui/semicolon_if_nothing_returned_testing.rs:4:5
   │
 4 │ ┏     nums.iter().for_each(|x| {
 5 │ ┃         if *x > 0 {
 6 │ ┃             println!("Positive number");
 7 │ ┃         } else {
   ┆ ┇
10 │ ┃     })
   │ ┗━━━━━━┛
   ╰╴
note: the lint level is defined here
   ╭▸ tests/ui/semicolon_if_nothing_returned_testing.rs:2:9
   │
 2 │ #![deny(clippy::semicolon_if_nothing_returned)]
   ╰╴        ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
help: add a `;` here
   ╭╴
 4 │     nums.iter().for_each(|x| {
 …
 9 │         }
10 ±     });
   ╰╴
"#]];
    let renderer_unicode = renderer_ascii.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer_unicode.render(input), expected_unicode);
}

#[test]
fn suggestion_no_fold() {
    let source = r#"fn main() {
    let variable_name = 42;
    function_with_lots_of_arguments(
        variable_name,
        variable_name,
        variable_name,
        variable_name,
    );
}"#;
    let path = "$DIR/trimmed_multiline_suggestion.rs";

    let input = &[
        Group::with_title(
            Level::ERROR
                .primary_title("this function takes 6 arguments but 5 arguments were supplied")
                .id("E0061"),
        )
        .element(
            Snippet::source(source)
                .path(path)
                .annotation(
                    AnnotationKind::Context
                        .span(108..121)
                        .label("argument #2 of type `char` is missing"),
                )
                .annotation(AnnotationKind::Primary.span(44..75)),
        ),
        Group::with_title(Level::HELP.secondary_title("provide the argument")).element(
            Snippet::source(source)
                .path(path)
                .fold(false)
                .patch(Patch::new(
                    75..174,
                    "(
        variable_name,
        /* char */,
        variable_name,
        variable_name,
        variable_name,
    )",
                )),
        ),
    ];

    let expected_ascii = str![[r#"
error[E0061]: this function takes 6 arguments but 5 arguments were supplied
  --> $DIR/trimmed_multiline_suggestion.rs:3:5
   |
 3 |     function_with_lots_of_arguments(
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
 4 |         variable_name,
 5 |         variable_name,
   |         ------------- argument #2 of type `char` is missing
   |
help: provide the argument
   |
 1 | fn main() {
 2 |     let variable_name = 42;
 3 |     function_with_lots_of_arguments(
 4 |         variable_name,
 5 ~         /* char */,
 6 ~         variable_name,
 7 |         variable_name,
 8 |         variable_name,
 9 |     );
10 | }
   |
"#]];
    let renderer_ascii = Renderer::plain();
    assert_data_eq!(renderer_ascii.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0061]: this function takes 6 arguments but 5 arguments were supplied
   ╭▸ $DIR/trimmed_multiline_suggestion.rs:3:5
   │
 3 │     function_with_lots_of_arguments(
   │     ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 4 │         variable_name,
 5 │         variable_name,
   │         ───────────── argument #2 of type `char` is missing
   ╰╴
help: provide the argument
   ╭╴
 1 │ fn main() {
 2 │     let variable_name = 42;
 3 │     function_with_lots_of_arguments(
 4 │         variable_name,
 5 ±         /* char */,
 6 ±         variable_name,
 7 │         variable_name,
 8 │         variable_name,
 9 │     );
10 │ }
   ╰╴
"#]];
    let renderer_unicode = renderer_ascii.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer_unicode.render(input), expected_unicode);
}

#[test]
fn suggestion_no_fold_replacement_ends_with_newline() {
    let source = r#"
use st::cell::Cell;

mod bar {
    pub fn bar() { bar::baz(); }

    fn baz() {}
}

use bas::bar;

struct Foo {
    bar: st::cell::Cell<bool>
}

fn main() {}"#;

    let input = &[
        Level::ERROR
            .primary_title("failed to resolve: use of undeclared crate or module `st`")
            .id("E0433")
            .element(
                Snippet::source(source).line_start(1).annotation(
                    AnnotationKind::Primary
                        .span(122..124)
                        .label("use of undeclared crate or module `st`"),
                ),
            ),
        Level::HELP
            .secondary_title("consider importing this module")
            .element(
                Snippet::source(source)
                    .fold(false)
                    .patch(Patch::new(1..1, "use std::cell;\n")),
            ),
    ];
    let expected_ascii = str![[r#"
error[E0433]: failed to resolve: use of undeclared crate or module `st`
   |
13 |     bar: st::cell::Cell<bool>
   |          ^^ use of undeclared crate or module `st`
   |
help: consider importing this module
   |
 1 |
 2 + use std::cell;
 3 | use st::cell::Cell;
 4 |
 5 | mod bar {
 6 |     pub fn bar() { bar::baz(); }
 7 |
 8 |     fn baz() {}
 9 | }
10 |
11 | use bas::bar;
12 |
13 | struct Foo {
14 |     bar: st::cell::Cell<bool>
15 | }
16 |
17 | fn main() {}
   |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0433]: failed to resolve: use of undeclared crate or module `st`
   ╭▸ 
13 │     bar: st::cell::Cell<bool>
   │          ━━ use of undeclared crate or module `st`
   ╰╴
help: consider importing this module
   ╭╴
 1 │
 2 + use std::cell;
 3 │ use st::cell::Cell;
 4 │
 5 │ mod bar {
 6 │     pub fn bar() { bar::baz(); }
 7 │
 8 │     fn baz() {}
 9 │ }
10 │
11 │ use bas::bar;
12 │
13 │ struct Foo {
14 │     bar: st::cell::Cell<bool>
15 │ }
16 │
17 │ fn main() {}
   ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn original_matches_replacement_suffix() {
    let source = r#"use sync;"#;
    let input = &[
        Group::with_level(Level::ERROR).element(
            Snippet::source(source).path("/tmp/test.rs").annotation(
                AnnotationKind::Primary
                    .span(4..8)
                    .label("no `sync` in the root"),
            ),
        ),
        Level::HELP
            .secondary_title("consider importing this module instead")
            .element(
                Snippet::source(source)
                    .path("/tmp/test.rs")
                    .patch(Patch::new(4..8, "std::sync")),
            ),
    ];

    let expected_ascii = str![[r#"
 --> /tmp/test.rs:1:5
  |
1 | use sync;
  |     ^^^^ no `sync` in the root
  |
help: consider importing this module instead
  |
1 | use std::sync;
  |     +++++
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
  ╭▸ /tmp/test.rs:1:5
  │
1 │ use sync;
  │     ━━━━ no `sync` in the root
  ╰╴
help: consider importing this module instead
  ╭╴
1 │ use std::sync;
  ╰╴    +++++
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn dont_panic_narrow_term_width_short_span() {
    let source = "pub fn f() { let mut foo_bar = 0; }";
    let input = &[Level::WARNING
        .primary_title("variable does not need to be mutable")
        .element(
            Snippet::source(source).path("ice.rs").annotation(
                AnnotationKind::Primary
                    .span(17..28)
                    .label("help: remove this `mut`"),
            ),
        )];
    let expected_ascii = str![[r#"
warning: variable does not need to be mutable
 --> ice.rs:1:18
  |
1 | ...et mut foo_bar = ...
  |       ^^^^^^^^^^^ help: remove this `mut`
"#]];
    let renderer = Renderer::plain().term_width(8);
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
warning: variable does not need to be mutable
  ╭▸ ice.rs:1:18
  │
1 │ … let mut foo_bar = 0;…
  ╰╴      ━━━━━━━━━━━ help: remove this `mut`
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn suggestion_different_path_than_primary() {
    let source = r#"    foo.bar();"#;
    let secondary_source = r#"fn bar(&self) {"#;
    let input = &[
        Level::ERROR
            .primary_title("method `five_years` is private")
            .id("E0624")
            .element(
                Snippet::source(source)
                    .path("lib.rs")
                    .annotation(AnnotationKind::Primary.span(8..13).label("private method")),
            )
            .element(
                Snippet::source(secondary_source)
                    .path("other.rs")
                    .annotation(
                        AnnotationKind::Context
                            .span(3..13)
                            .label("private method defined here"),
                    ),
            ),
        Level::HELP
            .secondary_title("consider making `bar` public")
            .element(
                Snippet::source(secondary_source)
                    .path("other.rs")
                    .patch(Patch::new(0..0, "pub ")),
            ),
    ];

    let expected_ascii = str![[r#"
error[E0624]: method `five_years` is private
 --> lib.rs:1:9
  |
1 |     foo.bar();
  |         ^^^^^ private method
  |
 ::: other.rs:1:4
  |
1 | fn bar(&self) {
  |    ---------- private method defined here
  |
help: consider making `bar` public
 --> other.rs:LL:1
  |
1 | pub fn bar(&self) {
  | +++
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0624]: method `five_years` is private
  ╭▸ lib.rs:1:9
  │
1 │     foo.bar();
  │         ━━━━━ private method
  │
  ⸬  other.rs:1:4
  │
1 │ fn bar(&self) {
  │    ────────── private method defined here
  ╰╴
help: consider making `bar` public
  ╭▸ other.rs:LL:1
  │
1 │ pub fn bar(&self) {
  ╰╴+++
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn prefer_first_annotation_on_secondary_snippets() {
    let input = &[Level::ERROR
        .primary_title("main diagnostic message")
        .id("test-diagnostics")
        .element(
            Snippet::source("aardvark\nbeetle\ncanary\n")
                .path("animals")
                .fold(false)
                .annotation(AnnotationKind::Primary.span(0..8)),
        )
        .element(
            Snippet::source("inchworm\njackrabbit\nkangaroo\n")
                .path("animals")
                .fold(false)
                .annotation(AnnotationKind::Context.span(20..28)),
        )];

    let expected_ascii = str![[r#"
error[test-diagnostics]: main diagnostic message
 --> animals:1:1
  |
1 | aardvark
  | ^^^^^^^^
2 | beetle
3 | canary
  |
 ::: animals:3:1
  |
1 | inchworm
2 | jackrabbit
3 | kangaroo
  | --------
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[test-diagnostics]: main diagnostic message
  ╭▸ animals:1:1
  │
1 │ aardvark
  │ ━━━━━━━━
2 │ beetle
3 │ canary
  │
  ⸬  animals:3:1
  │
1 │ inchworm
2 │ jackrabbit
3 │ kangaroo
  ╰╴────────
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn consistent_indentation_with_trailing_newline() {
    let input = &[Level::ERROR
        .primary_title("main diagnostic message")
        .id("test-diagnostic")
        .element(
            Snippet::source("dog\nelephant\nfinch\n")
                .path("spacey-animals")
                .fold(false)
                .line_start(7)
                .annotation(AnnotationKind::Primary.span(4..12)),
        )];
    let expected_ascii = str![[r#"
error[test-diagnostic]: main diagnostic message
 --> spacey-animals:8:1
  |
7 | dog
8 | elephant
  | ^^^^^^^^
9 | finch
  |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[test-diagnostic]: main diagnostic message
  ╭▸ spacey-animals:8:1
  │
7 │ dog
8 │ elephant
  │ ━━━━━━━━
9 │ finch
  ╰╴
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn long_line_cut() {
    let source = "abcd abcd abcd abcd abcd abcd abcd";
    let input = Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .line_start(1)
            .annotation(AnnotationKind::Primary.span(0..4)),
    );
    let expected = str![[r#"
error: 
  |
1 | abcd abcd a...
  | ^^^^
"#]];
    let renderer = Renderer::plain().term_width(18);
    assert_data_eq!(renderer.render(&[input]).clone(), expected);
}

#[test]
fn long_line_cut_custom() {
    let source = "abcd abcd abcd abcd abcd abcd abcd";
    let input = Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .line_start(1)
            .annotation(AnnotationKind::Primary.span(0..4)),
    );
    // This trims a little less because `…` is visually smaller than `...`.
    let expected = str![[r#"
error: 
  |
1 | abcd abcd abc…
  | ^^^^
"#]];
    let renderer = Renderer::plain().term_width(18).cut_indicator("…");
    assert_data_eq!(renderer.render(&[input]).clone(), expected);
}

#[test]
fn leading_nbsp_no_overflow() {
    // Regression test: an annotation pointing at leading whitespace caused a
    // subtraction overflow in Margin::compute because `label_right` can be less
    // than `whitespace_left`. See https://github.com/astral-sh/ty/issues/836
    let source = " \u{00A0}                                     'betting_env.datastructure.team_lineup.Tamheet.get_latest': ( 'dataStructure/team_lineup.htm#teamsheet.getlatest',";
    let input = Level::ERROR.primary_title("test").element(
        Snippet::source(source)
            .line_start(1)
            .annotation(AnnotationKind::Primary.span(0..1)),
    );
    // Should not panic.
    let renderer = Renderer::plain();
    let _ = renderer.render(&[input]).clone();
}

#[test]
fn hide_snippet() {
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
        .id("E0038")
        .element(
            Snippet::source(source)
                .line_start(1)
                .path("$DIR/object-fail.rs")
                .annotation(
                    AnnotationKind::Primary
                        .span(107..114)
                        .label("`EqAlias` is not dyn compatible")
                        .hide_snippet(true),
                ),
        )];
    let expected_ascii = str![[r#"
error[E0038]: the trait alias `EqAlias` is not dyn compatible
 --> $DIR/object-fail.rs:7:17
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0038]: the trait alias `EqAlias` is not dyn compatible
  ─▸ $DIR/object-fail.rs:7:17
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn only_origin() {
    let input = &[Level::ERROR
        .primary_title("the trait alias `EqAlias` is not dyn compatible")
        .id("E0038")
        .element(
            Origin::path("$SRC_DIR/core/src/cmp.rs")
                .line(334)
                .char_column(14),
        )];
    let expected_ascii = str![[r#"
error[E0038]: the trait alias `EqAlias` is not dyn compatible
 --> $SRC_DIR/core/src/cmp.rs:334:14
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error[E0038]: the trait alias `EqAlias` is not dyn compatible
  ─▸ $SRC_DIR/core/src/cmp.rs:334:14
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}

#[test]
fn lineno_offset() {
    let input = &[
        Level::ERROR.primary_title("oops").element(
            Snippet::source("First line\r\nSecond oops line")
                .path("<current file>")
                .annotation(AnnotationKind::Primary.span(19..23).label("oops")),
        ),
        Level::NOTE.secondary_title("details").element(
            Snippet::source("First line\r\nSecond oops line")
                .path("<other file>")
                .annotation(AnnotationKind::Primary.span(19..23).label("oops")),
        ),
        Level::HELP
            .secondary_title("suggestion")
            .element(Origin::path("<current file>"))
            .lineno_offset(10),
    ];
    let expected_ascii = str![[r#"
error: oops
           --> <current file>:2:8
            |
          2 | Second oops line
            |        ^^^^ oops
            |
note: details
           --> <other file>:2:8
            |
          2 | Second oops line
            |        ^^^^ oops
help: suggestion
           --> <current file>
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = str![[r#"
error: oops
            ╭▸ <current file>:2:8
            │
          2 │ Second oops line
            │        ━━━━ oops
            ╰╴
note: details
            ╭▸ <other file>:2:8
            │
          2 │ Second oops line
            ╰╴       ━━━━ oops
help: suggestion
            ─▸ <current file>
"#]];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
