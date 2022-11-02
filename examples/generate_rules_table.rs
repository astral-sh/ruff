//! Generate a Markdown-compatible table of supported lint rules.

use ruff::checks::{CheckCategory, CheckCode};
use strum::IntoEnumIterator;

fn main() {
    for check_category in CheckCategory::iter() {
        println!("### {}", check_category.title());
        println!();

        println!("| Code | Name | Message | Fix |");
        println!("| ---- | ---- | ------- | --- |");
        for check_code in CheckCode::iter() {
            if check_code.category() == check_category {
                let check_kind = check_code.kind();
                let fix_token = if check_kind.fixable() { "🛠" } else { "" };
                println!(
                    "| {} | {} | {} | {} |",
                    check_kind.code().as_ref(),
                    check_kind.as_ref(),
                    check_kind.summary().replace("|", r"\|"),
                    fix_token
                );
            }
        }
        println!();
    }
}
