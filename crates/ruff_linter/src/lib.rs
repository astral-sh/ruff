//! This is the library for the [Ruff] Python linter.
//!
//! **The API is currently completely unstable**
//! and subject to change drastically.
//!
//! [Ruff]: https://github.com/astral-sh/ruff

pub use locator::Locator;
pub use noqa::generate_noqa_edits;
#[cfg(feature = "clap")]
pub use registry::clap_completion::RuleParser;
#[cfg(feature = "clap")]
pub use rule_selector::clap_completion::RuleSelectorParser;
pub use rule_selector::RuleSelector;
pub use rules::pycodestyle::rules::IOError;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

mod checkers;
pub mod codes;
mod comments;
mod cst;
pub mod directives;
mod doc_lines;
mod docstrings;
mod fix;
pub mod fs;
mod importer;
pub mod line_width;
pub mod linter;
mod locator;
pub mod logging;
pub mod message;
mod noqa;
pub mod package;
pub mod packaging;
pub mod pyproject_toml;
pub mod registry;
mod renamer;
mod rule_redirects;
pub mod rule_selector;
pub mod rules;
pub mod settings;
pub mod source_kind;
mod text_helpers;
pub mod upstream_categories;

#[cfg(any(test, fuzzing))]
pub mod test;

pub const RUFF_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use std::path::Path;

    use ruff_python_ast::PySourceType;

    use crate::codes::Rule;
    use crate::settings::LinterSettings;
    use crate::source_kind::SourceKind;
    use crate::test::{print_messages, test_contents};

    /// Test for ad-hoc debugging.
    #[test]
    #[ignore]
    fn linter_quick_test() {
        let code = r#"class Platform:
    """ Remove sampler
            Args:
            Returns:
            """
"#;
        let source_type = PySourceType::Python;
        let rule = Rule::OverIndentation;

        let source_kind = SourceKind::from_source_code(code.to_string(), source_type)
            .expect("Source code should be valid")
            .expect("Notebook to contain python code");

        let (diagnostics, fixed) = test_contents(
            &source_kind,
            Path::new("ruff_linter/rules/quick_test"),
            &LinterSettings::for_rule(rule),
        );

        assert_eq!(print_messages(&diagnostics), "");
        assert_eq!(fixed.source_code(), code);
    }
}
