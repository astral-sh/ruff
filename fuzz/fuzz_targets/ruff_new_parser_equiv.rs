//! Fuzzer harness which ensures that the handwritten implementation of the parser ("new") is equivalent to the
//! auto-generated lalrpop version.

#![no_main]

use libfuzzer_sys::{fuzz_target, Corpus};

use ruff_python_ast::PySourceType;
use ruff_python_index::tokens_and_ranges;
use ruff_python_parser::{parse_tokens, set_new_parser, AsMode};

// modified from ruff_python_formatter::quick_test
fn do_fuzz(case: &[u8]) -> Corpus {
    let Ok(source) = std::str::from_utf8(case) else {
        return Corpus::Reject;
    };

    let source_type = PySourceType::Python;
    let Ok((tokens, _comment_ranges)) = tokens_and_ranges(source, source_type) else {
        return Corpus::Keep; // keep even rejected source code as this may allow us to explore later
    };

    set_new_parser(true);
    let module_new = parse_tokens(tokens.clone(), source, source_type.as_mode());

    set_new_parser(false);
    let module_lalrpop = parse_tokens(tokens, source, source_type.as_mode());

    assert_eq!(module_lalrpop.is_ok(), module_new.is_ok());

    if let (Ok(module_lalrpop), Ok(module_new)) = (module_lalrpop, module_new) {
        assert_eq!(module_lalrpop, module_new);
    }

    Corpus::Keep
}

fuzz_target!(|case: &[u8]| -> Corpus { do_fuzz(case) });
