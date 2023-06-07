//! Fuzzer harness which merely explores the parse/unparse coverage space and tries to make it
//! crash. On its own, this fuzzer is (hopefully) not going to find a crash.

#![no_main]

#[cfg(feature = "libafl")]
extern crate libafl_libfuzzer;

use libfuzzer_sys::{fuzz_target, Corpus};
use ruff_python_ast::source_code::round_trip;

fn do_fuzz(case: &[u8]) -> Corpus {
    let Ok(code) = std::str::from_utf8(case) else { return Corpus::Reject; };

    // just round-trip it once to trigger both parse and unparse
    let _ = round_trip(code, "fuzzed-source.py");

    Corpus::Keep
}

fuzz_target!(|case: &[u8]| -> Corpus { do_fuzz(case) });
