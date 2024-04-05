use crate::session::settings::*;

pub(crate) fn expected() -> InitializationOptions {
    InitializationOptions::HasWorkspaces {
        global_settings: UserSettings {
            fix_all: Some(false),
            organize_imports: Some(true),
            lint: Some(Lint { enable: Some(true) }),
            code_action: Some(CodeAction {
                disable_rule_comment: Some(CodeActionSettings {
                    enable: Some(false),
                }),
                fix_violation: Some(CodeActionSettings {
                    enable: Some(false),
                }),
            }),
        },
        workspace_settings: vec![
            WorkspaceSettings {
                user_settings: UserSettings {
                    fix_all: Some(true),
                    organize_imports: Some(true),
                    lint: Some(Lint { enable: Some(true) }),
                    code_action: Some(CodeAction {
                        disable_rule_comment: Some(CodeActionSettings {
                            enable: Some(false),
                        }),
                        fix_violation: Some(CodeActionSettings {
                            enable: Some(false),
                        }),
                    }),
                },
                workspace: Url::parse("file:///Users/test/projects/pandas").unwrap(),
            },
            WorkspaceSettings {
                user_settings: UserSettings {
                    fix_all: Some(true),
                    organize_imports: Some(true),
                    lint: Some(Lint { enable: Some(true) }),
                    code_action: Some(CodeAction {
                        disable_rule_comment: Some(CodeActionSettings { enable: Some(true) }),
                        fix_violation: Some(CodeActionSettings {
                            enable: Some(false),
                        }),
                    }),
                },
                workspace: Url::parse("file:///Users/test/projects/scipy").unwrap(),
            },
        ],
    }
}
