//! Fuzzer harness which actively tries to find testcases that cause Ruff to introduce errors into
//! the resulting file.

#![no_main]

use libfuzzer_sys::{fuzz_target, Corpus};
use ruff::settings::Settings;

static mut SETTINGS: Option<Settings> = None;

fn do_fuzz(case: &[u8]) -> Corpus {
    // throw away inputs which aren't utf-8
    let Ok(code) = std::str::from_utf8(case) else { return Corpus::Reject; };

    // the settings are immutable to test_snippet, so we avoid re-initialising here
    let settings = unsafe { SETTINGS.get_or_insert_with(Settings::default) };

    // unlike in the test framework, where the number of iterations is well-defined, we are only
    // looking for situations where a fix is bad; thus, we set the iterations to "infinite"
    let _ = ruff::test::test_snippet(code, settings, usize::MAX);

    Corpus::Keep
}

fuzz_target!(|case: &[u8]| -> Corpus { do_fuzz(case) });
