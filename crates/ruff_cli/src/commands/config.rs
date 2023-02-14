use ruff::settings::{
    options::Options,
    options_base::{ConfigurationOptions, OptionEntry, OptionField},
};

use crate::ExitStatus;

#[allow(clippy::print_stdout)]
pub(crate) fn config(key: Option<&str>) -> ExitStatus {
    let Some(entry) = Options::get(key) else {
        println!("Unknown option");
        return ExitStatus::Error;
    };

    match entry {
        OptionEntry::Field(OptionField {
            doc,
            default,
            value_type,
            example,
        }) => {
            println!("{doc}");
            println!();
            println!("Default value: {default}");
            println!("Type: {value_type}");
            println!("Example usage:\n```toml\n{example}\n```");
        }
        OptionEntry::Group(entries) => {
            for (name, _) in entries {
                println!("{name}");
            }
        }
    }

    ExitStatus::Success
}
