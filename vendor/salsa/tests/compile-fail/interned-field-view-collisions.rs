#[salsa::interned(constructor = make, field_view = make)]
struct ConstructorCollision {
    value: u32,
}

#[salsa::interned(field_view = r#read_fields)]
struct GetterCollision {
    #[get(read_fields)]
    value: u32,
}

#[salsa::interned(field_view = ingredient)]
struct IngredientCollision {
    value: u32,
}

#[salsa::interned(field_view = default_debug_fmt)]
struct DebugCollision {
    value: u32,
}

fn main() {}
