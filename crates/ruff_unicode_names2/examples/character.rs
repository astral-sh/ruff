use std::env;

fn main() {
    let mut arguments = env::args();
    arguments.next(); // Executable name
    for argument in arguments {
        match unicode_names2::character(&argument) {
            Some(c) => println!("{}", c),
            None => println!("{}: no such character", argument),
        }
    }
}
