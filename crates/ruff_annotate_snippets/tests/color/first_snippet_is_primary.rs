use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn case() {
    let file_txt_source = r#"this is from a txt file"#;

    let rust_source = r#"fn main() {
    let b: &[u8] = include_str!("file.txt");
    let s: &str = include_bytes!("file.txt");
}"#;

    let input = &[Level::ERROR
        .primary_title("mismatched types")
        .id("E0308")
        .element(
            Snippet::source(file_txt_source)
                .line_start(3)
                .path("$DIR/file.txt")
                .annotation(
                    AnnotationKind::Context
                        .span(0..23)
                        .label("the macro expands to this string"),
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
                    AnnotationKind::Primary
                        .span(31..55)
                        .label("expected `&[u8]`, found `&str`"),
                ),
        )
        .element(
            Level::NOTE.message("expected reference `&[u8]`\n   found reference `&'static str`"),
        )];

    let expected_ascii = file!["first_snippet_is_primary.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = file!["first_snippet_is_primary.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
