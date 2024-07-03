use ruff_db::vfs::VfsFile;

use crate::lint::{lint_semantic, lint_syntax, Diagnostics};
use crate::program::Program;

impl Program {
    /// Checks all open files in the workspace and its dependencies.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn check(&self) -> Vec<String> {
        let mut result = Vec::new();
        for open_file in self.workspace.open_files() {
            result.extend_from_slice(&self.check_file(open_file));
        }

        result
    }

    #[tracing::instrument(level = "debug", skip(self))]
    fn check_file(&self, file: VfsFile) -> Diagnostics {
        let mut diagnostics = Vec::new();
        diagnostics.extend_from_slice(lint_syntax(self, file));
        diagnostics.extend_from_slice(lint_semantic(self, file));
        Diagnostics::from(diagnostics)
    }
}
