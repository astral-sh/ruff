// Allow dead code because the helpers are compiled for each test using them and
// not all of them use all code in here.
#![allow(dead_code)]

/// Sets an environment variable for the duration this struct is in scope.
pub(crate) struct EnvVarGuard {
    name: String,
    old_value: Option<String>,
}

impl EnvVarGuard {
    pub(crate) fn new(name: &str, value: &str) -> Self {
        let old_value = std::env::var(name).ok();

        std::env::set_var(name, value);

        Self {
            name: name.to_string(),
            old_value,
        }
    }

    pub(crate) fn mock_user_configuration_directory(value: &str) -> Self {
        let env_var_name = if cfg!(windows) {
            "APPDATA"
        } else {
            "XDG_CONFIG_HOME"
        };

        Self::new(env_var_name, value)
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(old_value) = self.old_value.take() {
            std::env::set_var(&self.name, old_value);
        } else {
            std::env::remove_var(&self.name);
        }
    }
}
