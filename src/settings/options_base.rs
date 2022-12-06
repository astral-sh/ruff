#[derive(Debug)]
pub struct OptionGroup {
    pub name: &'static str,
    pub fields: Vec<OptionEntry>,
}

#[derive(Debug)]
pub struct OptionField {
    pub name: &'static str,
    pub doc: &'static str,
    pub default: &'static str,
    pub value_type: &'static str,
    pub example: &'static str,
}

#[derive(Debug)]
pub enum OptionEntry {
    Field(OptionField),
    Group(OptionGroup),
}

pub trait ConfigurationOptions {
    fn get_available_options() -> Vec<OptionEntry>;
}
