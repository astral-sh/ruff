use std::collections::BTreeSet;

#[test]
fn custom_error() {
    let target = "custom_error";
    let expected = snapbox::file!["../examples/custom_error.svg": TermSvg];
    assert_example(target, expected);
}

#[test]
fn custom_level() {
    let target = "custom_level";
    let expected = snapbox::file!["../examples/custom_level.svg": TermSvg];
    assert_example(target, expected);
}

#[test]
fn elide_header() {
    let target = "elide_header";
    let expected = snapbox::file!["../examples/elide_header.svg": TermSvg];
    assert_example(target, expected);
}

#[test]
fn expected_type() {
    let target = "expected_type";
    let expected = snapbox::file!["../examples/expected_type.svg": TermSvg];
    assert_example(target, expected);
}

#[test]
fn footer() {
    let target = "footer";
    let expected = snapbox::file!["../examples/footer.svg": TermSvg];
    assert_example(target, expected);
}

#[test]
fn format() {
    let target = "format";
    let expected = snapbox::file!["../examples/format.svg": TermSvg];
    assert_example(target, expected);
}

#[test]
fn highlight_source() {
    let target = "highlight_source";
    let expected = snapbox::file!["../examples/highlight_source.svg": TermSvg];
    assert_example(target, expected);
}

#[test]
fn highlight_message() {
    let target = "highlight_message";
    let expected = snapbox::file!["../examples/highlight_message.svg": TermSvg];
    assert_example(target, expected);
}

#[test]
fn id_hyperlink() {
    let target = "id_hyperlink";
    let expected = snapbox::file!["../examples/id_hyperlink.svg": TermSvg];
    assert_example(target, expected);
}

#[test]
fn multislice() {
    let target = "multislice";
    let expected = snapbox::file!["../examples/multislice.svg": TermSvg];
    assert_example(target, expected);
}

#[test]
fn multi_suggestion() {
    let target = "multi_suggestion";
    let expected = snapbox::file!["../examples/multi_suggestion.svg": TermSvg];
    assert_example(target, expected);
}

#[test]
fn struct_name_as_context() {
    let target = "struct_name_as_context";
    let expected = snapbox::file!["../examples/struct_name_as_context.svg": TermSvg];
    assert_example(target, expected);
}

#[track_caller]
fn assert_example(target: &str, expected: snapbox::Data) {
    let bin_path = snapbox::cmd::compile_example(target, ["--features=testing-colors"]).unwrap();
    snapbox::cmd::Command::new(bin_path)
        .env("CLICOLOR_FORCE", "1")
        .assert()
        .success()
        .stdout_eq(expected.raw());
}

#[test]
fn ensure_all_examples_have_tests() {
    let path = snapbox::utils::current_rs!();
    let actual = std::fs::read_to_string(&path).unwrap();
    let actual = actual
        .lines()
        .filter_map(|l| {
            if l.starts_with("fn ")
                && !l.starts_with("fn all_examples_have_tests")
                && !l.starts_with("fn assert_example")
            {
                Some(l[3..l.len() - 4].to_string())
            } else {
                None
            }
        })
        .collect::<BTreeSet<_>>();

    let expected = std::fs::read_dir("examples")
        .unwrap()
        .map(|res| res.map(|e| e.path().file_stem().unwrap().display().to_string()))
        .collect::<Result<BTreeSet<_>, std::io::Error>>()
        .unwrap();

    let mut diff = expected.difference(&actual).collect::<Vec<_>>();
    diff.sort();

    let mut need_added = String::new();
    for name in &diff {
        need_added.push_str(&format!("{name}\n"));
    }
    assert!(
        diff.is_empty(),
        "\n`Please add a test for the following examples to `tests/examples.rs`:\n{need_added}",
    );
}
