use annotate_snippets::{AnnotationKind, Level, Patch, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn case() {
    let source = r#"struct 啊啊啊啊 {}

const 哦哦: 啊啊啊啊 = 哈哈哈哈 {}; // some comment"#;

    let path = "$DIR/highlight_diff_line_with_wide_characters.rs";

    let report = &[
        Level::ERROR
            .primary_title("cannot find struct, variant or union type `哈哈哈哈` in this scope")
            .id("E0422")
            .element(
                Snippet::source(source)
                    .path(path)
                    .annotation(AnnotationKind::Primary.span(53..65).label("here"))
                    .annotation(
                        AnnotationKind::Context
                            .span(0..22)
                            .label("similarly named struct `啊啊啊啊` defined here"),
                    ),
            ),
        Level::HELP
            .secondary_title("a struct with a similar name exists: `啊啊啊啊`")
            .element(
                Snippet::source(source)
                    .path(path)
                    .patch(Patch::new(53..65, "啊啊啊啊")),
            ),
    ];

    let expected_ascii = file!["highlight_diff_line_with_wide_characters.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(report), expected_ascii);

    let expected_unicode =
        file!["highlight_diff_line_with_wide_characters.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(report), expected_unicode);
}
