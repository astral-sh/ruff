//! Fuzzer harness which merely explores the parse/unparse coverage space and tries to make it
//! crash. On its own, this fuzzer is (hopefully) not going to find a crash.

#![no_main]

use libfuzzer_sys::{fuzz_target, Corpus};
use ruff_python_codegen::{Generator, Stylist};
use ruff_python_parser::{lexer, parse_suite, Mode, ParseError};
use ruff_source_file::Locator;

fn do_fuzz(case: &[u8]) -> Corpus {
    let Ok(code) = std::str::from_utf8(case) else {
        return Corpus::Reject;
    };

    // just round-trip it once to trigger both parse and unparse
    let locator = Locator::new(code);
    let python_ast = match parse_suite(code) {
        Ok(stmts) => stmts,
        Err(ParseError { offset, .. }) => {
            let offset = offset.to_usize();
            assert!(
                code.is_char_boundary(offset),
                "Invalid error location {} (not at char boundary)",
                offset
            );
            return Corpus::Keep;
        }
    };

    let tokens: Vec<_> = lexer::lex(code, Mode::Module).collect();

    for maybe_token in tokens.iter() {
        match maybe_token.as_ref() {
            Ok((_, range)) => {
                let start = range.start().to_usize();
                let end = range.end().to_usize();
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
            Err(err) => {
                let offset = err.location().to_usize();
                assert!(
                    code.is_char_boundary(offset),
                    "Invalid error location {} (not at char boundary)",
                    offset
                );
            }
        }
    }

    let stylist = Stylist::from_tokens(&tokens, &locator);
    let mut generator: Generator = (&stylist).into();
    generator.unparse_suite(&python_ast);

    Corpus::Keep
}

fuzz_target!(|case: &[u8]| -> Corpus { do_fuzz(case) });
