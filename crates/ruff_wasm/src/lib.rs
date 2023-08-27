use std::path::Path;

use js_sys::Error;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use ruff::directives;
use ruff::line_width::{LineLength, TabSize};
use ruff::linter::{check_path, LinterResult};
use ruff::registry::AsRule;
use ruff::settings::types::PythonVersion;
use ruff::settings::{defaults, flags, Settings};
use ruff_python_ast::PySourceType;
use ruff_python_codegen::Stylist;
use ruff_python_formatter::{format_module, format_node, pretty_comments, PyFormatOptions};
use ruff_python_index::{CommentRangesBuilder, Indexer};
use ruff_python_parser::lexer::LexResult;
use ruff_python_parser::AsMode;
use ruff_python_parser::{parse_tokens, Mode};
use ruff_source_file::{Locator, SourceLocation};
use ruff_text_size::Ranged;
use ruff_workspace::configuration::Configuration;
use ruff_workspace::options::Options;

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
        message: string | null;
        edits: {
            content: string | null;
            location: {
                row: number;
                column: number;
            };
            end_location: {
                row: number;
                column: number;
            };
        }[];
    } | null;
};
"#;

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct ExpandedMessage {
    pub code: String,
    pub message: String,
    pub location: SourceLocation,
    pub end_location: SourceLocation,
    pub fix: Option<ExpandedFix>,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct ExpandedFix {
    message: Option<String>,
    edits: Vec<ExpandedEdit>,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
struct ExpandedEdit {
    location: SourceLocation,
    end_location: SourceLocation,
    content: Option<String>,
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
pub struct Workspace {
    settings: Settings,
}

#[wasm_bindgen]
impl Workspace {
    pub fn version() -> String {
        ruff::VERSION.to_string()
    }

    #[wasm_bindgen(constructor)]
    pub fn new(options: JsValue) -> Result<Workspace, Error> {
        let options: Options = serde_wasm_bindgen::from_value(options).map_err(into_error)?;
        let configuration =
            Configuration::from_options(options, Path::new(".")).map_err(into_error)?;
        let settings = configuration
            .into_settings(Path::new("."))
            .map_err(into_error)?;

        Ok(Workspace { settings })
    }

    #[wasm_bindgen(js_name=defaultSettings)]
    pub fn default_settings() -> Result<JsValue, Error> {
        serde_wasm_bindgen::to_value(&Options {
            // Propagate defaults.
            allowed_confusables: Some(Vec::default()),
            builtins: Some(Vec::default()),
            dummy_variable_rgx: Some(defaults::DUMMY_VARIABLE_RGX.as_str().to_string()),
            extend_fixable: Some(Vec::default()),
            extend_ignore: Some(Vec::default()),
            extend_select: Some(Vec::default()),
            extend_unfixable: Some(Vec::default()),
            external: Some(Vec::default()),
            ignore: Some(Vec::default()),
            line_length: Some(LineLength::default()),
            select: Some(defaults::PREFIXES.to_vec()),
            tab_size: Some(TabSize::default()),
            target_version: Some(PythonVersion::default()),
            // Ignore a bunch of options that don't make sense in a single-file editor.
            cache_dir: None,
            exclude: None,
            extend: None,
            extend_exclude: None,
            extend_include: None,
            extend_per_file_ignores: None,
            fix: None,
            fix_only: None,
            fixable: None,
            force_exclude: None,
            format: None,
            ignore_init_module_imports: None,
            include: None,
            logger_objects: None,
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
            ..Options::default()
        })
        .map_err(into_error)
    }

    pub fn check(&self, contents: &str) -> Result<JsValue, Error> {
        let source_type = PySourceType::default();

        // Tokenize once.
        let tokens: Vec<LexResult> = ruff_python_parser::tokenize(contents, source_type.as_mode());

        // Map row and column locations to byte slices (lazily).
        let locator = Locator::new(contents);

        // Detect the current code style (lazily).
        let stylist = Stylist::from_tokens(&tokens, &locator);

        // Extra indices from the code.
        let indexer = Indexer::from_tokens(&tokens, &locator);

        // Extract the `# noqa` and `# isort: skip` directives from the source.
        let directives =
            directives::extract_directives(&tokens, directives::Flags::empty(), &locator, &indexer);

        // Generate checks.
        let LinterResult {
            data: (diagnostics, _imports),
            ..
        } = check_path(
            Path::new("<filename>"),
            None,
            tokens,
            &locator,
            &stylist,
            &indexer,
            &directives,
            &self.settings,
            flags::Noqa::Enabled,
            None,
            source_type,
        );

        let source_code = locator.to_source_code();

        let messages: Vec<ExpandedMessage> = diagnostics
            .into_iter()
            .map(|message| {
                let start_location = source_code.source_location(message.start());
                let end_location = source_code.source_location(message.end());

                ExpandedMessage {
                    code: message.kind.rule().noqa_code().to_string(),
                    message: message.kind.body,
                    location: start_location,
                    end_location,
                    fix: message.fix.map(|fix| ExpandedFix {
                        message: message.kind.suggestion,
                        edits: fix
                            .edits()
                            .iter()
                            .map(|edit| ExpandedEdit {
                                location: source_code.source_location(edit.start()),
                                end_location: source_code.source_location(edit.end()),
                                content: edit.content().map(ToString::to_string),
                            })
                            .collect(),
                    }),
                }
            })
            .collect();

        serde_wasm_bindgen::to_value(&messages).map_err(into_error)
    }

    pub fn format(&self, contents: &str) -> Result<String, Error> {
        // TODO(konstin): Add an options for py/pyi to the UI (1/2)
        let options = PyFormatOptions::from_source_type(PySourceType::default());
        let printed = format_module(contents, options).map_err(into_error)?;

        Ok(printed.into_code())
    }

    pub fn format_ir(&self, contents: &str) -> Result<String, Error> {
        let tokens: Vec<_> = ruff_python_parser::lexer::lex(contents, Mode::Module).collect();
        let mut comment_ranges = CommentRangesBuilder::default();

        for (token, range) in tokens.iter().flatten() {
            comment_ranges.visit_token(token, *range);
        }

        let comment_ranges = comment_ranges.finish();
        let module = parse_tokens(tokens, Mode::Module, ".").map_err(into_error)?;

        // TODO(konstin): Add an options for py/pyi to the UI (2/2)
        let options = PyFormatOptions::from_source_type(PySourceType::default());
        let formatted =
            format_node(&module, &comment_ranges, contents, options).map_err(into_error)?;

        Ok(format!("{formatted}"))
    }

    pub fn comments(&self, contents: &str) -> Result<String, Error> {
        let tokens: Vec<_> = ruff_python_parser::lexer::lex(contents, Mode::Module).collect();
        let mut comment_ranges = CommentRangesBuilder::default();

        for (token, range) in tokens.iter().flatten() {
            comment_ranges.visit_token(token, *range);
        }
        let comment_ranges = comment_ranges.finish();
        let module = parse_tokens(tokens, Mode::Module, ".").map_err(into_error)?;

        // TODO(konstin): Add an options for py/pyi to the UI (2/2)
        let options = PyFormatOptions::from_source_type(PySourceType::default());
        let formatted =
            format_node(&module, &comment_ranges, contents, options).map_err(into_error)?;
        let comments = pretty_comments(&formatted, contents);

        Ok(comments)
    }

    /// Parses the content and returns its AST
    pub fn parse(&self, contents: &str) -> Result<String, Error> {
        let parsed = ruff_python_parser::parse(contents, Mode::Module, ".").map_err(into_error)?;

        Ok(format!("{parsed:#?}"))
    }

    pub fn tokens(&self, contents: &str) -> Result<String, Error> {
        let tokens: Vec<_> = ruff_python_parser::lexer::lex(contents, Mode::Module).collect();

        Ok(format!("{tokens:#?}"))
    }
}

pub(crate) fn into_error<E: std::fmt::Display>(err: E) -> Error {
    Error::new(&err.to_string())
}
