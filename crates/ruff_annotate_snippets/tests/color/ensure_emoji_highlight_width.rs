use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn case() {
    let source = r#""haha this isn't a valid name 🐛" = { package = "libc", version = "0.1" }
"#;

    let input = &[Level::ERROR.primary_title("invalid character ` ` in package name: `haha this isn't a valid name 🐛`, characters must be Unicode XID characters (numbers, `-`, `_`, or most letters)")
        .element(
            Snippet::source(source)
                .path("<file>")
                .line_start(7)
                .annotation(AnnotationKind::Primary.span(0..35).label(""))
        )];

    let expected_ascii = file!["ensure_emoji_highlight_width.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = file!["ensure_emoji_highlight_width.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
