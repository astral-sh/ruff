#[derive(Debug)]
pub struct RuffOptionGroup {
    pub name: &'static str,
    pub fields: Vec<RuffOptionEntry>,
}

#[derive(Debug)]
pub struct RuffOptionField {
    pub name: &'static str,
    pub doc: &'static str,
    pub default: &'static str,
    pub value_type: &'static str,
    pub example: &'static str,
}

#[derive(Debug)]
pub enum RuffOptionEntry {
    Field(RuffOptionField),
    Group(RuffOptionGroup),
}

pub trait ConfigurationOptions {
    fn get_available_options() -> Vec<RuffOptionEntry>;
}
