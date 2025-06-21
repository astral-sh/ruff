/// Resolved client settings for a specific document. These settings are meant to be
/// used directly by the server, and are *not* a 1:1 representation with how the client
/// sends them.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub(crate) struct ClientSettings {
    pub(super) disable_language_services: bool,
}

impl ClientSettings {
    pub(crate) fn is_language_services_disabled(&self) -> bool {
        self.disable_language_services
    }
}
