#![cfg(feature = "inventory")]

use salsa::Database;

#[salsa::input]
struct DefaultInput {
    text: String,
}

#[salsa::interned]
struct DefaultInterned<'db> {
    text: String,
}

#[salsa::tracked]
struct DefaultTracked<'db> {
    text: String,
}

#[salsa::tracked]
fn default_fn(db: &dyn Database, input: DefaultInput) -> String {
    let input: &String = input.text(db);

    let interned = DefaultInterned::new(db, input.clone());
    let _: &String = interned.text(db);

    let tracked = DefaultTracked::new(db, input.clone());
    let _: &String = tracked.text(db);

    input.clone()
}

#[test]
fn default_test() {
    salsa::DatabaseImpl::new().attach(|db| {
        let input = DefaultInput::new(db, "Test".into());
        let _: &String = input.text(db);
        let x: &String = default_fn(db, input);
        expect_test::expect![[r#"
            "Test"
        "#]]
        .assert_debug_eq(x);
    })
}

#[salsa::input]
struct CopyInput {
    #[returns(copy)]
    text: &'static str,
}

#[salsa::tracked(returns(copy))]
fn copy_fn(db: &dyn Database, input: CopyInput) -> &'static str {
    let input: &'static str = input.text(db);
    input
}

#[test]
fn copy_test() {
    salsa::DatabaseImpl::new().attach(|db| {
        let input = CopyInput::new(db, "Test");
        let x: &str = copy_fn(db, input);
        expect_test::expect![[r#"
            "Test"
        "#]]
        .assert_debug_eq(&x);
    })
}

#[salsa::input]
struct CloneInput {
    #[returns(clone)]
    text: String,
}

#[salsa::tracked(returns(clone))]
fn clone_fn(db: &dyn Database, input: CloneInput) -> String {
    let input: String = input.text(db);
    input
}

#[test]
fn clone_test() {
    salsa::DatabaseImpl::new().attach(|db| {
        let input = CloneInput::new(db, "Test".into());
        let x: String = clone_fn(db, input);
        expect_test::expect![[r#"
            "Test"
        "#]]
        .assert_debug_eq(&x);
    })
}

#[salsa::input]
struct RefInput {
    #[returns(ref)]
    text: String,
}

#[salsa::tracked(returns(ref))]
fn ref_fn(db: &dyn Database, input: RefInput) -> String {
    let input: &String = input.text(db);
    input.to_owned()
}

#[test]
fn ref_test() {
    salsa::DatabaseImpl::new().attach(|db| {
        let input = RefInput::new(db, "Test".into());
        let x: &String = ref_fn(db, input);
        expect_test::expect![[r#"
            "Test"
        "#]]
        .assert_debug_eq(&x);
    })
}

#[salsa::input]
struct DerefInput {
    #[returns(deref)]
    text: String,
}

#[salsa::tracked(returns(deref))]
fn deref_fn(db: &dyn Database, input: DerefInput) -> String {
    let input: &str = input.text(db);
    input.to_owned()
}

#[test]
fn deref_test() {
    salsa::DatabaseImpl::new().attach(|db| {
        let input = DerefInput::new(db, "Test".into());
        let x: &str = deref_fn(db, input);
        expect_test::expect![[r#"
            "Test"
        "#]]
        .assert_debug_eq(&x);
    })
}

#[salsa::input]
struct AsRefInput {
    #[returns(as_ref)]
    text: Option<String>,
}

#[salsa::tracked(returns(as_ref))]
fn as_ref_fn(db: &dyn Database, input: AsRefInput) -> Option<String> {
    let input: Option<&String> = input.text(db);
    input.cloned()
}

#[test]
fn as_ref_test() {
    salsa::DatabaseImpl::new().attach(|db| {
        let input = AsRefInput::new(db, Some("Test".into()));
        let x: Option<&String> = as_ref_fn(db, input);
        expect_test::expect![[r#"
            Some(
                "Test",
            )
        "#]]
        .assert_debug_eq(&x);
    })
}

#[salsa::input]
struct AsDerefInput {
    #[returns(as_deref)]
    text: Option<String>,
}

#[salsa::tracked(returns(as_deref))]
fn as_deref_fn(db: &dyn Database, input: AsDerefInput) -> Option<String> {
    let input: Option<&str> = input.text(db);
    input.map(|s| s.to_owned())
}

#[salsa::tracked]
impl AsDerefInput {
    #[salsa::tracked(returns(as_deref))]
    fn as_deref_method(self, db: &dyn Database) -> Option<String> {
        let input: Option<&str> = self.text(db);
        input.map(|s| s.to_owned())
    }
}

#[test]
fn as_deref_test() {
    salsa::DatabaseImpl::new().attach(|db| {
        let input = AsDerefInput::new(db, Some("Test".into()));
        let x: Option<&str> = as_deref_fn(db, input);
        expect_test::expect![[r#"
            Some(
                "Test",
            )
        "#]]
        .assert_debug_eq(&x);
    });
    salsa::DatabaseImpl::new().attach(|db| {
        let input = AsDerefInput::new(db, Some("Test".into()));
        let x: Option<&str> = input.as_deref_method(db);
        expect_test::expect![[r#"
            Some(
                "Test",
            )
        "#]]
        .assert_debug_eq(&x);
    });
}
