use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn case() {
    let source = r#"
[workspace.lints.rust]
rust_2018_idioms = { level = "warn", priority = -1 }
unnameable_types = "warn"
unreachable_pub = "warn"
unsafe_op_in_unsafe_fn = "warn"
unused_lifetimes = "warn"
unused_macro_rules = "warn"
unused_qualifications = "warn"
"#;

    let input = &[Level::ERROR
        .primary_title("all annotation kinds have a highlighted source")
        .element(
            Snippet::source(source)
                .path("Cargo.toml")
                .annotation(
                    AnnotationKind::Primary
                        .span(214..235)
                        .highlight_source(true),
                )
                .annotation(
                    AnnotationKind::Context
                        .span(238..244)
                        .highlight_source(true),
                )
                .annotation(AnnotationKind::Visible.span(2..22).highlight_source(true)),
        )];

    let expected_ascii = file!["highlight_source.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = file!["highlight_source.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
