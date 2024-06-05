//! Fuzzer harness which merely explores the parse/unparse coverage space and tries to make it
//! crash. On its own, this fuzzer is (hopefully) not going to find a crash.

#![no_main]

use libfuzzer_sys::{fuzz_target, Corpus};
use ruff_python_codegen::{Generator, Stylist};
use ruff_python_parser::{parse_module, ParseError};
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

fn do_fuzz(case: &[u8]) -> Corpus {
    let Ok(code) = std::str::from_utf8(case) else {
        return Corpus::Reject;
    };

    // just round-trip it once to trigger both parse and unparse
    let locator = Locator::new(code);
    let parsed = match parse_module(code) {
        Ok(parsed) => parsed,
        Err(ParseError { location, .. }) => {
            let offset = location.start().to_usize();
            assert!(
                code.is_char_boundary(offset),
                "Invalid error location {} (not at char boundary)",
                offset
            );
            return Corpus::Keep;
        }
    };

    for token in parsed.tokens() {
        let start = token.start().to_usize();
        let end = token.end().to_usize();
        assert!(
            code.is_char_boundary(start),
            "Invalid start position {} (not at char boundary)",
            start
        );
        assert!(
            code.is_char_boundary(end),
            "Invalid end position {} (not at char boundary)",
            end
        );
    }

    let stylist = Stylist::from_tokens(parsed.tokens(), &locator);
    let mut generator: Generator = (&stylist).into();
    generator.unparse_suite(parsed.suite());

    Corpus::Keep
}

fuzz_target!(|case: &[u8]| -> Corpus { do_fuzz(case) });
