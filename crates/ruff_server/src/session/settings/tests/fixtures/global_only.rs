use crate::session::settings::*;

pub(crate) fn expected() -> InitializationOptions {
    InitializationOptions::GlobalOnly {
        settings: Some(UserSettings {
            fix_all: Some(FixAll(false)),
            organize_imports: None,
            lint: Some(Lint {
                enable: None,
                run: Some(RunWhen::OnSave),
            }),
            code_action: Some(CodeAction {
                disable_rule_comment: Some(CodeActionSettings {
                    enable: Some(CodeActionEnable(false)),
                }),
                fix_violation: None,
            }),
            log_level: Some(LogLevel::Warn),
        }),
    }
}
