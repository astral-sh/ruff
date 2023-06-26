use ruff_python_formatter::format_module;
use similar::TextDiff;
use std::fmt::{Formatter, Write};
use std::fs;

#[test]
fn black_compatibility() {
    insta::glob!(
        "../resources",
        "test/fixtures/black/**/*.py",
        |input_path| {
            let content = fs::read_to_string(input_path).unwrap();

            let printed = format_module(&content).expect("Formatting to succeed");

            let expected_path = input_path.with_extension("py.expect");
            let expected_output = fs::read_to_string(&expected_path).unwrap_or_else(|_| {
                panic!("Expected Black output file '{expected_path:?}' to exist")
            });

            let formatted_code = printed.as_code();

            ensure_stability_when_formatting_twice(formatted_code);

            if formatted_code == expected_output {
                // Black and Ruff formatting matches. Delete any existing snapshot files because the Black output
                // already perfectly captures the expected output.
                // The following code mimics insta's logic generating the snapshot name for a test.
                let workspace_path = std::env::var("CARGO_MANIFEST_DIR").unwrap();
                let snapshot_name = insta::_function_name!()
                    .strip_prefix(&format!("{}::", module_path!()))
                    .unwrap();
                let module_path = module_path!().replace("::", "__");

                let snapshot_path = Path::new(&workspace_path)
                    .join("src/snapshots")
                    .join(format!(
                        "{module_path}__{}.snap",
                        snapshot_name.replace(&['/', '\\'][..], "__")
                    ));

                if snapshot_path.exists() && snapshot_path.is_file() {
                    // SAFETY: This is a convenience feature. That's why we don't want to abort
                    // when deleting a no longer needed snapshot fails.
                    fs::remove_file(&snapshot_path).ok();
                }

                let new_snapshot_path = snapshot_path.with_extension("snap.new");
                if new_snapshot_path.exists() && new_snapshot_path.is_file() {
                    // SAFETY: This is a convenience feature. That's why we don't want to abort
                    // when deleting a no longer needed snapshot fails.
                    fs::remove_file(&new_snapshot_path).ok();
                }
            } else {
                // Black and Ruff have different formatting. Write out a snapshot that covers the differences
                // today.
                let mut snapshot = String::new();
                write!(snapshot, "{}", Header::new("Input")).unwrap();
                write!(snapshot, "{}", CodeFrame::new("py", &content)).unwrap();

                write!(snapshot, "{}", Header::new("Black Differences")).unwrap();

                let diff = TextDiff::from_lines(expected_output.as_str(), formatted_code)
                    .unified_diff()
                    .header("Black", "Ruff")
                    .to_string();

                write!(snapshot, "{}", CodeFrame::new("diff", &diff)).unwrap();

                write!(snapshot, "{}", Header::new("Ruff Output")).unwrap();
                write!(snapshot, "{}", CodeFrame::new("py", formatted_code)).unwrap();

                write!(snapshot, "{}", Header::new("Black Output")).unwrap();
                write!(snapshot, "{}", CodeFrame::new("py", &expected_output)).unwrap();

                insta::with_settings!({
                    omit_expression => true,
                    input_file => input_path,
                    prepend_module_to_snapshot => false,
                }, {
                    insta::assert_snapshot!(snapshot);
                });
            }
        }
    );
}

#[test]
fn format() {
    insta::glob!("../resources", "test/fixtures/ruff/**/*.py", |input_path| {
        let content = fs::read_to_string(input_path).unwrap();

        let printed = format_module(&content).expect("Formatting to succeed");
        let formatted_code = printed.as_code();

        ensure_stability_when_formatting_twice(formatted_code);

        let snapshot = format!(
            r#"## Input
{}

## Output
{}"#,
            CodeFrame::new("py", &content),
            CodeFrame::new("py", formatted_code)
        );

        insta::with_settings!({
            omit_expression => true,
            input_file => input_path,
            prepend_module_to_snapshot => false,
        }, {
            insta::assert_snapshot!(snapshot);
        });
    });
}

/// Format another time and make sure that there are no changes anymore
fn ensure_stability_when_formatting_twice(formatted_code: &str) {
    let reformatted = match format_module(formatted_code) {
        Ok(reformatted) => reformatted,
        Err(err) => {
            panic!(
                "Expected formatted code to be valid syntax: {err}:\
                    \n---\n{formatted_code}---\n",
            );
        }
    };

    if reformatted.as_code() != formatted_code {
        let diff = TextDiff::from_lines(formatted_code, reformatted.as_code())
            .unified_diff()
            .header("Formatted once", "Formatted twice")
            .to_string();
        panic!(
            r#"Reformatting the formatted code a second time resulted in formatting changes.
---
{diff}---

Formatted once:
---
{formatted_code}---

Formatted twice:
---
{}---"#,
            reformatted.as_code()
        );
    }
}

struct Header<'a> {
    title: &'a str,
}

impl<'a> Header<'a> {
    fn new(title: &'a str) -> Self {
        Self { title }
    }
}

impl std::fmt::Display for Header<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "## {}", self.title)?;
        writeln!(f)
    }
}

struct CodeFrame<'a> {
    language: &'a str,
    code: &'a str,
}

impl<'a> CodeFrame<'a> {
    fn new(language: &'a str, code: &'a str) -> Self {
        Self { language, code }
    }
}

impl std::fmt::Display for CodeFrame<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "```{}", self.language)?;
        write!(f, "{}", self.code)?;
        writeln!(f, "```")?;
        writeln!(f)
    }
}
