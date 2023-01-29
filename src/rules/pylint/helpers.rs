use regex::Regex;
use std::hash::Hash;
#[derive(Debug, Clone)]
pub struct HashRegex(pub Regex);

impl<'a> TryFrom<&'a str> for HashRegex {
    type Error = String;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        match Regex::new(value) {
            Ok(re) => Ok(Self(re)),
            Err(error) => Err(error.to_string()),
        }
    }
}

impl Hash for HashRegex {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write(self.0.as_str().as_bytes());
    }
}
