use crate::session::settings::*;

pub(crate) fn expected() -> InitializationOptions {
    InitializationOptions::HasWorkspaces {
        global_settings: UserSettings {
            fix_all: Some(FixAll(false)),
            organize_imports: Some(OrganizeImports(true)),
            lint: Some(Lint {
                enable: Some(LintEnable(true)),
                run: Some(RunWhen::OnType),
            }),
            code_action: Some(CodeAction {
                disable_rule_comment: Some(CodeActionSettings {
                    enable: Some(CodeActionEnable(false)),
                }),
                fix_violation: Some(CodeActionSettings {
                    enable: Some(CodeActionEnable(false)),
                }),
            }),
            log_level: None,
        },
        workspace_settings: vec![
            WorkspaceSettings {
                user_settings: UserSettings {
                    fix_all: Some(FixAll(true)),
                    organize_imports: Some(OrganizeImports(true)),
                    lint: Some(Lint {
                        enable: Some(LintEnable(true)),
                        run: Some(RunWhen::OnType),
                    }),
                    code_action: Some(CodeAction {
                        disable_rule_comment: Some(CodeActionSettings {
                            enable: Some(CodeActionEnable(false)),
                        }),
                        fix_violation: Some(CodeActionSettings {
                            enable: Some(CodeActionEnable(false)),
                        }),
                    }),
                    log_level: None,
                },
                workspace: Url::parse("file:///Users/test/projects/pandas").unwrap(),
            },
            WorkspaceSettings {
                user_settings: UserSettings {
                    fix_all: Some(FixAll(true)),
                    organize_imports: Some(OrganizeImports(true)),
                    lint: Some(Lint {
                        enable: Some(LintEnable(true)),
                        run: Some(RunWhen::OnType),
                    }),
                    code_action: Some(CodeAction {
                        disable_rule_comment: Some(CodeActionSettings {
                            enable: Some(CodeActionEnable(true)),
                        }),
                        fix_violation: Some(CodeActionSettings {
                            enable: Some(CodeActionEnable(false)),
                        }),
                    }),
                    log_level: None,
                },
                workspace: Url::parse("file:///Users/test/projects/scipy").unwrap(),
            },
        ],
    }
}
