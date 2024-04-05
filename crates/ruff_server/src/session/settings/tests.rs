use super::*;

mod fixtures;

/// Attempts to deserialize a JSON object from the fixtures folder
/// and then match it against the return value of the associated
/// `expected()` function for that fixture.
macro_rules! test_deserialize {
    (fixture: $fixture:ident, into: $into:ty$(,)?) => {{
        #[cfg(not(windows))]
        const JSON: &str = include_str!(std::concat!(
            "tests/fixtures/",
            std::stringify!($fixture),
            ".json"
        ));
        #[cfg(windows)]
        const JSON: &str = include_str!(std::concat!(
            r#"tests\fixtures\"#,
            std::stringify!($fixture),
            ".json"
        ));

        let into: $into = serde_json::from_str(JSON).expect("test fixture JSON should deserialize");

        assert_eq!(into, fixtures::$fixture::expected());

        into
    }};
}

#[test]
fn test_vs_code_init_options_deserialize() {
    test_deserialize! {
        fixture: vs_code_initialization_options,
        into: InitializationOptions,
    };
}

#[test]
fn test_vs_code_workspace_settings_resolve() {
    let options = test_deserialize! {
        fixture: vs_code_initialization_options,
        into: InitializationOptions,
    };
    let AllSettings {
        global_settings,
        workspace_settings,
    } = AllSettings::from_init_options(options);
    let url = Url::parse("file:///Users/test/projects/pandas").unwrap();
    assert_eq!(
        ResolvedUserSettings::with_workspace(
            workspace_settings.get(&url).unwrap(),
            &global_settings
        ),
        ResolvedUserSettings {
            fix_all: true,
            organize_imports: true,
            lint_enable: true,
            disable_rule_comment_enable: false,
            fix_violation_enable: false,
        }
    );
    let url = Url::parse("file:///Users/test/projects/scipy").unwrap();
    assert_eq!(
        ResolvedUserSettings::with_workspace(
            workspace_settings.get(&url).unwrap(),
            &global_settings
        ),
        ResolvedUserSettings {
            fix_all: true,
            organize_imports: true,
            lint_enable: true,
            disable_rule_comment_enable: true,
            fix_violation_enable: false,
        }
    );
}

#[test]
fn test_global_only_init_options_deserialize() {
    test_deserialize! {
        fixture: global_only,
        into: InitializationOptions,
    };
}

#[test]
fn test_global_only_resolves_correctly() {
    let options = test_deserialize! {
        fixture: global_only,
        into: InitializationOptions,
    };

    let AllSettings {
        global_settings, ..
    } = AllSettings::from_init_options(options);
    assert_eq!(
        ResolvedUserSettings::global_only(&global_settings),
        ResolvedUserSettings {
            fix_all: false,
            organize_imports: true,
            lint_enable: true,
            disable_rule_comment_enable: false,
            fix_violation_enable: true,
        }
    );
}

#[test]
fn test_empty_init_options_deserialize() {
    test_deserialize! {
        fixture: empty,
        into: InitializationOptions,
    };
}
