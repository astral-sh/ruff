use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn case() {
    let source = r#"This is an example
of an edge case of an annotation overflowing
to exactly one character on next line.
"#;

    let input = &[Level::ERROR
        .primary_title("spacing error found")
        .id("E####")
        .element(
            Snippet::source(source)
                .path("foo.txt")
                .line_start(26)
                .fold(false)
                .annotation(
                    AnnotationKind::Primary
                        .span(11..19)
                        .label("this should not be on separate lines"),
                ),
        )];

    let expected_ascii = file!["ann_multiline2.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = file!["ann_multiline2.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
