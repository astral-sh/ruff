use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn case() {
    let source = r#"[workspace]

[package]
name = "hello"
version = "1.0.0"
license = "MIT"
rust-version = "1.70"
edition = "2021"

[lints]
workspace = 20
"#;

    let input = &[Level::ERROR
        .primary_title("invalid type: integer `20`, expected a bool")
        .id("E0308")
        .element(
            Snippet::source(source)
                .path("Cargo.toml")
                .line_start(1)
                .annotation(AnnotationKind::Primary.span(132..134).label("")),
        )];

    let expected_ascii = file!["fold_leading.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = file!["fold_leading.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
