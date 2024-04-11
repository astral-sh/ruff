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

    set_new_parser(false);
    let result_lalrpop = parse_tokens(tokens.clone(), source, source_type.as_mode());

    if result_lalrpop.as_ref().is_ok_and(|ast_lalrpop| {
        ast_lalrpop
            .as_module()
            .is_some_and(|module_lalrpop| module_lalrpop.range.is_empty())
    }) {
        // Reject the corpus which only contains whitespaces, comments, and newlines. This is
        // because the module range produced by LALRPOP is empty while it is the source length in
        // case of the new parser.
        return Corpus::Reject;
    }

    set_new_parser(true);
    let result_new = parse_tokens(tokens, source, source_type.as_mode());

    assert_eq!(result_lalrpop.is_ok(), result_new.is_ok());

    if let (Ok(ast_lalrpop), Ok(ast_new)) = (result_lalrpop, result_new) {
        assert_eq!(ast_lalrpop, ast_new);
    }

    Corpus::Keep
}

fuzz_target!(|case: &[u8]| -> Corpus { do_fuzz(case) });
