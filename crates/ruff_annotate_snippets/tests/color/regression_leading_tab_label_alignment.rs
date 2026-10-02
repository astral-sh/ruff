use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn case() {
    let source = "def f():\n\treturn (1 == '2')()  # Tab indented\n";

    let input = &[Level::ERROR
        .primary_title("call-non-callable")
        .id("E0308")
        .element(
            Snippet::source(source)
                .path("$DIR/main.py")
                .line_start(4)
                .annotation(AnnotationKind::Primary.span(17..29)),
        )];

    let expected_ascii = file!["regression_leading_tab_label_alignment.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode =
        file!["regression_leading_tab_label_alignment.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
