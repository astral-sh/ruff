use std::hint::black_box;

#[salsa::input]
pub(crate) struct Item {
    #[returns(copy)]
    value: u64,
}

#[derive(PartialEq, Clone, Copy)]
pub(crate) struct Value(pub(crate) u64);

#[salsa::tracked(returns(copy))]
pub(crate) fn no_eviction_value(db: &dyn salsa::Database, item: Item) -> Value {
    compute_value(item.value(db))
}

#[salsa::tracked(returns(copy), lru = 4096)]
pub(crate) fn lru_value(db: &dyn salsa::Database, item: Item) -> Value {
    compute_value(item.value(db))
}

#[derive(Clone, Copy)]
pub(crate) enum Policy {
    NoEviction,
    Lru,
}

impl Policy {
    pub(crate) fn set_capacity(self, db: &mut salsa::DatabaseImpl, capacity: usize) {
        match self {
            Self::NoEviction => {}
            Self::Lru => lru_value::set_lru_capacity(db, capacity),
        }
    }
}

impl std::fmt::Display for Policy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NoEviction => "NoEviction",
            Self::Lru => "Lru",
        })
    }
}

#[inline]
pub(crate) fn access_all(
    policy: Policy,
    db: &dyn salsa::Database,
    items: impl IntoIterator<Item = Item>,
) -> Value {
    match policy {
        Policy::NoEviction => access_all_with(db, items, no_eviction_value),
        Policy::Lru => access_all_with(db, items, lru_value),
    }
}

pub(crate) fn access_all_with(
    db: &dyn salsa::Database,
    items: impl IntoIterator<Item = Item>,
    fetch: impl Fn(&dyn salsa::Database, Item) -> Value,
) -> Value {
    let mut sum = 0u64;
    for item in items {
        sum = sum.wrapping_add(fetch(black_box(db), black_box(item)).0);
    }
    Value(sum)
}

pub(crate) fn new_items(db: &salsa::DatabaseImpl, count: usize) -> Vec<Item> {
    (0..count)
        .map(|value| Item::new(db, value as u64))
        .collect()
}

pub(crate) fn prewarm(policy: Policy, db: &dyn salsa::Database, items: &[Item]) {
    black_box(access_all(policy, db, items.iter().copied()));
}

fn compute_value(_value: u64) -> Value {
    const FIBONACCI_STEPS: u32 = 128;

    let mut previous = 0u64;
    let mut current = 1u64;

    for _ in 0..black_box(FIBONACCI_STEPS) {
        (previous, current) = (current, previous.wrapping_add(current));
    }

    Value(previous)
}
