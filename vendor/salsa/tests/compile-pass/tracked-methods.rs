#[derive(Copy, Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub struct Things<'db> {
    v: std::marker::PhantomData<&'db ()>,
}

#[salsa::interned]
struct Other {
    #[returns(copy)]
    b: bool,
}

#[salsa::tracked]
impl Things<'_> {
    #[salsa::tracked]
    pub fn implicit(db: &dyn salsa::Database, _: Other<'_>) -> Things<'_> {
        _ = db;
        todo!()
    }

    #[salsa::tracked(returns(copy))]
    pub fn implicit2(db: &dyn salsa::Database, _: Other<'_>) -> Things<'_> {
        _ = db;
        todo!()
    }
}

#[salsa::tracked]
impl<'db> Things<'db> {
    #[salsa::tracked]
    pub fn explicit(db: &'db dyn salsa::Database, _: Other<'db>) -> Things<'db> {
        _ = db;
        todo!()
    }

    #[salsa::tracked(returns(copy))]
    pub fn explicit2(db: &'db dyn salsa::Database, _: Other<'db>) -> Things<'db> {
        _ = db;
        todo!()
    }

    #[salsa::tracked]
    pub fn implicit3(db: &dyn salsa::Database, _: Other<'_>) -> Things<'_> {
        _ = db;
        todo!()
    }

    #[salsa::tracked(returns(copy))]
    pub fn implicit4(db: &dyn salsa::Database, _: Other<'_>) -> Things<'_> {
        _ = db;
        todo!()
    }

    #[salsa::tracked(returns(copy))]
    pub fn static_value(db: &dyn salsa::Database) -> &'static str {
        _ = db;
        "test"
    }

    #[salsa::tracked(returns(copy))]
    pub fn static_value2(db: &dyn salsa::Database) -> &'static str {
        _ = db;
        "test"
    }
}

#[salsa::tracked]
impl<'a> Things<'a> {
    // The body refers to the impl's lifetime by name; the generated signature
    // must keep using that same name.
    #[salsa::tracked(returns(copy))]
    fn body_annotation(db: &'a dyn salsa::Database) -> u32 {
        let _: &'a dyn salsa::Database = db;
        0
    }

    // A higher-ranked function pointer return whose binder reuses `'db`; the db
    // lifetime must not be renamed into the binder. `no_eq` avoids comparing fn
    // pointers (which is unpredictable and only incidental to this test).
    #[salsa::tracked(returns(copy), no_eq, unsafe(non_salsa_values))]
    fn hrtb(db: &'a dyn salsa::Database) -> for<'db> fn(&'a (), &'db ()) {
        _ = db;
        unimplemented!()
    }

    // An elided, independent input lifetime (`Other<'_>`) must be tied to the db
    // lifetime on the outer signature so the call to the inner fn type-checks.
    #[salsa::tracked(returns(copy))]
    fn named_return(db: &'a dyn salsa::Database, other: Other<'_>) -> Things<'a> {
        _ = (db, other);
        unimplemented!()
    }
}

fn main() {}
