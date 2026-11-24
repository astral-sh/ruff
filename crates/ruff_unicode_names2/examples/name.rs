use std::env;

fn main() {
    let mut arguments = env::args();
    arguments.next(); // Executable name
    for argument in arguments {
        for c in argument.chars() {
            match unicode_names2::name(c) {
                Some(name) => println!("{}", name),
                None => println!("{}: unknown character", argument),
            }
        }
    }
}
