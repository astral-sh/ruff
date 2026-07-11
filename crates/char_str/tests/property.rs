#![expect(
    clippy::needless_pass_by_value,
    reason = "QuickCheck generates owned inputs"
)]

use char_str::CharStr;
use quickcheck as _;

#[quickcheck_macros::quickcheck]
#[cfg_attr(miri, ignore)]
fn roundtrip(input: String) -> bool {
    let value = CharStr::from(input.as_str());

    value.as_str() == input
        && value.len() == input.len()
        && value.is_heap_allocated() == (input.len() > size_of::<CharStr>())
}

#[quickcheck_macros::quickcheck]
#[cfg_attr(miri, ignore)]
fn concatenate(input: Vec<String>) -> bool {
    let slices = input.iter().map(String::as_str).collect::<Vec<_>>();
    let value = CharStr::concat(&slices);

    value.as_str() == input.concat()
}

#[quickcheck_macros::quickcheck]
#[cfg_attr(miri, ignore)]
fn join(input: Vec<String>, separator: String) -> bool {
    let value = CharStr::join(&input, &separator);
    value.as_str() == input.join(&separator)
}

#[quickcheck_macros::quickcheck]
#[cfg_attr(miri, ignore)]
fn collect_chars(input: String) -> bool {
    let value = input.chars().collect::<CharStr>();
    value.as_str() == input
}
