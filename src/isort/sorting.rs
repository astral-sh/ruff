/// See: https://github.com/PyCQA/isort/blob/12cc5fbd67eebf92eb2213b03c07b138ae1fb448/isort/sorting.py#L13
use crate::python::string;

#[derive(PartialOrd, Ord, PartialEq, Eq)]
pub enum Prefix {
    Constants,
    Classes,
    Variables,
}

pub fn module_key<'a>(
    name: &'a str,
    asname: &'a Option<String>,
) -> (String, &'a str, &'a Option<String>) {
    (name.to_lowercase(), name, asname)
}

pub fn member_key<'a>(
    name: &'a str,
    asname: &'a Option<String>,
) -> (Prefix, String, &'a Option<String>) {
    (
        if name.len() > 1 && string::is_upper(name) {
            // Ex) `CONSTANT`
            Prefix::Constants
        } else if name
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
        name.to_lowercase(),
        asname,
    )
}
