use crate::ExitStatus;
use ruff::settings::options::Options;

#[allow(clippy::print_stdout)]
pub(crate) fn config(key: Option<&str>) -> ExitStatus {
    match key {
        None => print!("{}", Options::metadata()),
        Some(key) => match Options::metadata().get(key) {
            None => {
                println!("Unknown option");
                return ExitStatus::Error;
            }
            Some(entry) => {
                print!("{entry}");
            }
        },
    }
    ExitStatus::Success
}
