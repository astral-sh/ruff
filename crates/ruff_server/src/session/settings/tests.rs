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
    let settings_map = SettingsController::from_init_options(options);
    assert_eq!(
        settings_map.settings_for_workspace(Path::new("/Users/test/projects/pandas")),
        ResolvedUserSettings {
            fix_all: FixAll(true),
            organize_imports: OrganizeImports(true),
            lint_enable: LintEnable(true),
            run: RunWhen::OnType,
            disable_rule_comment_enable: CodeActionEnable(false),
            fix_violation_enable: CodeActionEnable(false),
            log_level: LogLevel::Error,
        }
    );
    assert_eq!(
        settings_map.settings_for_workspace(Path::new("/Users/test/projects/scipy")),
        ResolvedUserSettings {
            fix_all: FixAll(true),
            organize_imports: OrganizeImports(true),
            lint_enable: LintEnable(true),
            run: RunWhen::OnType,
            disable_rule_comment_enable: CodeActionEnable(true),
            fix_violation_enable: CodeActionEnable(false),
            log_level: LogLevel::Error,
        }
    );
}

#[test]
fn test_vs_code_unknown_workspace_resolves_to_fallback() {
    let options = test_deserialize! {
        fixture: vs_code_initialization_options,
        into: InitializationOptions,
    };
    let settings_map = SettingsController::from_init_options(options);
    assert_eq!(
        settings_map.settings_for_workspace(Path::new("/Users/test/invalid/workspace")),
        ResolvedUserSettings {
            fix_all: FixAll(false),
            organize_imports: OrganizeImports(true),
            lint_enable: LintEnable(true),
            run: RunWhen::OnType,
            disable_rule_comment_enable: CodeActionEnable(false),
            fix_violation_enable: CodeActionEnable(false),
            log_level: LogLevel::Error,
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

    let settings_map = SettingsController::from_init_options(options);
    assert_eq!(
        settings_map.settings_for_workspace(Path::new("/Users/test/some/workspace")),
        ResolvedUserSettings {
            fix_all: FixAll(false),
            organize_imports: OrganizeImports(true),
            lint_enable: LintEnable(true),
            run: RunWhen::OnSave,
            disable_rule_comment_enable: CodeActionEnable(false),
            fix_violation_enable: CodeActionEnable(true),
            log_level: LogLevel::Warn,
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
