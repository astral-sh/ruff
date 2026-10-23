use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn case() {
    let source = r#") -> Option<String> {
    for ann in annotations {
        match (ann.range.0, ann.range.1) {
            (None, None) => continue,
            (Some(start), Some(end)) if start > end_index || end < start_index => continue,
            (Some(start), Some(end)) if start >= start_index && end <= end_index => {
                let label = if let Some(ref label) = ann.label {
                    format!(" {}", label)
                } else {
                    String::from("")
                };

                return Some(format!(
                    "{}{}{}",
                    " ".repeat(start - start_index),
                    "^".repeat(end - start),
                    label
                ));
            }
            _ => continue,
        }
    }
"#;

    let input = &[Level::ERROR
        .primary_title("mismatched types")
        .id("E0308")
        .element(
            Snippet::source(source)
                .path("src/format.rs")
                .line_start(51)
                .annotation(AnnotationKind::Context.span(5..19).label(
                    "expected `std::option::Option<std::string::String>` because of return type",
                ))
                .annotation(
                    AnnotationKind::Primary
                        .span(22..766)
                        .label("expected enum `std::option::Option`, found ()"),
                ),
        )];

    let expected_ascii = file!["fold_ann_multiline.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = file!["fold_ann_multiline.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
