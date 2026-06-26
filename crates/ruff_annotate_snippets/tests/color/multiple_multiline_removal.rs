use annotate_snippets::{
    AnnotationKind, Level, Origin, Padding, Patch, Renderer, Snippet, renderer::DecorStyle,
};

use snapbox::{assert_data_eq, file};

#[test]
fn case() {
    // https://github.com/rust-lang/rust/blob/4b94758d2ba7d0ef71ccf5fde29ce4bc5d6fe2a4/tests/ui/suggestions/multi-suggestion.rs

    let source = r#"#![allow(dead_code)]
struct U <T> {
    wtf: Option<Box<U<T>>>,
    x: T,
}
fn main() {
    U {
        wtf: Some(Box(U {
            wtf: None,
            x: (),
        })),
        x: ()
    };
    let _ = std::collections::HashMap();
    let _ = std::collections::HashMap {};
    let _ = Box {};
}
"#;
    let path = "$DIR/suggest-box-new.rs";
    let secondary_path = "$SRC_DIR/alloc/src/boxed.rs";

    let report = &[
        Level::ERROR
            .primary_title("cannot initialize a tuple struct which contains private fields")
            .id("E0423")
            .element(
                Snippet::source(source)
                    .path(path)
                    .annotation(AnnotationKind::Primary.span(114..117)),
            ),
        Level::NOTE
            .secondary_title("constructor is not visible here due to private fields")
            .element(Origin::path(secondary_path).line(234).char_column(2))
            .element(Padding)
            .element(Level::NOTE.message("private field"))
            .element(Padding)
            .element(Level::NOTE.message("private field")),
        Level::HELP
            .secondary_title(
                "you might have meant to use an associated function to build this type",
            )
            .element(
                Snippet::source(source)
                    .path(path)
                    .patch(Patch::new(117..174, "::new(_)")),
            )
            .element(
                Snippet::source(source)
                    .path(path)
                    .patch(Patch::new(117..174, "::new_uninit()")),
            )
            .element(
                Snippet::source(source)
                    .path(path)
                    .patch(Patch::new(117..174, "::new_zeroed()")),
            )
            .element(
                Snippet::source(source)
                    .path(path)
                    .patch(Patch::new(117..174, "::new_in(_, _)")),
            )
            .element(Level::NOTE.no_name().message("and 12 other candidates")),
        Level::HELP
            .secondary_title("consider using the `Default` trait")
            .element(
                Snippet::source(source)
                    .path(path)
                    .patch(Patch::new(114..114, "<"))
                    .patch(Patch::new(
                        117..174,
                        " as std::default::Default>::default()",
                    )),
            ),
    ];

    let expected_ascii = file!["multiple_multiline_removal.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(report), expected_ascii);

    let expected_unicode = file!["multiple_multiline_removal.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(report), expected_unicode);
}
