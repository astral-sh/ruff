use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn case() {
    let source = r#"                        if let DisplayLine::Source {
                            ref mut inline_marks,
                        } = body[body_idx]
"#;

    let input = &[Level::ERROR
        .primary_title("pattern does not mention fields `lineno`, `content`")
        .id("E0027")
        .element(
            Snippet::source(source)
                .path("src/display_list.rs")
                .line_start(139)
                .fold(false)
                .annotation(
                    AnnotationKind::Primary
                        .span(31..128)
                        .label("missing fields `lineno`, `content`"),
                ),
        )];

    let expected_ascii = file!["ann_multiline.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = file!["ann_multiline.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
