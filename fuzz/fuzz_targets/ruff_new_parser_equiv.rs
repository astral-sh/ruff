//! Fuzzer harness which ensures that the handwritten implementation of the parser ("new") is equivalent to the
//! auto-generated lalrpop version.

#![no_main]

use libfuzzer_sys::{fuzz_target, Corpus};

use ruff_python_ast::PySourceType;
use ruff_python_index::tokens_and_ranges;
use ruff_python_parser::{parse_ok_tokens_lalrpop, parse_ok_tokens_new, AsMode};

// modified from ruff_python_formatter::quick_test
fn do_fuzz(case: &[u8]) -> Corpus {
    let Ok(source) = std::str::from_utf8(case) else {
        return Corpus::Reject;
    };

    let source_type = PySourceType::Python;
    let Ok((tokens, _comment_ranges)) = tokens_and_ranges(source, source_type) else {
        return Corpus::Keep; // keep even rejected source code as this may allow us to explore later
    };

    let source_path = "fuzzed-source.py";

    let module_new =
        parse_ok_tokens_new(tokens.clone(), source, source_type.as_mode(), source_path);
    let module_lalrpop =
        parse_ok_tokens_lalrpop(tokens, source, source_type.as_mode(), source_path);

    assert_eq!(module_lalrpop.is_ok(), module_new.is_ok());

    if let (Ok(module_lalrpop), Ok(module_new)) = (module_lalrpop, module_new) {
        assert_eq!(module_lalrpop, module_new);
    }

    Corpus::Keep
}

fuzz_target!(|case: &[u8]| -> Corpus { do_fuzz(case) });
