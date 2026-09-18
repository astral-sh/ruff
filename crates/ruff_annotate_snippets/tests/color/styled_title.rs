use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet, renderer::DecorStyle};
use anstyle::{AnsiColor, Effects, Style};

use snapbox::{assert_data_eq, file};

const MAGENTA: Style = AnsiColor::Magenta.on_default().effects(Effects::BOLD);
const BOLD: Style = Style::new().effects(Effects::BOLD);
#[test]
fn case() {
    let source = r#"use b::CustomErrorHandler;
use c::cnb_runtime;


    cnb_runtime(CustomErrorHandler {});
"#;

    let title_1 = "the trait bound `CustomErrorHandler: ErrorHandler` is not satisfied";
    let title_2 = format!(
        "{BOLD}there are {BOLD:#}{MAGENTA}multiple different versions{MAGENTA:#}{BOLD} of crate `{BOLD:#}{MAGENTA}c{MAGENTA:#}{BOLD}` in the dependency graph{BOLD:#}"
    );

    let label_1 = "the trait `ErrorHandler` is not implemented for `CustomErrorHandler`";
    let label_2 = "required by a bound introduced by this call";
    let label_3 = "one version of crate `c` is used here, as a dependency of crate `b`";
    let label_4 =
        "one version of crate `c` is used here, as a direct dependency of the current crate";

    let input = &[
        Level::ERROR.primary_title(title_1).id("E0277").element(
            Snippet::source(source)
                .path("src/main.rs")
                .annotation(AnnotationKind::Primary.span(65..86).label(label_1))
                .annotation(AnnotationKind::Context.span(53..64).label(label_2)),
        ),
        Level::HELP.secondary_title(title_2).element(
            Snippet::source(source)
                .path("src/main.rs")
                .annotation(AnnotationKind::Primary.span(4..5).label(label_3))
                .annotation(AnnotationKind::Primary.span(31..32).label(label_4)),
        ),
    ];

    let expected_ascii = file!["styled_title.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(input), expected_ascii);

    let expected_unicode = file!["styled_title.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(input), expected_unicode);
}
