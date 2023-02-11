pub trait ConfigurationOptions {
    fn get_available_options() -> Vec<(&'static str, OptionEntry)>;
}

#[derive(Debug)]
pub enum OptionEntry {
    Field(OptionField),
    Group(Vec<(&'static str, OptionEntry)>),
}

#[derive(Debug)]
pub struct OptionField {
    pub doc: &'static str,
    pub default: &'static str,
    pub value_type: &'static str,
    pub example: &'static str,
}
