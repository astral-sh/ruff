use annotate_snippets::{Group, Level, Patch, Renderer, Snippet, renderer::DecorStyle};

use snapbox::{assert_data_eq, file};

#[test]
fn test() {
    let report = &[Group::with_level(Level::ERROR).element(
        Snippet::source("do\n  local function f()\n    print()\n  end\nend\n")
            .patch(Patch::new(5..41, "")),
    )];

    let expected_ascii = file!["multiline_removal_indent.ascii.term.svg": TermSvg];
    let renderer = Renderer::styled();
    assert_data_eq!(renderer.render(report), expected_ascii);

    let expected_unicode = file!["multiline_removal_indent.unicode.term.svg": TermSvg];
    let renderer = renderer.decor_style(DecorStyle::Unicode);
    assert_data_eq!(renderer.render(report), expected_unicode);
}
