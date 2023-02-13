use {
    ruff_text_size::*,
    static_assertions::*,
    std::{
        fmt::Debug,
        hash::Hash,
        marker::{Send, Sync},
        panic::{RefUnwindSafe, UnwindSafe},
    },
};

// auto traits
assert_impl_all!(TextSize: Send, Sync, Unpin, UnwindSafe, RefUnwindSafe);
assert_impl_all!(TextRange: Send, Sync, Unpin, UnwindSafe, RefUnwindSafe);

// common traits
assert_impl_all!(TextSize: Copy, Debug, Default, Hash, Ord);
assert_impl_all!(TextRange: Copy, Debug, Default, Hash, Eq);
