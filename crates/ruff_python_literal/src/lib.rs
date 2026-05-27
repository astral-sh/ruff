pub mod cformat;
pub(crate) mod char;
pub mod escape;
pub mod float;
pub mod format;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Case {
    Lower,
    Upper,
}
