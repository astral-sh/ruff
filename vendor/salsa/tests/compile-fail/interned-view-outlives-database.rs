#[salsa::interned(unsafe(no_lifetime), revisions = usize::MAX, field_view = read_fields)]
struct Immortal {
    text: String,
}

fn main() {
    let text: &String = {
        let db = salsa::DatabaseImpl::new();
        let value = Immortal::new(&db, String::from("borrowed"));
        value.read_fields(salsa::FieldReads::new(&db)).text()
    };
    println!("{text}");
}
