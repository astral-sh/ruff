use std::{
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
    path::Path,
};

use serde::{Serialize, Serializer, ser::SerializeSeq};
use serde_json::json;

use crate::diagnostic::Diagnostic;

use super::FileResolver;

pub(super) struct GitlabRenderer<'a> {
    resolver: &'a dyn FileResolver,
}

impl<'a> GitlabRenderer<'a> {
    pub(super) fn new(resolver: &'a dyn FileResolver) -> Self {
        Self { resolver }
    }
}

impl GitlabRenderer<'_> {
    pub(super) fn render(
        &self,
        f: &mut std::fmt::Formatter,
        diagnostics: &[Diagnostic],
    ) -> std::fmt::Result {
        write!(
            f,
            "{:#}",
            serde_json::json!(SerializedMessages {
                diagnostics,
                resolver: self.resolver,
                project_dir: std::env::var("CI_PROJECT_DIR").ok().as_deref(),
            })
        )
    }
}

struct SerializedMessages<'a> {
    diagnostics: &'a [Diagnostic],
    resolver: &'a dyn FileResolver,
    project_dir: Option<&'a str>,
}

impl Serialize for SerializedMessages<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::{
        DiagnosticFormat,
        render::tests::{create_diagnostics, create_syntax_error_diagnostics},
    };

    #[test]
    fn output() {
        let (env, diagnostics) = create_diagnostics(DiagnosticFormat::Gitlab);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn syntax_errors() {
        let (env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Gitlab);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }
}
