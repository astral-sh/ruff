/// Generate a Markdown-compatible table of supported lint rules.
use strum::IntoEnumIterator;

use ruff::checks::{CheckCode, DEFAULT_CHECK_CODES};

fn main() {
    println!("| Code | Name | Message |     |     |");
    println!("| ---- | ---- | ------- | --- | --- |");
    for check_code in CheckCode::iter() {
        let check_kind = check_code.kind();
        let default_token = if DEFAULT_CHECK_CODES.contains(&check_code) {
            "✅"
        } else {
            ""
        };
        let fix_token = if check_kind.fixable() { "🛠" } else { "" };
        println!(
            "| {} | {} | {} | {} | {} |",
            check_kind.code().as_ref(),
            check_kind.as_ref(),
            check_kind.body(),
            default_token,
            fix_token
        );
    }
}
