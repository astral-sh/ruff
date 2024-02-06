use std::path::Path;

use js_sys::Error;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use ruff_formatter::printer::SourceMapGeneration;
use ruff_formatter::{FormatResult, Formatted, IndentStyle};
use ruff_linter::directives;
use ruff_linter::line_width::{IndentWidth, LineLength};
use ruff_linter::linter::{check_path, LinterResult, TokenSource};
use ruff_linter::registry::AsRule;
use ruff_linter::settings::types::PythonVersion;
use ruff_linter::settings::{flags, DEFAULT_SELECTORS, DUMMY_VARIABLE_RGX};
use ruff_linter::source_kind::SourceKind;
use ruff_python_ast::{Mod, PySourceType};
use ruff_python_codegen::Stylist;
use ruff_python_formatter::{format_module_ast, pretty_comments, PyFormatContext, QuoteStyle};
use ruff_python_index::{CommentRangesBuilder, Indexer};
use ruff_python_parser::lexer::LexResult;
use ruff_python_parser::{parse_tokens, tokenize_all, AsMode, Mode};
use ruff_python_trivia::CommentRanges;
use ruff_source_file::{Locator, SourceLocation};
use ruff_text_size::Ranged;
use ruff_workspace::configuration::Configuration;
use ruff_workspace::options::{FormatOptions, LintCommonOptions, LintOptions, Options};
use ruff_workspace::Settings;

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
        ruff_linter::VERSION.to_string()
    }

    #[wasm_bindgen(constructor)]
    pub fn new(options: JsValue) -> Result<Workspace, Error> {
        let options: Options = serde_wasm_bindgen::from_value(options).map_err(into_error)?;
        let configuration =
            Configuration::from_options(options, Some(Path::new(".")), Path::new("."))
                .map_err(into_error)?;
        let settings = configuration
            .into_settings(Path::new("."))
            .map_err(into_error)?;

        Ok(Workspace { settings })
    }

    #[wasm_bindgen(js_name = defaultSettings)]
    pub fn default_settings() -> Result<JsValue, Error> {
        serde_wasm_bindgen::to_value(&Options {
            preview: Some(false),

            // Propagate defaults.
            builtins: Some(Vec::default()),

            line_length: Some(LineLength::default()),

            indent_width: Some(IndentWidth::default()),
            target_version: Some(PythonVersion::default()),

            lint: Some(LintOptions {
                common: LintCommonOptions {
                    allowed_confusables: Some(Vec::default()),
                    dummy_variable_rgx: Some(DUMMY_VARIABLE_RGX.as_str().to_string()),
                    ignore: Some(Vec::default()),
                    select: Some(DEFAULT_SELECTORS.to_vec()),
                    extend_fixable: Some(Vec::default()),
                    extend_select: Some(Vec::default()),
                    external: Some(Vec::default()),
                    ..LintCommonOptions::default()
                },

                ..LintOptions::default()
            }),
            format: Some(FormatOptions {
                indent_style: Some(IndentStyle::Space),
                quote_style: Some(QuoteStyle::Double),
                ..FormatOptions::default()
            }),
            ..Options::default()
        })
        .map_err(into_error)
    }

    pub fn check(&self, contents: &str) -> Result<JsValue, Error> {
        let source_type = PySourceType::default();

        // TODO(dhruvmanila): Support Jupyter Notebooks
        let source_kind = SourceKind::Python(contents.to_string());

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
            &locator,
            &stylist,
            &indexer,
            &directives,
            &self.settings.linter,
            flags::Noqa::Enabled,
            &source_kind,
            source_type,
            TokenSource::Tokens(tokens),
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
        let parsed = ParsedModule::from_source(contents)?;
        let formatted = parsed.format(&self.settings).map_err(into_error)?;
        let printed = formatted.print().map_err(into_error)?;

        Ok(printed.into_code())
    }

    pub fn format_ir(&self, contents: &str) -> Result<String, Error> {
        let parsed = ParsedModule::from_source(contents)?;
        let formatted = parsed.format(&self.settings).map_err(into_error)?;

        Ok(format!("{formatted}"))
    }

    pub fn comments(&self, contents: &str) -> Result<String, Error> {
        let parsed = ParsedModule::from_source(contents)?;
        let comments = pretty_comments(&parsed.module, &parsed.comment_ranges, contents);
        Ok(comments)
    }

    /// Parses the content and returns its AST
    pub fn parse(&self, contents: &str) -> Result<String, Error> {
        let parsed = ruff_python_parser::parse(contents, Mode::Module).map_err(into_error)?;

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

struct ParsedModule<'a> {
    source_code: &'a str,
    module: Mod,
    comment_ranges: CommentRanges,
}

impl<'a> ParsedModule<'a> {
    fn from_source(source_code: &'a str) -> Result<Self, Error> {
        let tokens: Vec<_> = tokenize_all(source_code, Mode::Module);
        let mut comment_ranges = CommentRangesBuilder::default();

        for (token, range) in tokens.iter().flatten() {
            comment_ranges.visit_token(token, *range);
        }
        let comment_ranges = comment_ranges.finish();
        let module = parse_tokens(tokens, source_code, Mode::Module).map_err(into_error)?;

        Ok(Self {
            source_code,
            module,
            comment_ranges,
        })
    }

    fn format(&self, settings: &Settings) -> FormatResult<Formatted<PyFormatContext>> {
        // TODO(konstin): Add an options for py/pyi to the UI (2/2)
        let options = settings
            .formatter
            .to_format_options(PySourceType::default(), self.source_code)
            .with_source_map_generation(SourceMapGeneration::Enabled);

        format_module_ast(
            &self.module,
            &self.comment_ranges,
            self.source_code,
            options,
        )
    }
}
