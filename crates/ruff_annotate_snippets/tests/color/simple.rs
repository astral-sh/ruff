use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn case() {
    let source = r#"        })

        for line in &self.body {
"#;

    let input = &[Level::ERROR
        .primary_title("expected one of `.`, `;`, `?`, or an operator, found `for`")
        .element(
            Snippet::source(source)
                .path("src/format_color.rs")
                .line_start(169)
                .annotation(
                    AnnotationKind::Primary
                        .span(20..23)
                        .label("unexpected token"),
                )
                .annotation(
                    AnnotationKind::Context
                        .span(10..11)
                        .label("expected one of `.`, `;`, `?`, or an operator here"),
                ),
        )];

    let expected_ascii = file!["simple.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = file!["simple.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
