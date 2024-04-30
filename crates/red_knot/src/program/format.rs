use crate::db::QueryResult;
use crate::format::{FormatDb, FormatError};
use crate::program::Program;

impl Program {
    pub fn format(&self) -> QueryResult<()> {
        // Formats all open files

        // Do we need a similar concurrency abstraction as for `check`? That would be slightly annoying.
        for file in &self.workspace.open_files {
            // TODO how to write back the formatted output? Ideally that happens on each formatting thread.
            // The write to the file system triggers a new event that is picked up by the watcher, which in turn
            // cancels the format operation...
            // We also want to avoid reformatting those files, because we've just formatted them but
            // we do want to invalidate them every other time.
            if let Err(FormatError::Query(err)) = self.format_file(*file) {
                return Err(err);
            }

            // TODO handle other error cases.
        }

        Ok(())
    }
}
