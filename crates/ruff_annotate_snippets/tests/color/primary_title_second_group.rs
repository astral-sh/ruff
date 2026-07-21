use annotate_snippets::{AnnotationKind, Group, Level, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn case() {
    let report = &[
        Level::ERROR
            .primary_title("mismatched types")
            .id("E0308")
            .element(
                Snippet::source("        slices: vec![\"A\",")
                    .line_start(13)
                    .path("src/multislice.rs")
                    .annotation(AnnotationKind::Primary.span(21..24).label(
                        "expected struct `annotate_snippets::snippet::Slice`, found reference",
                    )),
            ),
        Group::with_title(Level::NOTE.primary_title(
            "expected type: `snippet::Annotation`\n   found type: `__&__snippet::Annotation`",
        )),
    ];

    let expected_ascii = file!["primary_title_second_group.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(report), expected_ascii);

    let expected_unicode = file!["primary_title_second_group.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(report), expected_unicode);
}
