use crate::message::Message;
use crate::settings::Settings;
use crate::{cache, fs};
use libcst_native::parse_module;

pub fn autofix(contents: &str, messages: &[Message]) {
    // Parse the module.
    let mut m = match parse_module(&contents, None) {
        Ok(m) => m,
        Err(e) => panic!("foo"),
    };

    m.body
}
