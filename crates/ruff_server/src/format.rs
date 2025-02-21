use std::path::Path;

use ruff_formatter::PrintedRange;
use ruff_python_ast::PySourceType;
use ruff_python_formatter::{format_module_source, FormatModuleError};
use ruff_text_size::TextRange;
use ruff_workspace::FormatterSettings;

use crate::edit::TextDocument;

pub(crate) fn format(
    document: &TextDocument,
    source_type: PySourceType,
    formatter_settings: &FormatterSettings,
    path: Option<&Path>,
) -> crate::Result<Option<String>> {
    let format_options =
        formatter_settings.to_format_options(source_type, document.contents(), path);
    match format_module_source(document.contents(), format_options) {
        Ok(formatted) => {
            let formatted = formatted.into_code();
            if formatted == document.contents() {
                Ok(None)
            } else {
                Ok(Some(formatted))
            }
        }
        // Special case - syntax/parse errors are handled here instead of
        // being propagated as visible server errors.
        Err(FormatModuleError::ParseError(error)) => {
            tracing::warn!("Unable to format document: {error}");
            Ok(None)
        }
        Err(err) => Err(err.into()),
    }
}

pub(crate) fn format_range(
    document: &TextDocument,
    source_type: PySourceType,
    formatter_settings: &FormatterSettings,
    range: TextRange,
    path: Option<&Path>,
) -> crate::Result<Option<PrintedRange>> {
    let format_options =
        formatter_settings.to_format_options(source_type, document.contents(), path);

    match ruff_python_formatter::format_range(document.contents(), range, format_options) {
        Ok(formatted) => {
            if formatted.as_code() == document.contents() {
                Ok(None)
            } else {
                Ok(Some(formatted))
            }
        }
        // Special case - syntax/parse errors are handled here instead of
        // being propagated as visible server errors.
        Err(FormatModuleError::ParseError(error)) => {
            tracing::warn!("Unable to format document range: {error}");
            Ok(None)
        }
        Err(err) => Err(err.into()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use insta::assert_snapshot;
    use ruff_linter::settings::types::{CompiledPerFileTargetVersionList, PerFileTargetVersion};
    use ruff_python_ast::{PySourceType, PythonVersion};
    use ruff_text_size::{TextRange, TextSize};
    use ruff_workspace::FormatterSettings;

    use crate::format::{format, format_range};
    use crate::TextDocument;

    #[test]
    fn format_per_file_version() {
        let document = TextDocument::new(r#"
with open("a_really_long_foo") as foo, open("a_really_long_bar") as bar, open("a_really_long_baz") as baz:
    pass
"#.to_string(), 0);
        let per_file_target_version =
            CompiledPerFileTargetVersionList::resolve(vec![PerFileTargetVersion::new(
                "test.py".to_string(),
                PythonVersion::PY310,
                Some(Path::new(".")),
            )])
            .unwrap();
        let result = format(
            &document,
            PySourceType::Python,
            &FormatterSettings {
                unresolved_target_version: PythonVersion::PY38,
                per_file_target_version,
                ..Default::default()
            },
            Some(Path::new("test.py")),
        )
        .expect("Expected no errors when formatting")
        .expect("Expected formatting changes");

        assert_snapshot!(result, @r#"
        with (
            open("a_really_long_foo") as foo,
            open("a_really_long_bar") as bar,
            open("a_really_long_baz") as baz,
        ):
            pass
        "#);

        // same as above but without the per_file_target_version override
        let result = format(
            &document,
            PySourceType::Python,
            &FormatterSettings {
                unresolved_target_version: PythonVersion::PY38,
                ..Default::default()
            },
            Some(Path::new("test.py")),
        )
        .expect("Expected no errors when formatting")
        .expect("Expected formatting changes");

        assert_snapshot!(result, @r#"
        with open("a_really_long_foo") as foo, open("a_really_long_bar") as bar, open(
            "a_really_long_baz"
        ) as baz:
            pass
        "#);
    }

    #[test]
    fn format_per_file_version_range() -> anyhow::Result<()> {
        // prepare a document with formatting changes before and after the intended range (the
        // context manager)
        let document = TextDocument::new(r#"
def fn(x: str) -> Foo | Bar: return foobar(x)

with open("a_really_long_foo") as foo, open("a_really_long_bar") as bar, open("a_really_long_baz") as baz:
    pass

sys.exit(
1
)
"#.to_string(), 0);

        let start = document.contents().find("with").unwrap();
        let end = document.contents().find("pass").unwrap() + "pass".len();
        let range = TextRange::new(TextSize::try_from(start)?, TextSize::try_from(end)?);

        let per_file_target_version =
            CompiledPerFileTargetVersionList::resolve(vec![PerFileTargetVersion::new(
                "test.py".to_string(),
                PythonVersion::PY310,
                Some(Path::new(".")),
            )])
            .unwrap();
        let result = format_range(
            &document,
            PySourceType::Python,
            &FormatterSettings {
                unresolved_target_version: PythonVersion::PY38,
                per_file_target_version,
                ..Default::default()
            },
            range,
            Some(Path::new("test.py")),
        )
        .expect("Expected no errors when formatting")
        .expect("Expected formatting changes");

        assert_snapshot!(result.as_code(), @r#"
        with (
            open("a_really_long_foo") as foo,
            open("a_really_long_bar") as bar,
            open("a_really_long_baz") as baz,
        ):
            pass
        "#);

        // same as above but without the per_file_target_version override
        let result = format_range(
            &document,
            PySourceType::Python,
            &FormatterSettings {
                unresolved_target_version: PythonVersion::PY38,
                ..Default::default()
            },
            range,
            Some(Path::new("test.py")),
        )
        .expect("Expected no errors when formatting")
        .expect("Expected formatting changes");

        assert_snapshot!(result.as_code(), @r#"
        with open("a_really_long_foo") as foo, open("a_really_long_bar") as bar, open(
            "a_really_long_baz"
        ) as baz:
            pass
        "#);

        Ok(())
    }
}
