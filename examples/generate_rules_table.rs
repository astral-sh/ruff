//! Generate a Markdown-compatible table of supported lint rules.

use strum::IntoEnumIterator;

use ruff::checks::{CheckCategory, CheckCode};

fn main() {
    for check_category in CheckCategory::iter() {
        println!("### {}", check_category.title());
        println!();

        println!("| Code | Name | Message |");
        println!("| ---- | ---- | ------- |");
        for check_code in CheckCode::iter() {
            if check_code.category() == check_category {
                let check_kind = check_code.kind();
                println!(
                    "| {} | {} | {} |",
                    check_kind.code().as_ref(),
                    check_kind.as_ref(),
                    check_kind.body().replace("|", r"\|")
                );
            }
        }
        println!();
    }
}
