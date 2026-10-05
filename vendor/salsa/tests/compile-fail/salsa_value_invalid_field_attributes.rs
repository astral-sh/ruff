#[derive(salsa::SalsaValue)]
struct Duplicate {
    #[salsa_value(unsafe(prove_safe_to_retain_manually))]
    #[salsa_value(unsafe(prove_safe_to_retain_manually))]
    field: String,
}

#[derive(salsa::SalsaValue)]
struct Malformed {
    #[salsa_value(prove_safe_to_retain_manually)]
    field: String,
}

#[derive(salsa::SalsaValue)]
struct MissingUnsafe<T> {
    #[salsa_value(prove(T: salsa::SalsaValue))]
    field: T,
}

#[derive(salsa::SalsaValue)]
struct EmptyProof<T> {
    #[salsa_value(unsafe(prove()))]
    field: T,
}

#[derive(salsa::SalsaValue)]
struct MalformedPredicate<T> {
    #[salsa_value(unsafe(prove(T salsa::SalsaValue)))]
    field: T,
}

#[salsa::input]
struct Disallowed {
    #[salsa_value(unsafe(prove_safe_to_retain_manually))]
    field: String,
}

#[salsa::input]
struct ConditionalDisallowed {
    #[salsa_value(unsafe(prove(String: salsa::SalsaValue)))]
    field: String,
}

fn main() {}
