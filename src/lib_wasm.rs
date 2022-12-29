use std::path::Path;

use rustpython_ast::Location;
use rustpython_parser::lexer::LexResult;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::autofix::Fix;
use crate::checks::CheckCode;
use crate::directives;
use crate::linter::check_path;
use crate::rustpython_helpers::tokenize;
use crate::settings::configuration::Configuration;
use crate::settings::options::Options;
use crate::settings::{flags, Settings};
use crate::source_code_locator::SourceCodeLocator;
use crate::source_code_style::SourceCodeStyleDetector;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &'static str = r#"
export interface Check {
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

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Message {
    code: CheckCode,
    message: String,
    location: Location,
    end_location: Location,
    fix: Option<Fix>,
}

#[wasm_bindgen(start)]
pub fn run() {
    use log::Level;
    console_error_panic_hook::set_once();
    console_log::init_with_level(Level::Debug).expect("Initializing logger went wrong.");
}

#[wasm_bindgen]
pub fn current_version() -> JsValue {
    JsValue::from(VERSION)
}

#[wasm_bindgen]
pub fn check(contents: &str, options: JsValue) -> Result<JsValue, JsValue> {
    let options: Options = serde_wasm_bindgen::from_value(options).map_err(|e| e.to_string())?;
    let configuration =
        Configuration::from_options(options, Path::new(".")).map_err(|e| e.to_string())?;
    let settings =
        Settings::from_configuration(configuration, Path::new(".")).map_err(|e| e.to_string())?;

    // Tokenize once.
    let tokens: Vec<LexResult> = tokenize(contents);

    // Map row and column locations to byte slices (lazily).
    let locator = SourceCodeLocator::new(contents);

    // Detect the current code style (lazily).
    let stylist = SourceCodeStyleDetector::from_contents(contents, &locator);

    // Extract the `# noqa` and `# isort: skip` directives from the source.
    let directives = directives::extract_directives(&tokens, &locator, directives::Flags::empty());

    // Generate checks.
    let checks = check_path(
        Path::new("<filename>"),
        None,
        contents,
        tokens,
        &locator,
        &stylist,
        &directives,
        &settings,
        flags::Autofix::Enabled,
        flags::Noqa::Enabled,
    )
    .map_err(|e| e.to_string())?;

    let messages: Vec<Message> = checks
        .into_iter()
        .map(|check| Message {
            code: check.kind.code().clone(),
            message: check.kind.body(),
            location: check.location,
            end_location: check.end_location,
            fix: check.fix,
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
            [Message {
                code: CheckCode::F634,
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
