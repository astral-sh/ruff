#[salsa::tracked]
struct MyTracked<'db> {
    field: u32,
}

#[salsa::tracked(returns(ref))]
impl<'db> std::default::Default for MyTracked<'db> {
    fn default() -> Self {}
}

#[salsa::tracked(specify)]
impl<'db> std::default::Default for MyTracked<'db> {
    fn default() -> Self {}
}

#[salsa::tracked(no_eq)]
impl<'db> std::default::Default for MyTracked<'db> {
    fn default() -> Self {}
}

#[salsa::tracked(data = Data)]
impl<'db> std::default::Default for MyTracked<'db> {
    fn default() -> Self {}
}

#[salsa::tracked(db = Db)]
impl<'db> std::default::Default for MyTracked<'db> {
    fn default() -> Self {}
}

#[salsa::tracked(recover_fn = recover)]
impl<'db> std::default::Default for MyTracked<'db> {
    fn default() -> Self {}
}

#[salsa::tracked(lru = 32)]
impl<'db> std::default::Default for MyTracked<'db> {
    fn default() -> Self {}
}

#[salsa::tracked(revisions = 32)]
impl<'db> std::default::Default for MyTracked<'db> {
    fn default() -> Self {}
}

#[salsa::tracked(constructor = Constructor)]
impl<'db> std::default::Default for MyTracked<'db> {
    fn default() -> Self {}
}

#[salsa::tracked]
impl<'db> std::default::Default for [MyTracked<'db>; 12] {
    fn default() -> Self {}
}

fn main() {}
