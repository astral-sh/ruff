/// Generate a Markdown-compatible table of supported lint rules.
use strum::IntoEnumIterator;

use ruff::checks::CheckCode;

fn main() {
    println!("| Code | Name | Message |     |");
    println!("| ---- | ---- | ------- | --- |");
    for check_code in CheckCode::iter() {
        let check_kind = check_code.kind();
        let fix_token = if check_kind.fixable() { "🛠" } else { "" };
        println!(
            "| {} | {} | {} | {} |",
            check_kind.code().as_ref(),
            check_kind.as_ref(),
            check_kind.body().replace("|", r"\|"),
            fix_token
        );
    }
}
