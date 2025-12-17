use ruff_macros::attribute_env_vars_metadata;

/// Declares all environment variable used throughout `ty` and its crates.
pub struct EnvVars;

#[attribute_env_vars_metadata]
impl EnvVars {
    /// If set, ty will use this value as the log level for its `--verbose` output.
    /// Accepts any filter compatible with the `tracing_subscriber` crate.
    ///
    /// For example:
    ///
    /// - `TY_LOG=uv=debug` is the equivalent of `-vv` to the command line
    /// - `TY_LOG=trace` will enable all trace-level logging.
    ///
    /// See the [tracing documentation](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#example-syntax)
    /// for more.
    pub const TY_LOG: &'static str = "TY_LOG";

    /// If set to `"1"` or `"true"`, ty will enable flamegraph profiling.
    /// This creates a `tracing.folded` file that can be used to generate flame graphs
    /// for performance analysis.
    pub const TY_LOG_PROFILE: &'static str = "TY_LOG_PROFILE";

    /// Control memory usage reporting format after ty execution.
    ///
    /// Accepted values:
    ///
    /// * `short` - Display short memory report
    /// * `mypy_primer` - Display `mypy_primer` format and suppress workspace diagnostics
    /// * `full` - Display full memory report
    #[attr_hidden]
    pub const TY_MEMORY_REPORT: &'static str = "TY_MEMORY_REPORT";

    /// Specifies an upper limit for the number of tasks ty is allowed to run in parallel.
    ///
    /// For example, how many files should be checked in parallel.
    /// This isn't the same as a thread limit. ty may spawn additional threads
    /// when necessary, e.g. to watch for file system changes or a dedicated UI thread.
    pub const TY_MAX_PARALLELISM: &'static str = "TY_MAX_PARALLELISM";

    /// Path to a `ty.toml` configuration file to use.
    ///
    /// When set, ty will use this file for configuration instead of
    /// discovering configuration files automatically.
    ///
    /// Equivalent to the `--config-file` command-line argument.
    pub const TY_CONFIG_FILE: &'static str = "TY_CONFIG_FILE";

    /// Used to detect an activated virtual environment.
    pub const VIRTUAL_ENV: &'static str = "VIRTUAL_ENV";

    /// Adds additional directories to ty's search paths.
    /// The format is the same as the shell’s PATH:
    /// one or more directory pathnames separated by os appropriate pathsep
    /// (e.g. colons on Unix or semicolons on Windows).
    pub const PYTHONPATH: &'static str = "PYTHONPATH";

    /// Used to determine the name of the active Conda environment.
    pub const CONDA_DEFAULT_ENV: &'static str = "CONDA_DEFAULT_ENV";

    /// Used to detect the path of an active Conda environment.
    /// If both `VIRTUAL_ENV` and `CONDA_PREFIX` are present, `VIRTUAL_ENV` will be preferred.
    pub const CONDA_PREFIX: &'static str = "CONDA_PREFIX";

    /// Used to determine the root install path of Conda.
    pub const CONDA_ROOT: &'static str = "_CONDA_ROOT";

    /// Filter which tests to run in mdtest.
    ///
    /// Only tests whose names contain this filter string will be executed.
    #[attr_hidden]
    pub const MDTEST_TEST_FILTER: &'static str = "MDTEST_TEST_FILTER";

    /// Switch mdtest output format to GitHub Actions annotations.
    ///
    /// If set (to any value), mdtest will output errors in GitHub Actions format.
    #[attr_hidden]
    pub const MDTEST_GITHUB_ANNOTATIONS_FORMAT: &'static str = "MDTEST_GITHUB_ANNOTATIONS_FORMAT";

    // Externally defined environment variables

    /// Specifies an upper limit for the number of threads ty uses when performing work in parallel.
    /// Equivalent to `TY_MAX_PARALLELISM`.
    ///
    /// This is a standard Rayon environment variable.
    pub const RAYON_NUM_THREADS: &'static str = "RAYON_NUM_THREADS";

    /// Path to user-level configuration directory on Unix systems.
    pub const XDG_CONFIG_HOME: &'static str = "XDG_CONFIG_HOME";
}
