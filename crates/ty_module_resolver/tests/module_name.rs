use ty_module_resolver::ModuleName;

fn name(value: &'static str) -> ModuleName {
    ModuleName::new_static(value).unwrap()
}

#[test]
fn validation() {
    assert_eq!(
        ModuleName::new_static("foo.bar").as_deref(),
        Some("foo.bar")
    );
    for invalid in ["", "..foo", ".foo", "foo.", "foo..bar", "2000"] {
        assert_eq!(ModuleName::new_static(invalid), None, "{invalid:?}");
    }
}

#[test]
fn components() {
    let module = name("foo.bar.baz");
    assert_eq!(
        module.components().collect::<Vec<_>>(),
        ["foo", "bar", "baz"]
    );
    assert_eq!(module.first_component(), "foo");
    assert_eq!(module.last_component(), "baz");
}

#[test]
fn from_components() {
    assert_eq!(ModuleName::from_components(["a"]).as_deref(), Some("a"));
    assert_eq!(
        ModuleName::from_components(["a", "b"]).as_deref(),
        Some("a.b")
    );
    assert_eq!(
        ModuleName::from_components(["a", "b", "c"]).as_deref(),
        Some("a.b.c")
    );
    assert_eq!(ModuleName::from_components(["a-b"]), None);
    assert_eq!(ModuleName::from_components(["a", "a-b"]), None);
    assert_eq!(ModuleName::from_components(["a", "b", "a-b-c"]), None);
}

#[test]
fn extend() {
    let mut module = name("foo");
    module.extend(&name("bar"));
    assert_eq!(module.as_str(), "foo.bar");
    module.extend(&name("baz.eggs.ham"));
    assert_eq!(module.as_str(), "foo.bar.baz.eggs.ham");
}

#[test]
fn parents_and_ancestors() {
    let module = name("foo.bar.baz");
    assert_eq!(module.parent().as_deref(), Some("foo.bar"));
    assert_eq!(name("foo.bar").parent().as_deref(), Some("foo"));
    assert_eq!(name("foo").parent(), None);
    assert_eq!(
        module.ancestors().collect::<Vec<_>>(),
        [module, name("foo.bar"), name("foo")]
    );
}

#[test]
fn starts_with_whole_components() {
    assert!(name("foo.bar").starts_with(&name("foo")));
    assert!(!name("foo.bar").starts_with(&name("bar")));
    assert!(!name("foo_bar").starts_with(&name("foo")));
}

#[test]
fn relative_to_parent() {
    let importlib = name("importlib");
    assert_eq!(
        name("importlib.resources")
            .relative_to(&importlib)
            .as_deref(),
        Some("resources")
    );
    assert_eq!(name("importliblib.resources").relative_to(&importlib), None);
    assert_eq!(
        name("importlibbbbb.resources").relative_to(&importlib),
        None
    );

    let module = name("foo.bar.baz.quux");
    assert_eq!(
        module.relative_to(&name("foo.bar")).as_deref(),
        Some("baz.quux")
    );
    assert_eq!(module.relative_to(&name("foo.barbaz")), None);
}

#[test]
fn test_modules() {
    for module in [
        "numpy.tests",
        "requests.test",
        "conftest",
        "foo.bar.conftest",
    ] {
        assert!(name(module).is_test_module(), "{module}");
    }
    for module in ["foo.testing", "tests", "test", "pytest", "unittest"] {
        assert!(!name(module).is_test_module(), "{module}");
    }
}

#[test]
fn private_modules() {
    for module in ["_foo", "foo._bar", "foo._bar.quux"] {
        assert!(name(module).is_private(), "{module}");
    }
}
