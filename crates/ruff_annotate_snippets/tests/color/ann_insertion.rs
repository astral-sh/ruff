use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn case() {
    let input = &[Level::ERROR.primary_title("expected `.`, `=`").element(
        Snippet::source("asf")
            .path("Cargo.toml")
            .line_start(1)
            .annotation(AnnotationKind::Primary.span(2..2).label("'d' belongs here")),
    )];

    let expected_ascii = file!["ann_insertion.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = file!["ann_insertion.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
