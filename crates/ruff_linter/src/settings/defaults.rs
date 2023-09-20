use once_cell::sync::Lazy;
use path_absolutize::path_dedot;
use regex::Regex;
use ruff_cache::cache_dir;
use rustc_hash::FxHashSet;
use std::collections::HashSet;

use super::types::{PreviewMode, PythonVersion};
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
use crate::settings::types::SerializationFormat;
use crate::settings::FileResolverSettings;

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

impl Default for Settings {
    fn default() -> Self {
        let project_root = path_dedot::CWD.clone();
        Self {
            cache_dir: cache_dir(&project_root),
            fix: false,
            fix_only: false,
            output_format: SerializationFormat::default(),
            show_fixes: false,
            show_source: false,

            file_resolver: FileResolverSettings::with_project_root(project_root),

            rules: PREFIXES
                .iter()
                .flat_map(|selector| selector.rules(PreviewMode::default()))
                .collect(),
            allowed_confusables: FxHashSet::from_iter([]),
            builtins: vec![],
            dummy_variable_rgx: DUMMY_VARIABLE_RGX.clone(),
            external: HashSet::default(),
            ignore_init_module_imports: false,
            line_length: LineLength::default(),
            logger_objects: vec![],
            namespace_packages: vec![],
            preview: PreviewMode::default(),
            per_file_ignores: vec![],

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
