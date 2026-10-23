use annotate_snippets::{AnnotationKind, Level, Patch, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn case() {
    let source = r#"struct Thingy;

impl Thingy {
    pub(crate) fn new(
        _a: String,
        _b: String,
        _c: String,
        _d: String,
        _e: String,
        _f: String,
    ) -> Self {
        unimplemented!()
    }
}

fn main() {
    let foo = Thingy::new(
        String::from(""),
        String::from(""),
        String::from(""),
        String::from(""),
        String::from(""),
        String::from(""),
        String::from(""),
    );
}"#;

    let path = "$DIR/wrong-highlight-span-extra-arguments-147070.rs";

    let report = &[
        Level::ERROR
            .primary_title("this function takes 6 arguments but 7 arguments were supplied")
            .id("E0061")
            .element(
                Snippet::source(source)
                    .path(path)
                    .annotation(
                        AnnotationKind::Context
                            .span(426..442)
                            .label("unexpected argument #7 of type `String`"),
                    )
                    .annotation(AnnotationKind::Primary.span(249..260)),
            ),
        Level::NOTE
            .secondary_title("associated function defined here")
            .element(
                Snippet::source(source)
                    .path(path)
                    .annotation(AnnotationKind::Primary.span(48..51)),
            ),
        Level::HELP
            .secondary_title("remove the extra argument")
            .element(
                Snippet::source(source)
                    .path(path)
                    .patch(Patch::new(416..442, "")),
            )
            .element(
                Snippet::source(source)
                    .path(path)
                    .patch(Patch::new(263..289, "")),
            )
            .element(
                Snippet::source(source)
                    .path(path)
                    .patch(Patch::new(286..312, "")),
            )
            .element(
                Snippet::source(source)
                    .path(path)
                    .patch(Patch::new(400..417, "")),
            )
            .element(
                Snippet::source(source)
                    .path(path)
                    .patch(Patch::new(416..442, "")),
            ),
    ];

    let expected_ascii = file!["multiple_highlight_duplicated.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(report), expected_ascii);

    let expected_unicode = file!["multiple_highlight_duplicated.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(report), expected_unicode);
}
