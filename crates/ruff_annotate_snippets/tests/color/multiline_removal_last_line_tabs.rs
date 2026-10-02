use annotate_snippets::{Group, Level, Patch, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn test() {
    let report = &[Group::with_level(Level::ERROR).element(
        Snippet::source("a.foo(|t| {\n\t\t\tb\n\t\t\t}).bar()\n").patch(Patch::new(1..22, "")),
    )];

    let expected_ascii = file!["multiline_removal_last_line_tabs.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(report), expected_ascii);

    let expected_unicode = file!["multiline_removal_last_line_tabs.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(report), expected_unicode);
}
