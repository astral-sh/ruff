//! Fuzzer harness which actively tries to find testcases that cause Ruff to introduce errors into
//! the resulting file.

#![no_main]

use std::collections::HashMap;
use std::sync::OnceLock;

use libfuzzer_sys::{fuzz_target, Corpus};
use ruff_linter::linter::ParseSource;
use ruff_linter::settings::flags::Noqa;
use ruff_linter::settings::LinterSettings;
use ruff_linter::source_kind::SourceKind;
use ruff_python_ast::PySourceType;
use ruff_python_formatter::{format_module_source, PyFormatOptions};
use similar::TextDiff;

static SETTINGS: OnceLock<LinterSettings> = OnceLock::new();

fn do_fuzz(case: &[u8]) -> Corpus {
    // throw away inputs which aren't utf-8
    let Ok(code) = std::str::from_utf8(case) else {
        return Corpus::Reject;
    };

    // the settings are immutable to test_snippet, so we avoid re-initialising here
    let linter_settings = SETTINGS.get_or_init(LinterSettings::default);
    let format_options = PyFormatOptions::default();

    let linter_result = ruff_linter::linter::lint_only(
        "fuzzed-source.py".as_ref(),
        None,
        linter_settings,
        Noqa::Enabled,
        &SourceKind::Python(code.to_string()),
        PySourceType::Python,
        ParseSource::None,
    );

    if linter_result.has_syntax_error {
        return Corpus::Keep; // keep, but don't continue
    }

    let mut warnings = HashMap::new();

    for msg in &linter_result.messages {
        let count: &mut usize = warnings.entry(msg.name()).or_default();
        *count += 1;
    }

    // format the code once
    if let Ok(formatted) = format_module_source(code, format_options.clone()) {
        let formatted = formatted.as_code().to_string();

        let linter_result = ruff_linter::linter::lint_only(
            "fuzzed-source.py".as_ref(),
            None,
            linter_settings,
            Noqa::Enabled,
            &SourceKind::Python(formatted.clone()),
            PySourceType::Python,
            ParseSource::None,
        );

        assert!(
            !linter_result.has_syntax_error,
            "formatter introduced a parse error"
        );

        for msg in &linter_result.messages {
            if let Some(count) = warnings.get_mut(msg.name()) {
                if let Some(decremented) = count.checked_sub(1) {
                    *count = decremented;
                } else {
                    panic!(
                        "formatter introduced additional linter warning: {msg:?}\ndiff: {}",
                        TextDiff::from_lines(code, &formatted)
                            .unified_diff()
                            .header("Unformatted", "Formatted")
                    );
                }
            } else {
                panic!(
                    "formatter introduced new linter warning that was not previously present: {msg:?}\ndiff: {}",
                    TextDiff::from_lines(code, &formatted)
                        .unified_diff()
                        .header("Unformatted", "Formatted")
                );
            }
        }
    }

    Corpus::Keep
}

fuzz_target!(|case: &[u8]| -> Corpus { do_fuzz(case) });
