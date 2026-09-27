use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn case() {
    let source = r#"fn add_title_line(result: &mut Vec<String>, main_annotation: Option<&Annotation>) {
    if let Some(annotation) = main_annotation {
        result.push(format_title_line(
            &annotation.annotation_type,
            None,
            &annotation.label,
        ));
    }
}
"#;

    let input = &[Level::ERROR.primary_title("").element(
        Snippet::source(source)
            .line_start(96)
            .fold(false)
            .annotation(
                AnnotationKind::Primary
                    .span(100..110)
                    .label("Variable defined here"),
            )
            .annotation(
                AnnotationKind::Primary
                    .span(184..194)
                    .label("Referenced here"),
            )
            .annotation(
                AnnotationKind::Primary
                    .span(243..253)
                    .label("Referenced again here"),
            ),
    )];

    let expected_ascii = file!["multiple_annotations.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = file!["multiple_annotations.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
