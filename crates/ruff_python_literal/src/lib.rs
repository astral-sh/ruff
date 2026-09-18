pub mod cformat;
mod char;
pub mod escape;
pub mod float;
pub mod format;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Case {
    Lower,
    Upper,
}
