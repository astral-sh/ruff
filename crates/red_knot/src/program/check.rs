use ruff_db::vfs::VfsFile;
use salsa::Cancelled;

use crate::lint::{lint_semantic, lint_syntax, Diagnostics};
use crate::program::Program;

impl Program {
    /// Checks all open files in the workspace and its dependencies.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn check(&self) -> Result<Vec<String>, Cancelled> {
        // TODO(micha): Wrap in `Cancelled::catch`
        //   See https://salsa.zulipchat.com/#narrow/stream/145099-general/topic/How.20to.20use.20.60Cancelled.3A.3Acatch.60
        let mut result = Vec::new();
        for open_file in self.workspace.open_files() {
            result.extend_from_slice(&self.check_file(open_file));
        }

        Ok(result)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    fn check_file(&self, file: VfsFile) -> Diagnostics {
        let mut diagnostics = Vec::new();
        diagnostics.extend_from_slice(lint_syntax(self, file));
        diagnostics.extend_from_slice(lint_semantic(self, file));
        Diagnostics::from(diagnostics)
    }
}
