//! Fuzzer harness which double formats the input and access the idempotency or unsteady state of the
//! ruff's formatter.

#![no_main]

use libfuzzer_sys::{fuzz_target, Corpus};
use similar::TextDiff;

use ruff_python_formatter::{format_module_source, PyFormatOptions};

fn do_fuzz(case: &[u8]) -> Corpus {
    // Throw away inputs which aren't utf-8
    let Ok(code) = std::str::from_utf8(case) else {
        return Corpus::Reject;
    };

    let options = PyFormatOptions::default();
    // format the code once
    if let Ok(formatted) = format_module_source(code, options.clone()) {
        let formatted = formatted.as_code();

        // reformat the code second time
        if let Ok(reformatted) = format_module_source(formatted, options.clone()) {
            let reformatted = reformatted.as_code();

            if formatted != reformatted {
                let diff = TextDiff::from_lines(formatted, reformatted)
                    .unified_diff()
                    .header("Formatted Once", "Formatted Twice")
                    .to_string();
                panic!(
                    "\nReformatting the code a second time resulted in formatting changes.\nInput: {:?}\ndiff:\n{}",
                    code, diff
                );
            }
        } else {
            panic!(
                "Unable to format the code second time:\nInput:{:?}\nformatted:\n{:?}",
                code, formatted
            );
        }
    }

    Corpus::Keep
}

fuzz_target!(|case: &[u8]| -> Corpus { do_fuzz(case) });
