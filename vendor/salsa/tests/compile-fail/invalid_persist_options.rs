#[salsa::input(persist)]
struct Input {
    text: String,
}

#[salsa::input(persist())]
struct Input2 {
    text: String,
}

#[salsa::input(persist(serialize = serde::Serialize::serialize))]
struct Input3 {
    text: String,
}

#[salsa::input(persist(deserialize = serde::Deserialize::deserialize))]
struct Input4 {
    text: String,
}

#[salsa::input(persist(serialize = serde::Serialize::serialize, deserialize = serde::Deserialize::deserialize))]
struct Input5 {
    text: String,
}

#[salsa::input(persist(serialize = serde::Serialize::serialize, serialize = serde::Serialize::serialize))]
struct InvalidInput {
    text: String,
}

#[salsa::input(persist(deserialize = serde::Deserialize::deserialize, deserialize = serde::Deserialize::deserialize))]
struct InvalidInput2 {
    text: String,
}

#[salsa::input(persist(not_an_option = std::convert::identity))]
struct InvalidInput3 {
    text: String,
}

#[salsa::tracked(persist)]
fn tracked_fn(db: &dyn salsa::Database, input: Input) -> String {
    input.text(db).clone()
}

#[salsa::tracked(persist())]
fn tracked_fn2(db: &dyn salsa::Database, input: Input) -> String {
    input.text(db).clone()
}

#[salsa::tracked(persist(serialize = serde::Serialize::serialize, deserialize = serde::Deserialize::deserialize))]
fn invalid_tracked_fn(db: &dyn salsa::Database, input: Input) -> String {
    input.text(db).clone()
}

fn main() {}
