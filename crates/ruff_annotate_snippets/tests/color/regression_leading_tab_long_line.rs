use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn case() {
    let source = "                                       s_data['d_dails'] = bb['contacted'][hostip]['ansible_facts']['ansible_devices']['vda']['vendor'] + \" \" + bb['contacted'][hostip]['an";

    let input = &[Level::ERROR
        .primary_title("mismatched types")
        .id("E0308")
        .element(
            Snippet::source(source)
                .path("$DIR/non-whitespace-trimming.rs")
                .line_start(4)
                .annotation(AnnotationKind::Primary.span(5..11)),
        )];

    let expected_ascii = file!["regression_leading_tab_long_line.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = file!["regression_leading_tab_long_line.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
