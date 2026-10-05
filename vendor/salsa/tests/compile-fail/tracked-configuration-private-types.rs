mod private_input {
    #[salsa::input]
    struct Input {
        #[returns(copy)]
        value: u32,
    }

    #[salsa::tracked(returns(copy), configuration = (pub QueryConfiguration))]
    fn query(db: &dyn salsa::Database, input: Input) -> u32 {
        input.value(db)
    }
}

mod private_output {
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct Output(u32);

    #[salsa::tracked(returns(copy), configuration = (pub QueryConfiguration))]
    fn query(_db: &dyn salsa::Database) -> Output {
        Output(0)
    }
}

mod public_marker {
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct Output(u32);

    #[salsa::tracked(returns(copy), configuration = (PrivateConfiguration))]
    pub fn query(_db: &dyn salsa::Database) -> Output {
        Output(0)
    }
}

fn main() {}
