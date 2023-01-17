use path_absolutize::path_dedot;
use regex::Regex;
use rustc_hash::FxHashSet;

use super::hashable::{HashableGlobSet, HashableHashSet};
use super::types::PythonVersion;
use super::Settings;
use crate::registry::RuleCodePrefix;
use crate::rules::{
    flake8_annotations, flake8_bandit, flake8_bugbear, flake8_errmsg, flake8_import_conventions,
    flake8_pytest_style, flake8_quotes, flake8_tidy_imports, flake8_unused_arguments, isort,
    mccabe, pep8_naming, pycodestyle, pydocstyle, pyupgrade,
};

impl Default for Settings {
    fn default() -> Self {
        Self {
            rules: [&RuleCodePrefix::E, &RuleCodePrefix::F]
                .into_iter()
                .flat_map(RuleCodePrefix::codes)
                .into(),
            allowed_confusables: FxHashSet::from_iter([]).into(),
            builtins: vec![],
            dummy_variable_rgx: Regex::new("^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$")
                .unwrap()
                .into(),
            exclude: HashableGlobSet::empty(),
            extend_exclude: HashableGlobSet::empty(),
            external: HashableHashSet::default(),
            force_exclude: false,
            ignore_init_module_imports: false,
            line_length: 88,
            namespace_packages: vec![],
            per_file_ignores: vec![],
            required_version: None,
            respect_gitignore: true,
            show_source: false,
            src: vec![path_dedot::CWD.clone()],
            target_version: PythonVersion::Py310,
            task_tags: vec!["TODO".to_string(), "FIXME".to_string(), "XXX".to_string()],
            typing_modules: vec![],
            flake8_annotations: flake8_annotations::settings::Settings::default(),
            flake8_bandit: flake8_bandit::settings::Settings::default(),
            flake8_bugbear: flake8_bugbear::settings::Settings::default(),
            flake8_errmsg: flake8_errmsg::settings::Settings::default(),
            flake8_import_conventions: flake8_import_conventions::settings::Settings::default(),
            flake8_pytest_style: flake8_pytest_style::settings::Settings::default(),
            flake8_quotes: flake8_quotes::settings::Settings::default(),
            flake8_tidy_imports: flake8_tidy_imports::Settings::default(),
            flake8_unused_arguments: flake8_unused_arguments::settings::Settings::default(),
            isort: isort::settings::Settings::default(),
            mccabe: mccabe::settings::Settings::default(),
            pep8_naming: pep8_naming::settings::Settings::default(),
            pycodestyle: pycodestyle::settings::Settings::default(),
            pydocstyle: pydocstyle::settings::Settings::default(),
            pyupgrade: pyupgrade::settings::Settings::default(),
        }
    }
}
