use std::iter::repeat;

use ruff_db::diagnostic::LintName;
use ruff_text_size::{TextLen, TextRange, TextSize};
use rustc_hash::FxHashMap;
use serde::Deserialize;
use toml::Spanned;

use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::TomlSourceType;

use crate::{
    AlwaysFixableViolation, checkers::ast::LintContext, codes::Rule,
    preview::is_human_readable_names_enabled, rule_redirects::get_redirect_target,
};

/// ## What it does
///
/// Checks for any configuration files that use rule codes as selectors.
///
/// ## Why is this bad?
///
/// Human-readable rule names are easier to understand than rule codes. Using names also avoids
/// requiring readers to look up the meaning of each code.
///
/// ## Example
///
/// ```toml
/// [tool.ruff.lint]
/// select = ["F401"]
/// ```
///
/// Use instead:
///
/// ```toml
/// [tool.ruff.lint]
/// select = ["unused-import"]
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct RuleCodesInSelectors {
    selector: &'static str,
}

impl AlwaysFixableViolation for RuleCodesInSelectors {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { selector } = self;
        format!("Rule code used instead of name in `{selector}`")
    }

    fn fix_title(&self) -> String {
        "Replace rule code with name".to_string()
    }
}

/// RUF201
pub(crate) fn rule_codes_in_selectors(context: &LintContext, source_type: TomlSourceType) {
    if !is_human_readable_names_enabled(context.settings().preview) {
        return;
    }

    let source = context.source_file().source_text();
    let Ok(Some(config_file)) = ConfigFile::from_toml_str(source, source_type) else {
        return;
    };

    for (spanned, selector) in config_file.selectors() {
        let Some(RuleCode { name, range }) = RuleCode::from_spanned(spanned, source) else {
            continue;
        };

        context
            .report_diagnostic(RuleCodesInSelectors { selector }, range)
            .set_fix(Fix::safe_edit(Edit::range_replacement(
                name.to_string(),
                range,
            )));
    }
}

enum ConfigFile {
    Pyproject(Pyproject),
    Ruff(Ruff),
}

impl ConfigFile {
    fn from_toml_str(
        source: &str,
        source_type: TomlSourceType,
    ) -> Result<Option<Self>, toml::de::Error> {
        match source_type {
            TomlSourceType::Pyproject => Ok(Some(Self::Pyproject(toml::from_str(source)?))),
            TomlSourceType::Ruff => Ok(Some(Self::Ruff(toml::from_str(source)?))),
            _ => Ok(None),
        }
    }

    fn selectors(&self) -> impl Iterator<Item = (&Spanned<String>, &'static str)> {
        let ruff = match self {
            ConfigFile::Pyproject(pyproject) => &pyproject.tool.ruff,
            ConfigFile::Ruff(ruff) => ruff,
        };
        ruff.select
            .iter()
            .zip(repeat("select"))
            .chain(ruff.extend_select.iter().zip(repeat("extend-select")))
            .chain(ruff.fixable.iter().zip(repeat("fixable")))
            .chain(ruff.extend_fixable.iter().zip(repeat("extend-fixable")))
            .chain(ruff.ignore.iter().zip(repeat("ignore")))
            .chain(ruff.extend_ignore.iter().zip(repeat("extend-ignore")))
            .chain(
                ruff.per_file_ignores
                    .values()
                    .flatten()
                    .zip(repeat("per-file-ignores")),
            )
            .chain(
                ruff.extend_per_file_ignores
                    .values()
                    .flatten()
                    .zip(repeat("extend-per-file-ignores")),
            )
            .chain(ruff.unfixable.iter().zip(repeat("unfixable")))
            .chain(ruff.extend_unfixable.iter().zip(repeat("extend-unfixable")))
            .chain(
                ruff.extend_safe_fixes
                    .iter()
                    .zip(repeat("extend-safe-fixes")),
            )
            .chain(
                ruff.extend_unsafe_fixes
                    .iter()
                    .zip(repeat("extend-unsafe-fixes")),
            )
            .chain(ruff.lint.selectors())
    }
}

#[derive(Deserialize)]
struct Pyproject {
    tool: Tool,
}

#[derive(Deserialize)]
struct Tool {
    ruff: Ruff,
}

type Selector = Spanned<String>;

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
struct Ruff {
    // Keep these deprecated top-level fields in sync with the selector-valued fields in
    // `ruff_workspace::options::LintCommonOptions`.
    select: Vec<Selector>,
    extend_select: Vec<Selector>,
    fixable: Vec<Selector>,
    extend_fixable: Vec<Selector>,
    ignore: Vec<Selector>,
    extend_ignore: Vec<Selector>,
    per_file_ignores: FxHashMap<String, Vec<Selector>>,
    extend_per_file_ignores: FxHashMap<String, Vec<Selector>>,
    unfixable: Vec<Selector>,
    extend_unfixable: Vec<Selector>,
    extend_safe_fixes: Vec<Selector>,
    extend_unsafe_fixes: Vec<Selector>,

    // Linter options
    lint: Lint,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
struct Lint {
    // Keep these fields in sync with the selector-valued fields in
    // `ruff_workspace::options::LintCommonOptions`.
    select: Vec<Selector>,
    extend_select: Vec<Selector>,
    fixable: Vec<Selector>,
    extend_fixable: Vec<Selector>,
    ignore: Vec<Selector>,
    extend_ignore: Vec<Selector>,
    per_file_ignores: FxHashMap<String, Vec<Selector>>,
    extend_per_file_ignores: FxHashMap<String, Vec<Selector>>,
    unfixable: Vec<Selector>,
    extend_unfixable: Vec<Selector>,
    extend_safe_fixes: Vec<Selector>,
    extend_unsafe_fixes: Vec<Selector>,
}

impl Lint {
    fn selectors(&self) -> impl Iterator<Item = (&Spanned<String>, &'static str)> {
        self.select
            .iter()
            .zip(repeat("lint.select"))
            .chain(self.extend_select.iter().zip(repeat("lint.extend-select")))
            .chain(self.fixable.iter().zip(repeat("lint.fixable")))
            .chain(
                self.extend_fixable
                    .iter()
                    .zip(repeat("lint.extend-fixable")),
            )
            .chain(self.ignore.iter().zip(repeat("lint.ignore")))
            .chain(self.extend_ignore.iter().zip(repeat("lint.extend-ignore")))
            .chain(
                self.per_file_ignores
                    .values()
                    .flatten()
                    .zip(repeat("lint.per-file-ignores")),
            )
            .chain(
                self.extend_per_file_ignores
                    .values()
                    .flatten()
                    .zip(repeat("lint.extend-per-file-ignores")),
            )
            .chain(self.unfixable.iter().zip(repeat("lint.unfixable")))
            .chain(
                self.extend_unfixable
                    .iter()
                    .zip(repeat("lint.extend-unfixable")),
            )
            .chain(
                self.extend_safe_fixes
                    .iter()
                    .zip(repeat("lint.extend-safe-fixes")),
            )
            .chain(
                self.extend_unsafe_fixes
                    .iter()
                    .zip(repeat("lint.extend-unsafe-fixes")),
            )
    }
}

struct RuleCode {
    name: LintName,
    range: TextRange,
}

impl RuleCode {
    /// Extract a rule code and its range from a spanned TOML string.
    ///
    /// The range corresponds to the code itself rather than the surrounding string:
    ///
    /// ```toml
    /// [lint]
    /// select = ["F401"]
    ///            ^^^^
    /// ```
    fn from_spanned(spanned: &Spanned<String>, source: &str) -> Option<Self> {
        let code = spanned.get_ref();
        let code = get_redirect_target(code).unwrap_or(code);
        let rule = Rule::from_code(code).ok()?;

        let span = spanned.span();
        let range = TextRange::new(
            TextSize::try_from(span.start).unwrap(),
            TextSize::try_from(span.end).unwrap(),
        );

        // Note that this should be infallible because the `Spanned<String>` guarantees that the
        // source is surrounded by valid TOML quotes, and `Rule::from_code` above guarantees that
        // the string content is a valid rule code. This means that we don't have to worry about
        // stripping nested quotes like `"'F401'"` or similar.
        let range = {
            let string = &source[range];
            let content = string.trim_start_matches(['"', '\'']);
            let quote_len = string.text_len() - content.text_len();
            let start = range.start() + quote_len;
            let end = range.end() - quote_len;
            TextRange::new(start, end)
        };

        Some(Self {
            name: rule.name(),
            range,
        })
    }
}
