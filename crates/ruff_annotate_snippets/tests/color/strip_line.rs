use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn case() {
    let source = r#"                                                                                                                                                                                    let _: () = 42;"#;

    let input = &[Level::ERROR
        .primary_title("mismatched types")
        .id("E0308")
        .element(
            Snippet::source(source)
                .path("$DIR/whitespace-trimming.rs")
                .line_start(4)
                .annotation(
                    AnnotationKind::Primary
                        .span(192..194)
                        .label("expected (), found integer"),
                ),
        )];

    let expected_ascii = file!["strip_line.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = file!["strip_line.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
