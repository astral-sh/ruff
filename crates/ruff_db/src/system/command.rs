use std::collections::BTreeMap;
use std::process::Output;

use super::{Result, SystemPath, SystemPathBuf};

/// An owned description of a command to execute with a [`CommandExecutor`].
#[derive(Debug)]
pub struct Command {
    executable: String,
    arguments: Vec<String>,
    current_directory: Option<SystemPathBuf>,
    environment: CommandEnv,
}

impl Command {
    /// Creates a command for the given executable.
    pub fn new(executable: impl Into<String>) -> Self {
        Self {
            executable: executable.into(),
            arguments: Vec::new(),
            current_directory: None,
            environment: CommandEnv::default(),
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

    /// Clears the inherited environment and any variables previously set on this command.
    pub fn env_clear(&mut self) -> &mut Self {
        self.environment.clear();
        self
    }

    /// Sets an environment variable for the command.
    pub fn env(&mut self, name: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.environment.set(name, value);
        self
    }

    /// Removes an environment variable from the command.
    pub fn env_remove(&mut self, name: impl Into<String>) -> &mut Self {
        self.environment.remove(name);
        self
    }

    /// Returns the environment variables explicitly set or removed for the command.
    ///
    /// Removed variables have a value of `None`. Variables inherited from the parent process
    /// are not included.
    #[cfg_attr(
        not(feature = "os"),
        expect(dead_code, reason = "available to non-OS command executors")
    )]
    pub(crate) fn get_envs(&self) -> impl Iterator<Item = (&str, Option<&str>)> {
        self.environment
            .vars
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_deref()))
    }

    /// Returns whether the command clears environment variables inherited from its parent process.
    #[cfg_attr(
        not(feature = "os"),
        expect(dead_code, reason = "available to non-OS command executors")
    )]
    pub(crate) fn get_env_clear(&self) -> bool {
        self.environment.get_clear()
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

    pub(super) fn env_merge(&mut self, environment: &CommandEnv) {
        self.environment.merge(environment);
    }
}

/// Environment changes relative to an inherited environment.
#[derive(Debug, Default)]
pub(super) struct CommandEnv {
    clear: bool,
    vars: BTreeMap<String, Option<String>>,
}

impl CommandEnv {
    pub(super) fn get_clear(&self) -> bool {
        self.clear
    }

    pub(super) fn get(&self, name: &str) -> Option<&Option<String>> {
        self.vars.get(name)
    }

    pub(super) fn clear(&mut self) {
        self.clear = true;
        self.vars.clear();
    }

    pub(super) fn set(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.vars.insert(name.into(), Some(value.into()));
    }

    pub(super) fn remove(&mut self, name: impl Into<String>) {
        let name = name.into();
        if self.clear {
            self.vars.remove(&name);
        } else {
            self.vars.insert(name, None);
        }
    }

    /// Uses `environment` as a base without replacing explicit changes.
    fn merge(&mut self, environment: &Self) {
        if self.clear {
            return;
        }

        self.clear = environment.clear;
        for (name, value) in &environment.vars {
            self.vars
                .entry(name.clone())
                .or_insert_with(|| value.clone());
        }
    }
}

/// Executes [`Command`]s.
pub trait CommandExecutor: Send + Sync {
    /// Runs a command and captures its standard output and standard error.
    fn execute(&self, command: Command) -> Result<Output>;

    /// Creates an owned executor that can be moved to another thread.
    fn dyn_clone(&self) -> Box<dyn CommandExecutor>;
}
