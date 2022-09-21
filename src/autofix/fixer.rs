#[derive(Hash)]
pub enum Mode {
    Generate,
    Apply,
    None,
}

impl From<bool> for Mode {
    fn from(value: bool) -> Self {
        match value {
            true => Mode::Apply,
            false => Mode::None,
        }
    }
}
