use annotate_snippets::{AnnotationKind, Group, Level, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn case() {
    let source = r#"[résumé] [naïve]"#;
    let report = &[Group::with_level(Level::WARNING).element(
        Snippet::source(source)
            .annotation(AnnotationKind::Primary.span(1..11).highlight_source(true))
            .annotation(AnnotationKind::Primary.span(14..21).highlight_source(true)),
    )];

    let expected_ascii = file!["highlight_source_zero_width_chars.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(report), expected_ascii);

    let expected_unicode = file!["highlight_source_zero_width_chars.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(report), expected_unicode);
}
