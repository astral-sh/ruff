use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn case() {
    let source = r#"

invalid syntax
"#;

    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .path("path/to/error.rs")
            .line_start(1)
            .annotation(AnnotationKind::Context.span(2..16).label("error here")),
    )];

    let expected_ascii = file!["fold_bad_origin_line.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = file!["fold_bad_origin_line.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
