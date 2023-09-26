use path_absolutize::path_dedot;
use ruff_cache::cache_dir;
use ruff_formatter::{FormatOptions, IndentStyle, LineWidth};
use ruff_linter::settings::types::{FilePattern, FilePatternSet, SerializationFormat};
use ruff_linter::settings::LinterSettings;
use ruff_macros::CacheKey;
use ruff_python_ast::PySourceType;
use ruff_python_formatter::{MagicTrailingComma, PreviewMode, PyFormatOptions, QuoteStyle};
use ruff_source_file::find_newline;
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
    pub output_format: SerializationFormat,
    #[cache_key(ignore)]
    pub show_fixes: bool,
    #[cache_key(ignore)]
    pub show_source: bool,

    pub file_resolver: FileResolverSettings,
    pub linter: LinterSettings,
    pub formatter: FormatterSettings,
}

impl Default for Settings {
    fn default() -> Self {
        let project_root = path_dedot::CWD.as_path();
        Self {
            cache_dir: cache_dir(project_root),
            fix: false,
            fix_only: false,
            output_format: SerializationFormat::default(),
            show_fixes: false,
            show_source: false,
            linter: LinterSettings::new(project_root),
            file_resolver: FileResolverSettings::new(project_root),
            formatter: FormatterSettings::default(),
        }
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
    FilePattern::Builtin("build"),
    FilePattern::Builtin("dist"),
    FilePattern::Builtin("node_modules"),
    FilePattern::Builtin("venv"),
];

pub(crate) static INCLUDE: &[FilePattern] = &[
    FilePattern::Builtin("*.py"),
    FilePattern::Builtin("*.pyi"),
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
    pub preview: PreviewMode,

    pub line_width: LineWidth,

    pub indent_style: IndentStyle,

    pub quote_style: QuoteStyle,

    pub magic_trailing_comma: MagicTrailingComma,

    pub line_ending: LineEnding,
}

impl FormatterSettings {
    pub fn to_format_options(&self, source_type: PySourceType, source: &str) -> PyFormatOptions {
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
            .with_indent_style(self.indent_style)
            .with_quote_style(self.quote_style)
            .with_magic_trailing_comma(self.magic_trailing_comma)
            .with_preview(self.preview)
            .with_line_ending(line_ending)
            .with_line_width(self.line_width)
    }
}

impl Default for FormatterSettings {
    fn default() -> Self {
        let default_options = PyFormatOptions::default();

        Self {
            preview: ruff_python_formatter::PreviewMode::Disabled,
            line_width: default_options.line_width(),
            line_ending: LineEnding::Lf,
            indent_style: default_options.indent_style(),
            quote_style: default_options.quote_style(),
            magic_trailing_comma: default_options.magic_trailing_comma(),
        }
    }
}

#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Default, CacheKey, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum LineEnding {
    ///  Line endings will be converted to `\n` as is common on Unix.
    #[default]
    Lf,

    /// Line endings will be converted to `\r\n` as is common on Windows.
    CrLf,

    /// The newline style is detected automatically on a file per file basis.
    /// Files with mixed line endings will be converted to the first detected line ending.
    /// Defaults to [`LineEnding::Lf`] for a files that contain no line endings.
    Auto,

    /// Line endings will be converted to `\n` on Unix and `\r\n` on Windows.
    Native,
}
