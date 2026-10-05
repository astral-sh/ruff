#[salsa::accumulator(returns(ref))]
struct AccWithRetRef(u32);

#[salsa::accumulator(specify)]
struct AccWithSpecify(u32);

#[salsa::accumulator(no_eq)]
struct AccWithNoEq(u32);

#[salsa::accumulator(data = MyAcc)]
struct AccWithData(u32);

#[salsa::accumulator(db = Db)]
struct AcWithcDb(u32);

#[salsa::accumulator(recover_fn = recover)]
struct AccWithRecover(u32);

#[salsa::accumulator(lru = 12)]
struct AccWithLru(u32);

#[salsa::accumulator(revisions = 12)]
struct AccWithRevisions(u32);

#[salsa::accumulator(constructor = Constructor)]
struct AccWithConstructor(u32);

#[salsa::accumulator(heap_size = size)]
struct AccWithHeapSize(u32);

fn main() {}
