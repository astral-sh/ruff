use crate::Location;

#[derive(Debug, PartialEq, Eq)]
pub struct Error<T> {
    pub error: T,
    pub location: Location,
    pub source_path: String,
}

impl<T> std::ops::Deref for Error<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.error
    }
}

impl<T> Error<T> {
    pub fn error(self) -> T {
        self.error
    }
}
