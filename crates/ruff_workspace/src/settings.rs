use path_absolutize::path_dedot;
use ruff_cache::cache_dir;
use ruff_formatter::{FormatOptions, IndentStyle, IndentWidth, LineWidth};
use ruff_graph::AnalyzeSettings;
use ruff_linter::display_settings;
use ruff_linter::settings::types::{
    CompiledPerFileTargetVersionList, ExtensionMapping, FilePattern, FilePatternSet, OutputFormat,
    UnsafeFixes,
};
use ruff_linter::settings::LinterSettings;
use ruff_macros::CacheKey;
use ruff_python_ast::{PySourceType, PythonVersion};
use ruff_python_formatter::{
    DocstringCode, DocstringCodeLineWidth, MagicTrailingComma, PreviewMode, PyFormatOptions,
    QuoteStyle,
};
use ruff_source_file::find_newline;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, CacheKey)]
#[allow(clippy::struct_excessive_bools)]
pub struct Settings {
    #[cache_key(ignore)]
    pub cache_dir: PathBuf,
    #[cache_key(ignore)]
    pub fix: bool,
    #[cache_key(ignore)]
    pub fix_only: bool,
    #[cache_key(ignore)]
    pub unsafe_fixes: UnsafeFixes,
    #[cache_key(ignore)]
    pub output_format: OutputFormat,
    #[cache_key(ignore)]
    pub show_fixes: bool,

    pub file_resolver: FileResolverSettings,
    pub linter: LinterSettings,
    pub formatter: FormatterSettings,
    pub analyze: AnalyzeSettings,
}

impl Default for Settings {
    fn default() -> Self {
        let project_root = path_dedot::CWD.as_path();
        Self {
            cache_dir: cache_dir(project_root),
            fix: false,
            fix_only: false,
            output_format: OutputFormat::default(),
            show_fixes: false,
            unsafe_fixes: UnsafeFixes::default(),
            linter: LinterSettings::new(project_root),
            file_resolver: FileResolverSettings::new(project_root),
            formatter: FormatterSettings::default(),
            analyze: AnalyzeSettings::default(),
        }
    }
}

impl fmt::Display for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "\n# General Settings")?;
        display_settings! {
            formatter = f,
            fields = [
                self.cache_dir     | path,
                self.fix,
                self.fix_only,
                self.output_format,
                self.show_fixes,
                self.unsafe_fixes,
                self.file_resolver | nested,
                self.linter        | nested,
                self.formatter     | nested,
                self.analyze       | nested,
            ]
        }
        Ok(())
    }
}

#[derive(Debug, CacheKey)]
pub struct FileResolverSettings {
    pub exclude: FilePatternSet,
    pub extend_exclude: FilePatternSet,
    pub force_exclude: bool,
    pub include: FilePatternSet,
    pub extend_include: FilePatternSet,
    pub respect_gitignore: bool,
    pub project_root: PathBuf,
}

impl fmt::Display for FileResolverSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "\n# File Resolver Settings")?;
        display_settings! {
            formatter = f,
            namespace = "file_resolver",
            fields = [
                self.exclude,
                self.extend_exclude,
                self.force_exclude,
                self.include,
                self.extend_include,
                self.respect_gitignore,
                self.project_root | path,
            ]
        }
        Ok(())
    }
}

pub(crate) static EXCLUDE: &[FilePattern] = &[
    FilePattern::Builtin(".bzr"),
    FilePattern::Builtin(".direnv"),
    FilePattern::Builtin(".eggs"),
    FilePattern::Builtin(".git"),
    FilePattern::Builtin(".git-rewrite"),
    FilePattern::Builtin(".hg"),
    FilePattern::Builtin(".ipynb_checkpoints"),
    FilePattern::Builtin(".mypy_cache"),
    FilePattern::Builtin(".nox"),
    FilePattern::Builtin(".pants.d"),
    FilePattern::Builtin(".pyenv"),
    FilePattern::Builtin(".pytest_cache"),
    FilePattern::Builtin(".pytype"),
    FilePattern::Builtin(".ruff_cache"),
    FilePattern::Builtin(".svn"),
    FilePattern::Builtin(".tox"),
    FilePattern::Builtin(".venv"),
    FilePattern::Builtin(".vscode"),
    FilePattern::Builtin("__pypackages__"),
    FilePattern::Builtin("_build"),
    FilePattern::Builtin("buck-out"),
    FilePattern::Builtin("dist"),
    FilePattern::Builtin("node_modules"),
    FilePattern::Builtin("site-packages"),
    FilePattern::Builtin("venv"),
];

pub(crate) static INCLUDE: &[FilePattern] = &[
    FilePattern::Builtin("*.py"),
    FilePattern::Builtin("*.pyi"),
    FilePattern::Builtin("*.ipynb"),
    FilePattern::Builtin("**/pyproject.toml"),
];

impl FileResolverSettings {
    fn new(project_root: &Path) -> Self {
        Self {
            project_root: project_root.to_path_buf(),
            exclude: FilePatternSet::try_from_iter(EXCLUDE.iter().cloned()).unwrap(),
            extend_exclude: FilePatternSet::default(),
            extend_include: FilePatternSet::default(),
            force_exclude: false,
            respect_gitignore: true,
            include: FilePatternSet::try_from_iter(INCLUDE.iter().cloned()).unwrap(),
        }
    }
}

#[derive(CacheKey, Clone, Debug)]
pub struct FormatterSettings {
    pub exclude: FilePatternSet,
    pub extension: ExtensionMapping,
    pub preview: PreviewMode,
    /// The non-path-resolved Python version specified by the `target-version` input option.
    ///
    /// See [`FormatterSettings::resolve_target_version`] for a way to obtain the Python version for
    /// a given file, while respecting the overrides in `per_file_target_version`.
    pub unresolved_target_version: PythonVersion,
    /// Path-specific overrides to `unresolved_target_version`.
    ///
    /// See [`FormatterSettings::resolve_target_version`] for a way to check a given [`Path`]
    /// against these patterns, while falling back to `unresolved_target_version` if none of them
    /// match.
    pub per_file_target_version: CompiledPerFileTargetVersionList,

    pub line_width: LineWidth,

    pub indent_style: IndentStyle,
    pub indent_width: IndentWidth,

    pub quote_style: QuoteStyle,

    pub magic_trailing_comma: MagicTrailingComma,

    pub line_ending: LineEnding,

    pub docstring_code_format: DocstringCode,
    pub docstring_code_line_width: DocstringCodeLineWidth,
}

impl FormatterSettings {
    pub fn to_format_options(
        &self,
        source_type: PySourceType,
        source: &str,
        path: Option<&Path>,
    ) -> PyFormatOptions {
        let target_version = path
            .map(|path| self.resolve_target_version(path))
            .unwrap_or(self.unresolved_target_version);

        let line_ending = match self.line_ending {
            LineEnding::Lf => ruff_formatter::printer::LineEnding::LineFeed,
            LineEnding::CrLf => ruff_formatter::printer::LineEnding::CarriageReturnLineFeed,
            #[cfg(target_os = "windows")]
            LineEnding::Native => ruff_formatter::printer::LineEnding::CarriageReturnLineFeed,
            #[cfg(not(target_os = "windows"))]
            LineEnding::Native => ruff_formatter::printer::LineEnding::LineFeed,
            LineEnding::Auto => match find_newline(source) {
                Some((_, ruff_source_file::LineEnding::Lf)) => {
                    ruff_formatter::printer::LineEnding::LineFeed
                }
                Some((_, ruff_source_file::LineEnding::CrLf)) => {
                    ruff_formatter::printer::LineEnding::CarriageReturnLineFeed
                }
                Some((_, ruff_source_file::LineEnding::Cr)) => {
                    ruff_formatter::printer::LineEnding::CarriageReturn
                }
                None => ruff_formatter::printer::LineEnding::LineFeed,
            },
        };

        PyFormatOptions::from_source_type(source_type)
            .with_target_version(target_version)
            .with_indent_style(self.indent_style)
            .with_indent_width(self.indent_width)
            .with_quote_style(self.quote_style)
            .with_magic_trailing_comma(self.magic_trailing_comma)
            .with_preview(self.preview)
            .with_line_ending(line_ending)
            .with_line_width(self.line_width)
            .with_docstring_code(self.docstring_code_format)
            .with_docstring_code_line_width(self.docstring_code_line_width)
    }

    /// Resolve the [`PythonVersion`] to use for formatting.
    ///
    /// This method respects the per-file version overrides in
    /// [`FormatterSettings::per_file_target_version`] and falls back on
    /// [`FormatterSettings::unresolved_target_version`] if none of the override patterns match.
    pub fn resolve_target_version(&self, path: &Path) -> PythonVersion {
        self.per_file_target_version
            .is_match(path)
            .unwrap_or(self.unresolved_target_version)
    }
}

impl Default for FormatterSettings {
    fn default() -> Self {
        let default_options = PyFormatOptions::default();

        Self {
            exclude: FilePatternSet::default(),
            extension: ExtensionMapping::default(),
            unresolved_target_version: default_options.target_version(),
            per_file_target_version: CompiledPerFileTargetVersionList::default(),
            preview: PreviewMode::Disabled,
            line_width: default_options.line_width(),
            line_ending: LineEnding::Auto,
            indent_style: default_options.indent_style(),
            indent_width: default_options.indent_width(),
            quote_style: default_options.quote_style(),
            magic_trailing_comma: default_options.magic_trailing_comma(),
            docstring_code_format: default_options.docstring_code(),
            docstring_code_line_width: default_options.docstring_code_line_width(),
        }
    }
}

impl fmt::Display for FormatterSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "\n# Formatter Settings")?;
        display_settings! {
            formatter = f,
            namespace = "formatter",
            fields = [
                self.exclude,
                self.unresolved_target_version,
                self.per_file_target_version,
                self.preview,
                self.line_width,
                self.line_ending,
                self.indent_style,
                self.indent_width,
                self.quote_style,
                self.magic_trailing_comma,
                self.docstring_code_format,
                self.docstring_code_line_width,
            ]
        }
        Ok(())
    }
}

#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Default, CacheKey, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum LineEnding {
    /// The newline style is detected automatically on a file per file basis.
    /// Files with mixed line endings will be converted to the first detected line ending.
    /// Defaults to [`LineEnding::Lf`] for a files that contain no line endings.
    #[default]
    Auto,

    ///  Line endings will be converted to `\n` as is common on Unix.
    Lf,

    /// Line endings will be converted to `\r\n` as is common on Windows.
    CrLf,

    /// Line endings will be converted to `\n` on Unix and `\r\n` on Windows.
    Native,
}

impl fmt::Display for LineEnding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Lf => write!(f, "lf"),
            Self::CrLf => write!(f, "crlf"),
            Self::Native => write!(f, "native"),
        }
    }
}
