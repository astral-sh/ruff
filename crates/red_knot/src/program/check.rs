use ruff_db::files::File;
use salsa::Cancelled;

use crate::lint::{lint_semantic, lint_syntax, Diagnostics};
use crate::program::Program;

impl Program {
    /// Checks all open files in the workspace and its dependencies.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn check(&self) -> Result<Vec<String>, Cancelled> {
        self.with_db(|db| {
            let mut result = Vec::new();
            for open_file in db.workspace.open_files() {
                result.extend_from_slice(&db.check_file_impl(open_file));
            }

            result
        })
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub fn check_file(&self, file: File) -> Result<Diagnostics, Cancelled> {
        self.with_db(|db| db.check_file_impl(file))
    }

    fn check_file_impl(&self, file: File) -> Diagnostics {
        let mut diagnostics = Vec::new();
        diagnostics.extend_from_slice(lint_syntax(self, file));
        diagnostics.extend_from_slice(lint_semantic(self, file));
        Diagnostics::from(diagnostics)
    }
}
