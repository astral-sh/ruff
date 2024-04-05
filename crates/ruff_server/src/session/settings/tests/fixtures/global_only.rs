use crate::session::settings::*;

pub(crate) fn expected() -> InitializationOptions {
    InitializationOptions::GlobalOnly {
        settings: Some(UserSettings {
            fix_all: Some(false),
            organize_imports: None,
            lint: Some(Lint { enable: None }),
            code_action: Some(CodeAction {
                disable_rule_comment: Some(CodeActionSettings {
                    enable: Some(false),
                }),
                fix_violation: None,
            }),
        }),
    }
}
