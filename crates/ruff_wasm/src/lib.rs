use std::path::Path;

use rustpython_parser::ast::Location;
use rustpython_parser::lexer::LexResult;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use ruff::directives;
use ruff::linter::{check_path, LinterResult};
use ruff::rules::{
    flake8_annotations, flake8_bandit, flake8_bugbear, flake8_builtins, flake8_comprehensions,
    flake8_errmsg, flake8_implicit_str_concat, flake8_import_conventions, flake8_pytest_style,
    flake8_quotes, flake8_self, flake8_tidy_imports, flake8_type_checking, flake8_unused_arguments,
    isort, mccabe, pep8_naming, pycodestyle, pydocstyle, pylint, pyupgrade,
};
use ruff::settings::configuration::Configuration;
use ruff::settings::options::Options;
use ruff::settings::{defaults, flags, Settings};
use ruff_python_ast::source_code::{Indexer, Locator, Stylist};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &'static str = r#"
export interface Diagnostic {
    code: string;
    message: string;
    location: {
        row: number;
        column: number;
    };
    end_location: {
        row: number;
        column: number;
    };
    fix: {
        content: string;
        message: string | null;
        location: {
            row: number;
            column: number;
        };
        end_location: {
            row: number;
            column: number;
        };
    } | null;
};
"#;

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct ExpandedMessage {
    pub code: String,
    pub message: String,
    pub location: Location,
    pub end_location: Location,
    pub fix: Option<ExpandedFix>,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct ExpandedFix {
    content: String,
    message: Option<String>,
    location: Location,
    end_location: Location,
}

#[wasm_bindgen(start)]
pub fn run() {
    use log::Level;

    // When the `console_error_panic_hook` feature is enabled, we can call the
    // `set_panic_hook` function at least once during initialization, and then
    // we will get better error messages if our code ever panics.
    //
    // For more details see
    // https://github.com/rustwasm/console_error_panic_hook#readme
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();

    console_log::init_with_level(Level::Debug).expect("Initializing logger went wrong.");
}

#[wasm_bindgen]
#[allow(non_snake_case)]
pub fn currentVersion() -> JsValue {
    JsValue::from(VERSION)
}

#[wasm_bindgen]
#[allow(non_snake_case)]
pub fn defaultSettings() -> Result<JsValue, JsValue> {
    Ok(serde_wasm_bindgen::to_value(&Options {
        // Propagate defaults.
        allowed_confusables: Some(Vec::default()),
        builtins: Some(Vec::default()),
        dummy_variable_rgx: Some(defaults::DUMMY_VARIABLE_RGX.as_str().to_string()),
        extend_ignore: Some(Vec::default()),
        extend_select: Some(Vec::default()),
        external: Some(Vec::default()),
        ignore: Some(Vec::default()),
        line_length: Some(defaults::LINE_LENGTH),
        select: Some(defaults::PREFIXES.to_vec()),
        target_version: Some(defaults::TARGET_VERSION),
        // Ignore a bunch of options that don't make sense in a single-file editor.
        cache_dir: None,
        exclude: None,
        extend: None,
        extend_exclude: None,
        fix: None,
        fix_only: None,
        fixable: None,
        force_exclude: None,
        format: None,
        ignore_init_module_imports: None,
        namespace_packages: None,
        per_file_ignores: None,
        required_version: None,
        respect_gitignore: None,
        show_fixes: None,
        show_source: None,
        src: None,
        task_tags: None,
        typing_modules: None,
        unfixable: None,
        update_check: None,
        // Use default options for all plugins.
        flake8_annotations: Some(flake8_annotations::settings::Settings::default().into()),
        flake8_bandit: Some(flake8_bandit::settings::Settings::default().into()),
        flake8_bugbear: Some(flake8_bugbear::settings::Settings::default().into()),
        flake8_builtins: Some(flake8_builtins::settings::Settings::default().into()),
        flake8_comprehensions: Some(flake8_comprehensions::settings::Settings::default().into()),
        flake8_errmsg: Some(flake8_errmsg::settings::Settings::default().into()),
        flake8_pytest_style: Some(flake8_pytest_style::settings::Settings::default().into()),
        flake8_quotes: Some(flake8_quotes::settings::Settings::default().into()),
        flake8_self: Some(flake8_self::settings::Settings::default().into()),
        flake8_implicit_str_concat: Some(
            flake8_implicit_str_concat::settings::Settings::default().into(),
        ),
        flake8_import_conventions: Some(
            flake8_import_conventions::settings::Settings::default().into(),
        ),
        flake8_tidy_imports: Some(flake8_tidy_imports::Settings::default().into()),
        flake8_type_checking: Some(flake8_type_checking::settings::Settings::default().into()),
        flake8_unused_arguments: Some(
            flake8_unused_arguments::settings::Settings::default().into(),
        ),
        isort: Some(isort::settings::Settings::default().into()),
        mccabe: Some(mccabe::settings::Settings::default().into()),
        pep8_naming: Some(pep8_naming::settings::Settings::default().into()),
        pycodestyle: Some(pycodestyle::settings::Settings::default().into()),
        pydocstyle: Some(pydocstyle::settings::Settings::default().into()),
        pylint: Some(pylint::settings::Settings::default().into()),
        pyupgrade: Some(pyupgrade::settings::Settings::default().into()),
    })?)
}

#[wasm_bindgen]
#[allow(non_snake_case)]
pub fn check(contents: &str, options: JsValue) -> Result<JsValue, JsValue> {
    let options: Options = serde_wasm_bindgen::from_value(options).map_err(|e| e.to_string())?;
    let configuration =
        Configuration::from_options(options, Path::new(".")).map_err(|e| e.to_string())?;
    let settings =
        Settings::from_configuration(configuration, Path::new(".")).map_err(|e| e.to_string())?;

    // Tokenize once.
    let tokens: Vec<LexResult> = ruff_rustpython::tokenize(contents);

    // Map row and column locations to byte slices (lazily).
    let locator = Locator::new(contents);

    // Detect the current code style (lazily).
    let stylist = Stylist::from_contents(contents, &locator);

    // Extra indices from the code.
    let indexer: Indexer = tokens.as_slice().into();

    // Extract the `# noqa` and `# isort: skip` directives from the source.
    let directives = directives::extract_directives(&tokens, directives::Flags::empty());

    // Generate checks.
    let LinterResult {
        data: diagnostics, ..
    } = check_path(
        Path::new("<filename>"),
        None,
        contents,
        tokens,
        &locator,
        &stylist,
        &indexer,
        &directives,
        &settings,
        flags::Noqa::Enabled,
        flags::Autofix::Enabled,
    );

    let messages: Vec<ExpandedMessage> = diagnostics
        .into_iter()
        .map(|message| ExpandedMessage {
            code: message.kind.name,
            message: message.kind.body,
            location: message.location,
            end_location: message.end_location,
            fix: message.fix.map(|fix| ExpandedFix {
                content: fix.content,
                message: message.kind.suggestion,
                location: fix.location,
                end_location: fix.end_location,
            }),
        })
        .collect();

    Ok(serde_wasm_bindgen::to_value(&messages)?)
}
