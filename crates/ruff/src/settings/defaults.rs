use once_cell::sync::Lazy;
use path_absolutize::path_dedot;
use regex::Regex;
use rustc_hash::FxHashSet;

use super::hashable::{HashableGlobSet, HashableHashSet};
use super::types::{FilePattern, PythonVersion};
use super::Settings;
use crate::registry::RuleCodePrefix;
use crate::rule_selector::{prefix_to_selector, RuleSelector};
use crate::rules::{
    flake8_annotations, flake8_bandit, flake8_bugbear, flake8_builtins, flake8_errmsg,
    flake8_implicit_str_concat, flake8_import_conventions, flake8_pytest_style, flake8_quotes,
    flake8_tidy_imports, flake8_type_checking, flake8_unused_arguments, isort, mccabe, pep8_naming,
    pycodestyle, pydocstyle, pylint, pyupgrade,
};

pub const PREFIXES: &[RuleSelector] = &[
    prefix_to_selector(RuleCodePrefix::E),
    prefix_to_selector(RuleCodePrefix::F),
];

pub const TARGET_VERSION: PythonVersion = PythonVersion::Py310;

pub const LINE_LENGTH: usize = 88;

pub const TASK_TAGS: &[&str] = &["TODO", "FIXME", "XXX"];

pub static DUMMY_VARIABLE_RGX: Lazy<Regex> =
    Lazy::new(|| Regex::new("^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$").unwrap());

pub static EXCLUDE: Lazy<Vec<FilePattern>> = Lazy::new(|| {
    vec![
        FilePattern::Builtin(".bzr"),
        FilePattern::Builtin(".direnv"),
        FilePattern::Builtin(".eggs"),
        FilePattern::Builtin(".git"),
        FilePattern::Builtin(".hg"),
        FilePattern::Builtin(".mypy_cache"),
        FilePattern::Builtin(".nox"),
        FilePattern::Builtin(".pants.d"),
        FilePattern::Builtin(".ruff_cache"),
        FilePattern::Builtin(".svn"),
        FilePattern::Builtin(".tox"),
        FilePattern::Builtin(".venv"),
        FilePattern::Builtin("__pypackages__"),
        FilePattern::Builtin("_build"),
        FilePattern::Builtin("buck-out"),
        FilePattern::Builtin("build"),
        FilePattern::Builtin("dist"),
        FilePattern::Builtin("node_modules"),
        FilePattern::Builtin("venv"),
    ]
});

impl Default for Settings {
    fn default() -> Self {
        Self {
            rules: PREFIXES.iter().flat_map(IntoIterator::into_iter).into(),
            allowed_confusables: FxHashSet::from_iter([]).into(),
            builtins: vec![],
            dummy_variable_rgx: DUMMY_VARIABLE_RGX.clone().into(),
            exclude: HashableGlobSet::new(EXCLUDE.clone()).unwrap(),
            extend_exclude: HashableGlobSet::empty(),
            external: HashableHashSet::default(),
            force_exclude: false,
            ignore_init_module_imports: false,
            line_length: LINE_LENGTH,
            namespace_packages: vec![],
            per_file_ignores: vec![],
            respect_gitignore: true,
            show_source: false,
            src: vec![path_dedot::CWD.clone()],
            project_root: path_dedot::CWD.clone(),
            target_version: TARGET_VERSION,
            task_tags: TASK_TAGS.iter().map(ToString::to_string).collect(),
            typing_modules: vec![],
            flake8_annotations: flake8_annotations::settings::Settings::default(),
            flake8_bandit: flake8_bandit::settings::Settings::default(),
            flake8_bugbear: flake8_bugbear::settings::Settings::default(),
            flake8_builtins: flake8_builtins::settings::Settings::default(),
            flake8_errmsg: flake8_errmsg::settings::Settings::default(),
            flake8_implicit_str_concat: flake8_implicit_str_concat::settings::Settings::default(),
            flake8_import_conventions: flake8_import_conventions::settings::Settings::default(),
            flake8_pytest_style: flake8_pytest_style::settings::Settings::default(),
            flake8_quotes: flake8_quotes::settings::Settings::default(),
            flake8_tidy_imports: flake8_tidy_imports::Settings::default(),
            flake8_type_checking: flake8_type_checking::settings::Settings::default(),
            flake8_unused_arguments: flake8_unused_arguments::settings::Settings::default(),
            isort: isort::settings::Settings::default(),
            mccabe: mccabe::settings::Settings::default(),
            pep8_naming: pep8_naming::settings::Settings::default(),
            pycodestyle: pycodestyle::settings::Settings::default(),
            pydocstyle: pydocstyle::settings::Settings::default(),
            pylint: pylint::settings::Settings::default(),
            pyupgrade: pyupgrade::settings::Settings::default(),
        }
    }
}
