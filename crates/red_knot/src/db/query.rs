use std::fmt::{Display, Formatter};

/// Reason why a db query operation failed.
#[derive(Debug, Clone, Copy)]
pub enum QueryError {
    /// The query was cancelled because the DB was mutated or the query was cancelled by the host (e.g. on a file change or when pressing CTRL+C).
    Cancelled,
}

impl Display for QueryError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryError::Cancelled => f.write_str("query was cancelled"),
        }
    }
}

impl std::error::Error for QueryError {}

pub type QueryResult<T> = Result<T, QueryError>;
