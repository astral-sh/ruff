use ruff::settings::{
    options::Options,
    options_base::{ConfigurationOptions, OptionEntry, OptionField},
};

use crate::ExitStatus;

#[allow(clippy::print_stdout)]
pub(crate) fn config(option: Option<&str>) -> ExitStatus {
    let entries = Options::get_available_options();
    let mut entries = &entries;

    let mut parts_iter = option.iter().flat_map(|s| s.split('.'));

    while let Some(part) = parts_iter.next() {
        let Some((_, field)) = entries.iter().find(|(name, _)| *name == part) else {
            println!("Unknown option");
            return ExitStatus::Error;
        };
        match field {
            OptionEntry::Field(OptionField {
                doc,
                default,
                value_type,
                example,
            }) => {
                if parts_iter.next().is_some() {
                    println!("Unknown option");
                    return ExitStatus::Error;
                }

                println!("{doc}");
                println!();
                println!("Default value: {default}");
                println!("Type: {value_type}");
                println!("Example usage:\n```toml\n{example}\n```");
                return ExitStatus::Success;
            }
            OptionEntry::Group(fields) => {
                entries = fields;
            }
        }
    }
    for (name, _) in entries {
        println!("{name}");
    }
    ExitStatus::Success
}
