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
            "debug" => Some(LoggingLevel::Debug),
            "critical" => Some(LoggingLevel::Critical),
            "error" => Some(LoggingLevel::Error),
            "exception" => Some(LoggingLevel::Exception),
            "info" => Some(LoggingLevel::Info),
            "warn" => Some(LoggingLevel::Warn),
            "warning" => Some(LoggingLevel::Warning),
            _ => None,
        }
    }
}
