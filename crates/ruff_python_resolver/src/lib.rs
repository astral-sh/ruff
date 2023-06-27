#![allow(dead_code)]

mod config;
mod execution_environment;
mod host;
mod implicit_imports;
mod import_result;
mod module_descriptor;
mod native_module;
mod py_typed;
mod python_platform;
mod python_version;
mod resolver;
mod search;

pub(crate) const SITE_PACKAGES: &str = "site-packages";
