use crate::db::{QueryResult, SourceDb};
use crate::format::{FormatDb, FormatError, FormattedFile};
use crate::program::Program;

impl Program {
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn format(&mut self) -> QueryResult<()> {
        // Formats all open files

        // TODO make `Executor` from `check` reusable.
        for file in self.workspace.open_files() {
            match self.format_file(file) {
                Ok(FormattedFile::Formatted(content)) => {
                    let path = self.file_path(file);

                    // TODO: This is problematic because it immediately re-triggers the file watcher.
                    //  A possible solution is to track the self "inflicted" changes inside of programs
                    //  by tracking the file revision right after the write. It could then use the revision
                    //  to determine which changes are safe to ignore (and in which context).
                    //  An other alternative is to not write as part of the `format` command and instead
                    //  return a Vec with the format results and leave the writing to the caller.
                    //  I think that's undesired because a) we still need a way to tell the formatter
                    //  that it won't be necessary to format the content again and
                    //  b) it would reduce concurrency because the writing would need to wait for the file
                    //  formatting to be complete, unless we use some form of communication channel.
                    std::fs::write(path, content).expect("Unable to write file");
                }
                Ok(FormattedFile::Unchanged) => {
                    // No op
                }
                Err(FormatError::Query(error)) => {
                    return Err(error);
                }
                Err(FormatError::Format(error)) => {
                    // TODO proper error handling. We should either propagate this error or
                    //  emit a diagnostic (probably this).
                    tracing::warn!("Failed to format file: {}", error);
                }
            }
        }

        Ok(())
    }
}
