// MIRIFLAGS="-Zmiri-preemption-rate=1" cargo +nightly miri test -p char_str --test race_condition
//
// This test exercises concurrent reference-count increments and decrements. Miri's preemption
// mode explores thread interleavings and detects use-after-free races.

use std::thread;

use char_str::CharStr;

#[test]
fn clone_while_dropping_another_reference() {
    let original = CharStr::from("a string longer than the inline limit");
    let dropped = original.clone();

    let thread = thread::spawn(move || drop(dropped));

    let clone = original.clone();
    assert_eq!(clone, "a string longer than the inline limit");
    assert_eq!(original, clone);

    thread.join().unwrap();
}
