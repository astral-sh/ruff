// Since this is a vendored copy of `annotate-snippets`, we squash Clippy
// warnings from upstream in order to the reduce the diff. If our copy drifts
// far from upstream such that patches become impractical to apply in both
// places, then we can get rid of these suppressions and fix the lints.
#![allow(clippy::redundant_clone, clippy::should_panic_without_expect)]

use ruff_annotate_snippets::{Level, Renderer, Snippet};

use snapbox::{assert_data_eq, str};

#[test]
fn test_i_29() {
    let snippets = Level::Error.title("oops").snippet(
        Snippet::source("First line\r\nSecond oops line")
            .origin("<current file>")
            .annotation(Level::Error.span(19..23).label("oops"))
            .fold(true),
    );
    let expected = str![[r#"
error: oops
 --> <current file>:2:8
  |
2 | Second oops line
  |        ^^^^ oops
  |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(snippets).to_string(), expected);
}

#[test]
fn test_point_to_double_width_characters() {
    let snippets = Level::Error.title("").snippet(
        Snippet::source("こんにちは、世界")
            .origin("<current file>")
            .annotation(Level::Error.span(18..24).label("world")),
    );

    let expected = str![[r#"
error
 --> <current file>:1:7
  |
1 | こんにちは、世界
  |             ^^^^ world
  |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(snippets).to_string(), expected);
}

#[test]
fn test_point_to_double_width_characters_across_lines() {
    let snippets = Level::Error.title("").snippet(
        Snippet::source("おはよう\nございます")
            .origin("<current file>")
            .annotation(Level::Error.span(6..22).label("Good morning")),
    );

    let expected = str![[r#"
error
 --> <current file>:1:3
  |
1 |   おはよう
  |  _____^
2 | | ございます
  | |______^ Good morning
  |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(snippets).to_string(), expected);
}

#[test]
fn test_point_to_double_width_characters_multiple() {
    let snippets = Level::Error.title("").snippet(
        Snippet::source("お寿司\n食べたい🍣")
            .origin("<current file>")
            .annotation(Level::Error.span(0..9).label("Sushi1"))
            .annotation(Level::Note.span(16..22).label("Sushi2")),
    );

    let expected = str![[r#"
error
 --> <current file>:1:1
  |
1 | お寿司
  | ^^^^^^ Sushi1
2 | 食べたい🍣
  |     ---- note: Sushi2
  |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(snippets).to_string(), expected);
}

#[test]
fn test_point_to_double_width_characters_mixed() {
    let snippets = Level::Error.title("").snippet(
        Snippet::source("こんにちは、新しいWorld！")
            .origin("<current file>")
            .annotation(Level::Error.span(18..32).label("New world")),
    );

    let expected = str![[r#"
error
 --> <current file>:1:7
  |
1 | こんにちは、新しいWorld！
  |             ^^^^^^^^^^^ New world
  |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(snippets).to_string(), expected);
}

#[test]
fn test_format_title() {
    let input = Level::Error.title("This is a title").id("E0001");

    let expected = str![r#"error[E0001]: This is a title"#];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
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
    let src_annotation = Level::Error.span(56..58).label("B006");
    let snippet = Snippet::source(source)
        .line_start(1)
        .annotation(src_annotation)
        .fold(false);
    let message = Level::None.title("").snippet(snippet);

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
    assert_data_eq!(Renderer::plain().render(message).to_string(), expected);
}

#[test]
fn test_format_snippet_only() {
    let source = "This is line 1\nThis is line 2";
    let input = Level::Error
        .title("")
        .snippet(Snippet::source(source).line_start(5402));

    let expected = str![[r#"
error
     |
5402 | This is line 1
5403 | This is line 2
     |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn test_format_snippets_continuation() {
    let src_0 = "This is slice 1";
    let src_1 = "This is slice 2";
    let input = Level::Error
        .title("")
        .snippet(Snippet::source(src_0).line_start(5402).origin("file1.rs"))
        .snippet(Snippet::source(src_1).line_start(2).origin("file2.rs"));
    let expected = str![[r#"
error
    --> file1.rs
     |
5402 | This is slice 1
     |
    ::: file2.rs
     |
   2 | This is slice 2
     |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn test_format_snippet_annotation_standalone() {
    let line_1 = "This is line 1";
    let line_2 = "This is line 2";
    let source = [line_1, line_2].join("\n");
    // In line 2
    let range = 22..24;
    let input = Level::Error.title("").snippet(
        Snippet::source(&source)
            .line_start(5402)
            .annotation(Level::Info.span(range.clone()).label("Test annotation")),
    );
    let expected = str![[r#"
error
     |
5402 | This is line 1
5403 | This is line 2
     |        -- info: Test annotation
     |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn test_format_footer_title() {
    let input = Level::Error
        .title("")
        .footer(Level::Error.title("This __is__ a title"));
    let expected = str![[r#"
error
 = error: This __is__ a title
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
#[should_panic]
fn test_i26() {
    let source = "short";
    let label = "label";
    let input = Level::Error.title("").snippet(
        Snippet::source(source)
            .line_start(0)
            .annotation(Level::Error.span(0..source.len() + 2).label(label)),
    );
    let renderer = Renderer::plain();
    let _ = renderer.render(input).to_string();
}

#[test]
fn test_source_content() {
    let source = "This is an example\nof content lines";
    let input = Level::Error
        .title("")
        .snippet(Snippet::source(source).line_start(56));
    let expected = str![[r#"
error
   |
56 | This is an example
57 | of content lines
   |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn test_source_annotation_standalone_singleline() {
    let source = "tests";
    let input = Level::Error.title("").snippet(
        Snippet::source(source)
            .line_start(1)
            .annotation(Level::Help.span(0..5).label("Example string")),
    );
    let expected = str![[r#"
error
  |
1 | tests
  | ----- help: Example string
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn test_source_annotation_standalone_multiline() {
    let source = "tests";
    let input = Level::Error.title("").snippet(
        Snippet::source(source)
            .line_start(1)
            .annotation(Level::Help.span(0..5).label("Example string"))
            .annotation(Level::Help.span(0..5).label("Second line")),
    );
    let expected = str![[r#"
error
  |
1 | tests
  | -----
  | |
  | help: Example string
  | help: Second line
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn test_only_source() {
    let input = Level::Error
        .title("")
        .snippet(Snippet::source("").origin("file.rs"));
    let expected = str![[r#"
error
--> file.rs
 |
 |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn test_anon_lines() {
    let source = "This is an example\nof content lines\n\nabc";
    let input = Level::Error
        .title("")
        .snippet(Snippet::source(source).line_start(56));
    let expected = str![[r#"
error
   |
LL | This is an example
LL | of content lines
LL |
LL | abc
   |
"#]];
    let renderer = Renderer::plain().anonymized_line_numbers(true);
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn issue_130() {
    let input = Level::Error.title("dummy").snippet(
        Snippet::source("foo\nbar\nbaz")
            .origin("file/path")
            .line_start(3)
            .fold(true)
            .annotation(Level::Error.span(4..11)), // bar\nbaz
    );

    let expected = str![[r#"
error: dummy
 --> file/path:4:1
  |
4 | / bar
5 | | baz
  | |___^
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn unterminated_string_multiline() {
    let source = "\
a\"
// ...
";
    let input = Level::Error.title("").snippet(
        Snippet::source(source)
            .origin("file/path")
            .line_start(3)
            .fold(true)
            .annotation(Level::Error.span(0..10)), // 1..10 works
    );
    let expected = str![[r#"
error
 --> file/path:3:1
  |
3 | / a"
4 | | // ...
  | |_______^
  |
"#]];
    let renderer = Renderer::plain().anonymized_line_numbers(false);
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn char_and_nl_annotate_char() {
    let source = "a\r\nb";
    let input = Level::Error.title("").snippet(
        Snippet::source(source)
            .origin("file/path")
            .line_start(3)
            .annotation(Level::Error.span(0..2)), // a\r
    );
    let expected = str![[r#"
error
 --> file/path:3:1
  |
3 | a
  | ^
4 | b
  |"#]];
    let renderer = Renderer::plain().anonymized_line_numbers(false);
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn char_eol_annotate_char() {
    let source = "a\r\nb";
    let input = Level::Error.title("").snippet(
        Snippet::source(source)
            .origin("file/path")
            .line_start(3)
            .annotation(Level::Error.span(0..3)), // a\r\n
    );
    let expected = str![[r#"
error
 --> file/path:3:1
  |
3 | a
  | ^
4 | b
  |"#]];
    let renderer = Renderer::plain().anonymized_line_numbers(false);
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn char_eol_annotate_char_double_width() {
    let snippets = Level::Error.title("").snippet(
        Snippet::source("こん\r\nにちは\r\n世界")
            .origin("<current file>")
            .annotation(Level::Error.span(3..8)), // ん\r\n
    );

    let expected = str![[r#"
error
 --> <current file>:1:2
  |
1 | こん
  |   ^^
2 | にちは
3 | 世界
  |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(snippets).to_string(), expected);
}

#[test]
fn annotate_eol() {
    let source = "a\r\nb";
    let input = Level::Error.title("").snippet(
        Snippet::source(source)
            .origin("file/path")
            .line_start(3)
            .annotation(Level::Error.span(1..2)), // \r
    );
    let expected = str![[r#"
error
 --> file/path:3:2
  |
3 | a
  |  ^
4 | b
  |"#]];
    let renderer = Renderer::plain().anonymized_line_numbers(false);
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn annotate_eol2() {
    let source = "a\r\nb";
    let input = Level::Error.title("").snippet(
        Snippet::source(source)
            .origin("file/path")
            .line_start(3)
            .annotation(Level::Error.span(1..3)), // \r\n
    );
    let expected = str![[r#"
error
 --> file/path:3:2
  |
3 | a
  |  ^
4 | b
  |"#]];
    let renderer = Renderer::plain().anonymized_line_numbers(false);
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn annotate_eol3() {
    let source = "a\r\nb";
    let input = Level::Error.title("").snippet(
        Snippet::source(source)
            .origin("file/path")
            .line_start(3)
            .annotation(Level::Error.span(2..3)), // \n
    );
    let expected = str![[r#"
error
 --> file/path:3:2
  |
3 | a
  |  ^
4 | b
  |"#]];
    let renderer = Renderer::plain().anonymized_line_numbers(false);
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn annotate_eol4() {
    let source = "a\r\nb";
    let input = Level::Error.title("").snippet(
        Snippet::source(source)
            .origin("file/path")
            .line_start(3)
            .annotation(Level::Error.span(2..2)), // \n
    );
    let expected = str![[r#"
error
 --> file/path:3:2
  |
3 | a
  |  ^
4 | b
  |"#]];
    let renderer = Renderer::plain().anonymized_line_numbers(false);
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn annotate_eol_double_width() {
    let snippets = Level::Error.title("").snippet(
        Snippet::source("こん\r\nにちは\r\n世界")
            .origin("<current file>")
            .annotation(Level::Error.span(7..8)), // \n
    );

    let expected = str![[r#"
error
 --> <current file>:1:3
  |
1 | こん
  |     ^
2 | にちは
3 | 世界
  |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(snippets).to_string(), expected);
}

#[test]
fn multiline_eol_start() {
    let source = "a\r\nb";
    let input = Level::Error.title("").snippet(
        Snippet::source(source)
            .origin("file/path")
            .line_start(3)
            .annotation(Level::Error.span(1..4)), // \r\nb
    );
    let expected = str![[r#"
error
 --> file/path:3:2
  |
3 |   a
  |  __^
4 | | b
  | |_^
  |"#]];
    let renderer = Renderer::plain().anonymized_line_numbers(false);
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn multiline_eol_start2() {
    let source = "a\r\nb";
    let input = Level::Error.title("").snippet(
        Snippet::source(source)
            .origin("file/path")
            .line_start(3)
            .annotation(Level::Error.span(2..4)), // \nb
    );
    let expected = str![[r#"
error
 --> file/path:3:2
  |
3 |   a
  |  __^
4 | | b
  | |_^
  |"#]];
    let renderer = Renderer::plain().anonymized_line_numbers(false);
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn multiline_eol_start3() {
    let source = "a\nb";
    let input = Level::Error.title("").snippet(
        Snippet::source(source)
            .origin("file/path")
            .line_start(3)
            .annotation(Level::Error.span(1..3)), // \nb
    );
    let expected = str![[r#"
error
 --> file/path:3:2
  |
3 |   a
  |  __^
4 | | b
  | |_^
  |"#]];
    let renderer = Renderer::plain().anonymized_line_numbers(false);
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn multiline_eol_start_double_width() {
    let snippets = Level::Error.title("").snippet(
        Snippet::source("こん\r\nにちは\r\n世界")
            .origin("<current file>")
            .annotation(Level::Error.span(7..11)), // \r\nに
    );

    let expected = str![[r#"
error
 --> <current file>:1:3
  |
1 |   こん
  |  _____^
2 | | にちは
  | |__^
3 |   世界
  |
"#]];

    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(snippets).to_string(), expected);
}

#[test]
fn multiline_eol_start_eol_end() {
    let source = "a\nb\nc";
    let input = Level::Error.title("").snippet(
        Snippet::source(source)
            .origin("file/path")
            .line_start(3)
            .annotation(Level::Error.span(1..4)), // \nb\n
    );
    let expected = str![[r#"
error
 --> file/path:3:2
  |
3 |   a
  |  __^
4 | | b
  | |__^
5 |   c
  |
"#]];
    let renderer = Renderer::plain().anonymized_line_numbers(false);
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn multiline_eol_start_eol_end2() {
    let source = "a\r\nb\r\nc";
    let input = Level::Error.title("").snippet(
        Snippet::source(source)
            .origin("file/path")
            .line_start(3)
            .annotation(Level::Error.span(2..5)), // \nb\r
    );
    let expected = str![[r#"
error
 --> file/path:3:2
  |
3 |   a
  |  __^
4 | | b
  | |__^
5 |   c
  |
"#]];
    let renderer = Renderer::plain().anonymized_line_numbers(false);
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn multiline_eol_start_eol_end3() {
    let source = "a\r\nb\r\nc";
    let input = Level::Error.title("").snippet(
        Snippet::source(source)
            .origin("file/path")
            .line_start(3)
            .annotation(Level::Error.span(2..6)), // \nb\r\n
    );
    let expected = str![[r#"
error
 --> file/path:3:2
  |
3 |   a
  |  __^
4 | | b
  | |__^
5 |   c
  |
"#]];
    let renderer = Renderer::plain().anonymized_line_numbers(false);
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn multiline_eol_start_eof_end() {
    let source = "a\r\nb";
    let input = Level::Error.title("").snippet(
        Snippet::source(source)
            .origin("file/path")
            .line_start(3)
            .annotation(Level::Error.span(1..5)), // \r\nb(EOF)
    );
    let expected = str![[r#"
error
 --> file/path:3:2
  |
3 |   a
  |  __^
4 | | b
  | |__^
  |
"#]];
    let renderer = Renderer::plain().anonymized_line_numbers(false);
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn multiline_eol_start_eof_end_double_width() {
    let source = "ん\r\nに";
    let input = Level::Error.title("").snippet(
        Snippet::source(source)
            .origin("file/path")
            .line_start(3)
            .annotation(Level::Error.span(3..9)), // \r\nに(EOF)
    );
    let expected = str![[r#"
error
 --> file/path:3:2
  |
3 |   ん
  |  ___^
4 | | に
  | |___^
  |
"#]];
    let renderer = Renderer::plain().anonymized_line_numbers(false);
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn two_single_line_same_line() {
    let source = r#"bar = { version = "0.1.0", optional = true }"#;
    let input = Level::Error.title("unused optional dependency").snippet(
        Snippet::source(source)
            .origin("Cargo.toml")
            .line_start(4)
            .annotation(
                Level::Error
                    .span(0..3)
                    .label("I need this to be really long so I can test overlaps"),
            )
            .annotation(
                Level::Info
                    .span(27..42)
                    .label("This should also be long but not too long"),
            ),
    );
    let expected = str![[r#"
error: unused optional dependency
 --> Cargo.toml:4:1
  |
4 | bar = { version = "0.1.0", optional = true }
  | ^^^                        --------------- info: This should also be long but not too long
  | |
  | I need this to be really long so I can test overlaps
  |
"#]];
    let renderer = Renderer::plain().anonymized_line_numbers(false);
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn multi_and_single() {
    let source = r#"bar = { version = "0.1.0", optional = true }
this is another line
so is this
bar = { version = "0.1.0", optional = true }
"#;
    let input = Level::Error.title("unused optional dependency").snippet(
        Snippet::source(source)
            .line_start(4)
            .annotation(
                Level::Error
                    .span(41..119)
                    .label("I need this to be really long so I can test overlaps"),
            )
            .annotation(
                Level::Info
                    .span(27..42)
                    .label("This should also be long but not too long"),
            ),
    );
    let expected = str![[r#"
error: unused optional dependency
  |
4 |   bar = { version = "0.1.0", optional = true }
  |  ____________________________--------------^
  | |                            |
  | |                            info: This should also be long but not too long
5 | | this is another line
6 | | so is this
7 | | bar = { version = "0.1.0", optional = true }
  | |__________________________________________^ I need this to be really long so I can test overlaps
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn two_multi_and_single() {
    let source = r#"bar = { version = "0.1.0", optional = true }
this is another line
so is this
bar = { version = "0.1.0", optional = true }
"#;
    let input = Level::Error.title("unused optional dependency").snippet(
        Snippet::source(source)
            .line_start(4)
            .annotation(
                Level::Error
                    .span(41..119)
                    .label("I need this to be really long so I can test overlaps"),
            )
            .annotation(
                Level::Error
                    .span(8..102)
                    .label("I need this to be really long so I can test overlaps"),
            )
            .annotation(
                Level::Info
                    .span(27..42)
                    .label("This should also be long but not too long"),
            ),
    );
    let expected = str![[r#"
error: unused optional dependency
  |
4 |    bar = { version = "0.1.0", optional = true }
  |   _________^__________________--------------^
  |  |         |                  |
  |  |_________|                  info: This should also be long but not too long
  | ||
5 | || this is another line
6 | || so is this
7 | || bar = { version = "0.1.0", optional = true }
  | ||_________________________^________________^ I need this to be really long so I can test overlaps
  | |__________________________|
  |                            I need this to be really long so I can test overlaps
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn three_multi_and_single() {
    let source = r#"bar = { version = "0.1.0", optional = true }
this is another line
so is this
bar = { version = "0.1.0", optional = true }
this is another line
"#;
    let input = Level::Error.title("unused optional dependency").snippet(
        Snippet::source(source)
            .line_start(4)
            .annotation(
                Level::Error
                    .span(41..119)
                    .label("I need this to be really long so I can test overlaps"),
            )
            .annotation(
                Level::Error
                    .span(8..102)
                    .label("I need this to be really long so I can test overlaps"),
            )
            .annotation(
                Level::Error
                    .span(48..126)
                    .label("I need this to be really long so I can test overlaps"),
            )
            .annotation(
                Level::Info
                    .span(27..42)
                    .label("This should also be long but not too long"),
            ),
    );
    let expected = str![[r#"
error: unused optional dependency
  |
4 |     bar = { version = "0.1.0", optional = true }
  |   __________^__________________--------------^
  |  |          |                  |
  |  |__________|                  info: This should also be long but not too long
  | ||
5 | ||  this is another line
  | || ____^
6 | ||| so is this
7 | ||| bar = { version = "0.1.0", optional = true }
  | |||_________________________^________________^ I need this to be really long so I can test overlaps
  | |_|_________________________|
  |   |                         I need this to be really long so I can test overlaps
8 |   | this is another line
  |   |____^ I need this to be really long so I can test overlaps
  |
"#]];
    let renderer = Renderer::plain();
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn origin_correct_start_line() {
    let source = "aaa\nbbb\nccc\nddd\n";
    let input = Level::Error.title("title").snippet(
        Snippet::source(source)
            .origin("origin.txt")
            .fold(false)
            .annotation(Level::Error.span(8..8 + 3).label("annotation")),
    );

    let expected = str![[r#"
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
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn origin_correct_mid_line() {
    let source = "aaa\nbbb\nccc\nddd\n";
    let input = Level::Error.title("title").snippet(
        Snippet::source(source)
            .origin("origin.txt")
            .fold(false)
            .annotation(Level::Error.span(8 + 1..8 + 3).label("annotation")),
    );

    let expected = str![[r#"
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
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn long_line_cut() {
    let source = "abcd abcd abcd abcd abcd abcd abcd";
    let input = Level::Error.title("").snippet(
        Snippet::source(source)
            .line_start(1)
            .annotation(Level::Error.span(0..4)),
    );
    let expected = str![[r#"
error
  |
1 | abcd abcd a...
  | ^^^^
  |
"#]];
    let renderer = Renderer::plain().term_width(18);
    assert_data_eq!(renderer.render(input).to_string(), expected);
}

#[test]
fn long_line_cut_custom() {
    let source = "abcd abcd abcd abcd abcd abcd abcd";
    let input = Level::Error.title("").snippet(
        Snippet::source(source)
            .line_start(1)
            .annotation(Level::Error.span(0..4)),
    );
    // This trims a little less because `…` is visually smaller than `...`.
    let expected = str![[r#"
error
  |
1 | abcd abcd abc…
  | ^^^^
  |
"#]];
    let renderer = Renderer::plain().term_width(18).cut_indicator("…");
    assert_data_eq!(renderer.render(input).to_string(), expected);
}
