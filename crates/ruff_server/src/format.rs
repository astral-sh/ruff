use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::Context;

use ruff_formatter::{FormatOptions, PrintedRange};
use ruff_python_ast::PySourceType;
use ruff_python_formatter::{FormatModuleError, PyFormatOptions, format_module_source};
use ruff_source_file::LineIndex;
use ruff_text_size::TextRange;
use ruff_workspace::FormatterSettings;

use crate::edit::TextDocument;

/// The backend to use for formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FormatBackend {
    /// Use the built-in Ruff formatter.
    ///
    /// The formatter version will match the LSP version.
    #[default]
    Internal,
    /// Use uv for formatting.
    ///
    /// The formatter version may differ from the LSP version.
    Uv,
}

pub(crate) fn format(
    document: &TextDocument,
    source_type: PySourceType,
    formatter_settings: &FormatterSettings,
    path: &Path,
    backend: FormatBackend,
) -> crate::Result<Option<String>> {
    match backend {
        FormatBackend::Uv => format_external(document, source_type, formatter_settings, path),
        FormatBackend::Internal => format_internal(document, source_type, formatter_settings, path),
    }
}

/// Format using the built-in Ruff formatter.
fn format_internal(
    document: &TextDocument,
    source_type: PySourceType,
    formatter_settings: &FormatterSettings,
    path: &Path,
) -> crate::Result<Option<String>> {
    let format_options =
        formatter_settings.to_format_options(source_type, document.contents(), Some(path));
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

/// Format using an external uv command.
fn format_external(
    document: &TextDocument,
    source_type: PySourceType,
    formatter_settings: &FormatterSettings,
    path: &Path,
) -> crate::Result<Option<String>> {
    let format_options =
        formatter_settings.to_format_options(source_type, document.contents(), Some(path));
    let uv_command = UvFormatCommand::from(format_options);
    uv_command.format_document(document.contents(), path)
}

pub(crate) fn format_range(
    document: &TextDocument,
    source_type: PySourceType,
    formatter_settings: &FormatterSettings,
    range: TextRange,
    path: &Path,
    backend: FormatBackend,
) -> crate::Result<Option<PrintedRange>> {
    match backend {
        FormatBackend::Uv => {
            format_range_external(document, source_type, formatter_settings, range, path)
        }
        FormatBackend::Internal => {
            format_range_internal(document, source_type, formatter_settings, range, path)
        }
    }
}

/// Format range using the built-in Ruff formatter
fn format_range_internal(
    document: &TextDocument,
    source_type: PySourceType,
    formatter_settings: &FormatterSettings,
    range: TextRange,
    path: &Path,
) -> crate::Result<Option<PrintedRange>> {
    let format_options =
        formatter_settings.to_format_options(source_type, document.contents(), Some(path));

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

/// Format range using an external command, i.e., `uv`.
fn format_range_external(
    document: &TextDocument,
    source_type: PySourceType,
    formatter_settings: &FormatterSettings,
    range: TextRange,
    path: &Path,
) -> crate::Result<Option<PrintedRange>> {
    let format_options =
        formatter_settings.to_format_options(source_type, document.contents(), Some(path));
    let uv_command = UvFormatCommand::from(format_options);

    // Format the range using uv and convert the result to `PrintedRange`
    match uv_command.format_range(document.contents(), range, path, document.index())? {
        Some(formatted) => Ok(Some(PrintedRange::new(formatted, range))),
        None => Ok(None),
    }
}

/// Builder for uv format commands
#[derive(Debug)]
pub(crate) struct UvFormatCommand {
    options: PyFormatOptions,
}

impl From<PyFormatOptions> for UvFormatCommand {
    fn from(options: PyFormatOptions) -> Self {
        Self { options }
    }
}

impl UvFormatCommand {
    /// Build the command with all necessary arguments
    fn build_command(
        &self,
        path: &Path,
        range_with_index: Option<(TextRange, &LineIndex, &str)>,
    ) -> Command {
        let mut command = Command::new("uv");
        command.arg("format");
        command.arg("--");

        let target_version = format!(
            "py{}{}",
            self.options.target_version().major,
            self.options.target_version().minor
        );

        // Add only the formatting options that the CLI supports
        command.arg("--target-version");
        command.arg(&target_version);

        command.arg("--line-length");
        command.arg(self.options.line_width().to_string());

        if self.options.preview().is_enabled() {
            command.arg("--preview");
        }

        // Pass other formatting options via --config
        command.arg("--config");
        command.arg(format!(
            "format.indent-style = '{}'",
            self.options.indent_style()
        ));

        command.arg("--config");
        command.arg(format!("indent-width = {}", self.options.indent_width()));

        command.arg("--config");
        command.arg(format!(
            "format.quote-style = '{}'",
            self.options.quote_style()
        ));

        command.arg("--config");
        command.arg(format!(
            "format.line-ending = '{}'",
            self.options.line_ending().as_setting_str()
        ));

        command.arg("--config");
        command.arg(format!(
            "format.skip-magic-trailing-comma = {}",
            match self.options.magic_trailing_comma() {
                ruff_python_formatter::MagicTrailingComma::Respect => "false",
                ruff_python_formatter::MagicTrailingComma::Ignore => "true",
            }
        ));

        if let Some((range, line_index, source)) = range_with_index {
            // The CLI expects line:column format
            let start_pos = line_index.line_column(range.start(), source);
            let end_pos = line_index.line_column(range.end(), source);
            let range_str = format!(
                "{}:{}-{}:{}",
                start_pos.line.get(),
                start_pos.column.get(),
                end_pos.line.get(),
                end_pos.column.get()
            );
            command.arg("--range");
            command.arg(&range_str);
        }

        command.arg("--stdin-filename");
        command.arg(path.to_string_lossy().as_ref());

        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        command
    }

    /// Execute the format command on the given source.
    pub(crate) fn format(
        &self,
        source: &str,
        path: &Path,
        range_with_index: Option<(TextRange, &LineIndex)>,
    ) -> crate::Result<Option<String>> {
        let mut command =
            self.build_command(path, range_with_index.map(|(r, idx)| (r, idx, source)));
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                anyhow::bail!("uv was not found; is it installed and on the PATH?")
            }
            Err(err) => return Err(err).context("Failed to spawn uv"),
        };

        let mut stdin = child
            .stdin
            .take()
            .context("Failed to get stdin from format subprocess")?;
        stdin
            .write_all(source.as_bytes())
            .context("Failed to write to stdin")?;
        drop(stdin);

        let result = child
            .wait_with_output()
            .context("Failed to get output from format subprocess")?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            // We don't propagate format errors due to invalid syntax
            if stderr.contains("Failed to parse") {
                tracing::warn!("Unable to format document: {}", stderr);
                return Ok(None);
            }
            // Special-case for when `uv format` is not available
            if stderr.contains("unrecognized subcommand 'format'") {
                anyhow::bail!(
                    "The installed version of uv does not support `uv format`; upgrade to a newer version"
                );
            }
            anyhow::bail!("Failed to format document: {stderr}");
        }

        let formatted = String::from_utf8(result.stdout)
            .context("Failed to parse stdout from format subprocess as utf-8")?;

        if formatted == source {
            Ok(None)
        } else {
            Ok(Some(formatted))
        }
    }

    /// Format the entire document.
    pub(crate) fn format_document(
        &self,
        source: &str,
        path: &Path,
    ) -> crate::Result<Option<String>> {
        self.format(source, path, None)
    }

    /// Format a specific range.
    pub(crate) fn format_range(
        &self,
        source: &str,
        range: TextRange,
        path: &Path,
        line_index: &LineIndex,
    ) -> crate::Result<Option<String>> {
        self.format(source, path, Some((range, line_index)))
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

    use crate::TextDocument;
    use crate::format::{FormatBackend, format, format_range};

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
            Path::new("test.py"),
            FormatBackend::Internal,
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
            Path::new("test.py"),
            FormatBackend::Internal,
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
            Path::new("test.py"),
            FormatBackend::Internal,
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
            Path::new("test.py"),
            FormatBackend::Internal,
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

    #[cfg(feature = "test-uv")]
    mod uv_tests {
        use super::*;

        #[test]
        fn test_uv_format_document() {
            let document = TextDocument::new(
                r#"
def hello(  x,y ,z  ):
    return x+y  +z


def world(  ):
    pass
"#
                .to_string(),
                0,
            );

            let result = format(
                &document,
                PySourceType::Python,
                &FormatterSettings::default(),
                Path::new("test.py"),
                FormatBackend::Uv,
            )
            .expect("Expected no errors when formatting with uv")
            .expect("Expected formatting changes");

            // uv should format this to a consistent style
            assert_snapshot!(result, @r#"
            def hello(x, y, z):
                return x + y + z


            def world():
                pass
            "#);
        }

        #[test]
        fn test_uv_format_range() -> anyhow::Result<()> {
            let document = TextDocument::new(
                r#"
def messy_function(  a,  b,c   ):
    return a+b+c

def another_function(x,y,z):
    result=x+y+z
    return result
"#
                .to_string(),
                0,
            );

            // Find the range of the second function
            let start = document.contents().find("def another_function").unwrap();
            let end = document.contents().find("return result").unwrap() + "return result".len();
            let range = TextRange::new(TextSize::try_from(start)?, TextSize::try_from(end)?);

            let result = format_range(
                &document,
                PySourceType::Python,
                &FormatterSettings::default(),
                range,
                Path::new("test.py"),
                FormatBackend::Uv,
            )
            .expect("Expected no errors when formatting range with uv")
            .expect("Expected formatting changes");

            assert_snapshot!(result.as_code(), @r#"
            def messy_function(  a,  b,c   ):
                return a+b+c

            def another_function(x, y, z):
                result = x + y + z
                return result
            "#);

            Ok(())
        }

        #[test]
        fn test_uv_format_with_line_length() {
            use ruff_formatter::LineWidth;

            let document = TextDocument::new(
                r#"
def hello(very_long_parameter_name_1, very_long_parameter_name_2, very_long_parameter_name_3):
    return very_long_parameter_name_1 + very_long_parameter_name_2 + very_long_parameter_name_3
"#
                .to_string(),
                0,
            );

            // Test with shorter line length
            let formatter_settings = FormatterSettings {
                line_width: LineWidth::try_from(60).unwrap(),
                ..Default::default()
            };

            let result = format(
                &document,
                PySourceType::Python,
                &formatter_settings,
                Path::new("test.py"),
                FormatBackend::Uv,
            )
            .expect("Expected no errors when formatting with uv")
            .expect("Expected formatting changes");

            // With line length 60, the function should be wrapped
            assert_snapshot!(result, @r#"
            def hello(
                very_long_parameter_name_1,
                very_long_parameter_name_2,
                very_long_parameter_name_3,
            ):
                return (
                    very_long_parameter_name_1
                    + very_long_parameter_name_2
                    + very_long_parameter_name_3
                )
            "#);
        }

        #[test]
        fn test_uv_format_with_indent_style() {
            use ruff_formatter::IndentStyle;

            let document = TextDocument::new(
                r#"
def hello():
    if True:
        print("Hello")
        if False:
            print("World")
"#
                .to_string(),
                0,
            );

            // Test with tabs instead of spaces
            let formatter_settings = FormatterSettings {
                indent_style: IndentStyle::Tab,
                ..Default::default()
            };

            let result = format(
                &document,
                PySourceType::Python,
                &formatter_settings,
                Path::new("test.py"),
                FormatBackend::Uv,
            )
            .expect("Expected no errors when formatting with uv")
            .expect("Expected formatting changes");

            // Should have formatting changes (spaces to tabs)
            assert_snapshot!(result, @r#"
            def hello():
            	if True:
            		print("Hello")
            		if False:
            			print("World")
            "#);
        }

        #[test]
        fn test_uv_format_syntax_error() {
            let document = TextDocument::new(
                r#"
def broken(:
    pass
"#
                .to_string(),
                0,
            );

            // uv should return None for syntax errors (as indicated by the TODO comment)
            let result = format(
                &document,
                PySourceType::Python,
                &FormatterSettings::default(),
                Path::new("test.py"),
                FormatBackend::Uv,
            )
            .expect("Expected no errors from format function");

            // Should return None since the syntax is invalid
            assert_eq!(result, None, "Expected None for syntax error");
        }

        #[test]
        fn test_uv_format_with_quote_style() {
            use ruff_python_formatter::QuoteStyle;

            let document = TextDocument::new(
                r#"
x = "hello"
y = 'world'
z = '''multi
line'''
"#
                .to_string(),
                0,
            );

            // Test with single quotes
            let formatter_settings = FormatterSettings {
                quote_style: QuoteStyle::Single,
                ..Default::default()
            };

            let result = format(
                &document,
                PySourceType::Python,
                &formatter_settings,
                Path::new("test.py"),
                FormatBackend::Uv,
            )
            .expect("Expected no errors when formatting with uv")
            .expect("Expected formatting changes");

            assert_snapshot!(result, @r#"
            x = 'hello'
            y = 'world'
            z = """multi
            line"""
            "#);
        }

        #[test]
        fn test_uv_format_with_magic_trailing_comma() {
            use ruff_python_formatter::MagicTrailingComma;

            let document = TextDocument::new(
                r#"
foo = [
    1,
    2,
    3,
]

bar = [1, 2, 3,]
"#
                .to_string(),
                0,
            );

            // Test with ignore magic trailing comma
            let formatter_settings = FormatterSettings {
                magic_trailing_comma: MagicTrailingComma::Ignore,
                ..Default::default()
            };

            let result = format(
                &document,
                PySourceType::Python,
                &formatter_settings,
                Path::new("test.py"),
                FormatBackend::Uv,
            )
            .expect("Expected no errors when formatting with uv")
            .expect("Expected formatting changes");

            assert_snapshot!(result, @r#"
            foo = [1, 2, 3]

            bar = [1, 2, 3]
            "#);
        }
    }
}
