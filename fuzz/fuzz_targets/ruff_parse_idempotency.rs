//! Fuzzer harness which searches for situations where the parser does not parse or unparse a
//! particular source snippet consistently.

#![no_main]

use libfuzzer_sys::{fuzz_target, Corpus};
use ruff_python_codegen::round_trip;
use similar::TextDiff;

fn do_fuzz(case: &[u8]) -> Corpus {
    let Ok(code) = std::str::from_utf8(case) else {
        return Corpus::Reject;
    };

    // round trip it once to get a formatted version
    if let Ok(first) = round_trip(code) {
        // round trip it a second time to get a case to compare against
        if let Ok(second) = round_trip(&first) {
            if cfg!(feature = "full-idempotency") {
                // potentially, we don't want to test for full idempotency, but just for unsteady states
                // enable the "full-idempotency" feature when fuzzing for full idempotency
                let diff = TextDiff::from_lines(&first, &second)
                    .unified_diff()
                    .header("Parsed once", "Parsed twice")
                    .to_string();
                assert_eq!(
                    first, second,
                    "\nIdempotency violation (orig => first => second); original: {:?}\ndiff:\n{}",
                    code, diff
                );
            } else if first != second {
                // by the third time we've round-tripped it, we shouldn't be introducing any more
                // changes; if we do, then it's likely that we're in an unsteady parsing state
                let third = round_trip(&second).expect("Couldn't round-trip the processed source.");
                let diff = TextDiff::from_lines(&second, &third)
                    .unified_diff()
                    .header("Parsed twice", "Parsed three times")
                    .to_string();
                assert_eq!(
                    second, third,
                    "\nPotential unsteady state (orig => first => second => third); original: {:?}\ndiff:\n{}",
                    code, diff
                );
            }
        } else {
            panic!(
                "Unable to perform the second round trip!\nbefore: {:?}\nfirst: {:?}",
                code, first
            );
        }
    }

    Corpus::Keep
}

fuzz_target!(|case: &[u8]| -> Corpus { do_fuzz(case) });
