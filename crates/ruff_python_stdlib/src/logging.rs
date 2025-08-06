#[derive(Debug, Copy, Clone)]
pub enum LoggingLevel {
    Debug,
    Critical,
    Error,
    Exception,
    Info,
    Warn,
    Warning,
}

impl LoggingLevel {
    pub fn from_attribute(level: &str) -> Option<Self> {
        match level {
            "debug" => Some(Self::Debug),
            "critical" => Some(Self::Critical),
            "error" => Some(Self::Error),
            "exception" => Some(Self::Exception),
            "info" => Some(Self::Info),
            "warn" => Some(Self::Warn),
            "warning" => Some(Self::Warning),
            _ => None,
        }
    }
}
