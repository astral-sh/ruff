// RUSTFLAGS="--cfg loom" cargo test -p char_str --test loom --release --features loom -- --test-threads=1
#![cfg(loom)]

use char_str::CharStr;
use loom::thread;

#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

#[test]
fn concurrent_clone_and_drop() {
    loom::model(|| {
        let _profiler = dhat::Profiler::builder().testing().build();
        {
            let original = CharStr::from("a string longer than the inline limit");
            let shared = original.clone();

            let thread = thread::spawn(move || {
                let clone = shared.clone();
                assert_eq!(clone, "a string longer than the inline limit");
            });

            let clone = original.clone();
            assert_eq!(clone, "a string longer than the inline limit");
            thread.join().unwrap();
        }

        let stats = dhat::HeapStats::get();
        // https://github.com/tokio-rs/loom/issues/369
        dhat::assert_eq!(stats.curr_blocks, 1);
    });
}
