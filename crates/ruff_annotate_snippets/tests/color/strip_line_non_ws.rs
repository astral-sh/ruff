use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn case() {
    let source = r#"	let _: () = (); let _: () = (); let _: () = (); let _: () = (); let _: () = (); let _: () = (); let _: () = (); let _: () = (); let _: () = (); let _: () = (); let _: () = (); let _: () = (); let _: () = (); let _: () = (); let _: () = 42; let _: () = (); let _: () = (); let _: () = (); let _: () = (); let _: () = (); let _: () = (); let _: () = (); let _: () = ();
"#;

    let input = &[Level::ERROR
        .primary_title("mismatched types")
        .id("E0308")
        .element(
            Snippet::source(source)
                .path("$DIR/non-whitespace-trimming.rs")
                .line_start(4)
                .annotation(
                    AnnotationKind::Primary
                        .span(237..239)
                        .label("expected `()`, found integer"),
                )
                .annotation(
                    AnnotationKind::Primary
                        .span(232..234)
                        .label("expected due to this"),
                ),
        )];

    let expected_ascii = file!["strip_line_non_ws.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = file!["strip_line_non_ws.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
