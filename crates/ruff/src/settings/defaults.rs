use once_cell::sync::Lazy;
use path_absolutize::path_dedot;
use regex::Regex;
use rustc_hash::FxHashSet;
use std::collections::HashSet;

use super::types::{FilePattern, PreviewMode, PythonVersion};
use super::Settings;
use crate::codes::{self, RuleCodePrefix};
use crate::line_width::{LineLength, TabSize};
use crate::registry::Linter;
use crate::rule_selector::RuleSelector;
use crate::rules::{
    flake8_annotations, flake8_bandit, flake8_bugbear, flake8_builtins, flake8_comprehensions,
    flake8_copyright, flake8_errmsg, flake8_gettext, flake8_implicit_str_concat,
    flake8_import_conventions, flake8_pytest_style, flake8_quotes, flake8_self,
    flake8_tidy_imports, flake8_type_checking, flake8_unused_arguments, isort, mccabe, pep8_naming,
    pycodestyle, pydocstyle, pyflakes, pylint, pyupgrade,
};
use crate::settings::types::FilePatternSet;

pub const PREFIXES: &[RuleSelector] = &[
    RuleSelector::Prefix {
        prefix: RuleCodePrefix::Pycodestyle(codes::Pycodestyle::E),
        redirected_from: None,
    },
    RuleSelector::Linter(Linter::Pyflakes),
];

pub const TASK_TAGS: &[&str] = &["TODO", "FIXME", "XXX"];

pub static DUMMY_VARIABLE_RGX: Lazy<Regex> =
    Lazy::new(|| Regex::new("^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$").unwrap());

pub static EXCLUDE: Lazy<Vec<FilePattern>> = Lazy::new(|| {
    vec![
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
    ]
});

pub static INCLUDE: Lazy<Vec<FilePattern>> = Lazy::new(|| {
    vec![
        FilePattern::Builtin("*.py"),
        FilePattern::Builtin("*.pyi"),
        FilePattern::Builtin("**/pyproject.toml"),
    ]
});

impl Default for Settings {
    fn default() -> Self {
        Self {
            rules: PREFIXES
                .iter()
                .flat_map(|selector| selector.rules(PreviewMode::default()))
                .collect(),
            allowed_confusables: FxHashSet::from_iter([]),
            builtins: vec![],
            dummy_variable_rgx: DUMMY_VARIABLE_RGX.clone(),
            exclude: FilePatternSet::try_from_vec(EXCLUDE.clone()).unwrap(),
            extend_exclude: FilePatternSet::default(),
            extend_include: FilePatternSet::default(),
            external: HashSet::default(),
            force_exclude: false,
            ignore_init_module_imports: false,
            include: FilePatternSet::try_from_vec(INCLUDE.clone()).unwrap(),
            line_length: LineLength::default(),
            logger_objects: vec![],
            namespace_packages: vec![],
            preview: PreviewMode::default(),
            per_file_ignores: vec![],
            project_root: path_dedot::CWD.clone(),
            respect_gitignore: true,
            src: vec![path_dedot::CWD.clone()],
            tab_size: TabSize::default(),
            target_version: PythonVersion::default(),
            task_tags: TASK_TAGS.iter().map(ToString::to_string).collect(),
            typing_modules: vec![],
            flake8_annotations: flake8_annotations::settings::Settings::default(),
            flake8_bandit: flake8_bandit::settings::Settings::default(),
            flake8_bugbear: flake8_bugbear::settings::Settings::default(),
            flake8_builtins: flake8_builtins::settings::Settings::default(),
            flake8_comprehensions: flake8_comprehensions::settings::Settings::default(),
            flake8_copyright: flake8_copyright::settings::Settings::default(),
            flake8_errmsg: flake8_errmsg::settings::Settings::default(),
            flake8_implicit_str_concat: flake8_implicit_str_concat::settings::Settings::default(),
            flake8_import_conventions: flake8_import_conventions::settings::Settings::default(),
            flake8_pytest_style: flake8_pytest_style::settings::Settings::default(),
            flake8_quotes: flake8_quotes::settings::Settings::default(),
            flake8_gettext: flake8_gettext::settings::Settings::default(),
            flake8_self: flake8_self::settings::Settings::default(),
            flake8_tidy_imports: flake8_tidy_imports::settings::Settings::default(),
            flake8_type_checking: flake8_type_checking::settings::Settings::default(),
            flake8_unused_arguments: flake8_unused_arguments::settings::Settings::default(),
            isort: isort::settings::Settings::default(),
            mccabe: mccabe::settings::Settings::default(),
            pep8_naming: pep8_naming::settings::Settings::default(),
            pycodestyle: pycodestyle::settings::Settings::default(),
            pydocstyle: pydocstyle::settings::Settings::default(),
            pyflakes: pyflakes::settings::Settings::default(),
            pylint: pylint::settings::Settings::default(),
            pyupgrade: pyupgrade::settings::Settings::default(),
        }
    }
}
