use serde::Deserialize;

use crate::external::ast::rule::ExternalAstRuleSpec;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExternalAstLinterFile {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    #[serde(rename = "rule")]
    pub rules: Vec<ExternalAstRuleSpec>,
}

#[allow(dead_code)]
pub fn _assert_specs_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ExternalAstRuleSpec>();
}
