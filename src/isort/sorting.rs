/// See: https://github.com/PyCQA/isort/blob/12cc5fbd67eebf92eb2213b03c07b138ae1fb448/isort/sorting.py#L13
use crate::python::string;

#[derive(PartialOrd, Ord, PartialEq, Eq)]
pub enum Prefix {
    Constants,
    Classes,
    Variables,
}

pub fn module_key(module_name: &str) -> String {
    module_name.to_lowercase()
}

pub fn member_key(member_name: &str) -> (Prefix, String) {
    (
        if member_name.len() > 1 && string::is_upper(member_name) {
            // Ex) `CONSTANT`
            Prefix::Constants
        } else if member_name
            .chars()
            .next()
            .map(|char| char.is_uppercase())
            .unwrap_or(false)
        {
            // Ex) `Class`
            Prefix::Classes
        } else {
            // Ex) `variable`
            Prefix::Variables
        },
        member_name.to_lowercase(),
    )
}
