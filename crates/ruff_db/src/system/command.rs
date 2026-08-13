use std::process::Output;

use super::{Result, SystemPath, SystemPathBuf};

/// An owned description of a command to execute with a [`CommandExecutor`].
#[derive(Debug)]
pub struct Command {
    executable: String,
    arguments: Vec<String>,
    current_directory: Option<SystemPathBuf>,
}

impl Command {
    /// Creates a command for the given executable.
    pub fn new(executable: impl Into<String>) -> Self {
        Self {
            executable: executable.into(),
            arguments: Vec::new(),
            current_directory: None,
        }
    }

    /// Adds an argument to the command.
    pub fn arg(&mut self, argument: impl Into<String>) -> &mut Self {
        self.arguments.push(argument.into());
        self
    }

    /// Adds multiple arguments to the command.
    pub fn args<I, S>(&mut self, arguments: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arguments.extend(arguments.into_iter().map(Into::into));
        self
    }

    /// Sets the working directory for the command.
    pub fn current_dir(&mut self, directory: impl AsRef<SystemPath>) -> &mut Self {
        self.current_directory = Some(directory.as_ref().to_path_buf());
        self
    }

    /// Returns the executable to invoke.
    pub fn get_executable(&self) -> &str {
        &self.executable
    }

    /// Returns the arguments passed to the executable.
    pub fn get_args(&self) -> &[String] {
        &self.arguments
    }

    /// Returns the command's working directory, if explicitly configured.
    pub fn get_current_dir(&self) -> Option<&SystemPath> {
        self.current_directory.as_deref()
    }
}

/// Executes [`Command`]s.
pub trait CommandExecutor: Send + Sync {
    /// Runs a command and captures its standard output and standard error.
    fn execute(&self, command: Command) -> Result<Output>;

    /// Creates an owned executor that can be moved to another thread.
    fn dyn_clone(&self) -> Box<dyn CommandExecutor>;
}
