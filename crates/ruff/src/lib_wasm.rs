use std::path::Path;

use rustpython_parser::ast::Location;
use rustpython_parser::lexer::LexResult;
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::directives;
use crate::linter::{check_path, LinterResult};
use crate::registry::Rule;
use crate::rules::{
    flake8_annotations, flake8_bandit, flake8_bugbear, flake8_builtins, flake8_errmsg,
    flake8_implicit_str_concat, flake8_import_conventions, flake8_pytest_style, flake8_quotes,
    flake8_tidy_imports, flake8_type_checking, flake8_unused_arguments, isort, mccabe, pep8_naming,
    pycodestyle, pydocstyle, pylint, pyupgrade,
};
use crate::rustpython_helpers::tokenize;
use crate::settings::configuration::Configuration;
use crate::settings::options::Options;
use crate::settings::{defaults, flags, Settings};
use crate::source_code::{Indexer, Locator, Stylist};

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

#[derive(Serialize)]
struct ExpandedMessage {
    code: SerializeRuleAsCode,
    message: String,
    location: Location,
    end_location: Location,
    fix: Option<ExpandedFix>,
}

struct SerializeRuleAsCode(Rule);

impl Serialize for SerializeRuleAsCode {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.0.code())
    }
}

impl From<Rule> for SerializeRuleAsCode {
    fn from(rule: Rule) -> Self {
        Self(rule)
    }
}

#[derive(Serialize)]
struct ExpandedFix {
    content: String,
    message: Option<String>,
    location: Location,
    end_location: Location,
}

#[wasm_bindgen(start)]
pub fn run() {
    use log::Level;
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
        flake8_errmsg: Some(flake8_errmsg::settings::Settings::default().into()),
        flake8_pytest_style: Some(flake8_pytest_style::settings::Settings::default().into()),
        flake8_quotes: Some(flake8_quotes::settings::Settings::default().into()),
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
    let tokens: Vec<LexResult> = tokenize(contents);

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
        flags::Autofix::Enabled,
        flags::Noqa::Enabled,
    );

    let messages: Vec<ExpandedMessage> = diagnostics
        .into_iter()
        .map(|diagnostic| ExpandedMessage {
            code: diagnostic.kind.rule().clone().into(),
            message: diagnostic.kind.body(),
            location: diagnostic.location,
            end_location: diagnostic.end_location,
            fix: diagnostic.fix.map(|fix| ExpandedFix {
                content: fix.content,
                message: diagnostic.kind.commit(),
                location: fix.location,
                end_location: fix.end_location,
            }),
        })
        .collect();

    Ok(serde_wasm_bindgen::to_value(&messages)?)
}

#[cfg(test)]
mod test {
    use js_sys;
    use wasm_bindgen_test::*;

    use super::*;

    macro_rules! check {
        ($source:expr, $config:expr, $expected:expr) => {{
            let foo = js_sys::JSON::parse($config).unwrap();
            match check($source, foo) {
                Ok(output) => {
                    let result: Vec<Message> = serde_wasm_bindgen::from_value(output).unwrap();
                    assert_eq!(result, $expected);
                }
                Err(e) => assert!(false, "{:#?}", e),
            }
        }};
    }

    #[wasm_bindgen_test]
    fn empty_config() {
        check!(
            "if (1, 2): pass",
            r#"{}"#,
            [ExpandedMessage {
                code: Rule::IfTuple.into(),
                message: "If test is a tuple, which is always `True`".to_string(),
                location: Location::new(1, 0),
                end_location: Location::new(1, 15),
                fix: None,
            }]
        );
    }

    #[wasm_bindgen_test]
    fn partial_config() {
        check!("if (1, 2): pass", r#"{"ignore": ["F"]}"#, []);
    }

    #[wasm_bindgen_test]
    fn partial_nested_config() {
        let config = r#"{
          "select": ["Q"],
          "flake8-quotes": {
            "inline-quotes": "single"
          }
        }"#;
        check!(r#"print('hello world')"#, config, []);
    }
}
