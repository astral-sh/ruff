use std::char;

// Count how many code points have names in the standard.
//
// This does a full naive scan of all valid codepoints, and still only
// takes milliseconds to complete. Specifically, with optimisations,
// it takes about 14ms to run, meaning ~12ns per look-up (only 2048
// out of 1114111 fail the from_u32 check).
//
// NB. this is not actually doing any work to compute the name, just
// checking it exists, which is why it can be so efficient.

fn main() {
    let number = (0u32..0x10FFFF)
        .filter(|x| char::from_u32(*x).map_or(false, |c| unicode_names2::name(c).is_some()))
        .count();

    println!("there are {} named code points", number)
}
