use path_absolutize::path_dedot;
use ruff_cache::cache_dir;
use ruff_linter::settings::types::{FilePattern, FilePatternSet, SerializationFormat};
use ruff_linter::settings::LinterSettings;
use ruff_macros::CacheKey;
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
