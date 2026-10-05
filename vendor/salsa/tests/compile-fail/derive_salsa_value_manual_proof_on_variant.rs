#[derive(salsa::SalsaValue)]
enum Value {
    #[salsa_value(unsafe(prove_safe_to_retain_manually))]
    Variant,
}

fn main() {}
