/// Generate a Markdown-compatible table of supported lint rules.
use ruff::checks::{CheckCode, ALL_CHECK_CODES, DEFAULT_CHECK_CODES};

fn main() {
    let mut check_codes: Vec<CheckCode> = ALL_CHECK_CODES.to_vec();
    check_codes.sort();

    println!("| Code | Name | Message |     |     |");
    println!("| ---- | ---- | ------- | --- | --- |");
    for check_code in check_codes {
        let check_kind = check_code.kind();
        let default_token = if DEFAULT_CHECK_CODES.contains(&check_code) {
            "✅"
        } else {
            ""
        };
        let fix_token = if check_kind.fixable() { "🛠" } else { "" };
        println!(
            "| {} | {} | {} | {} | {} |",
            check_kind.code().as_str(),
            check_kind.name(),
            check_kind.body(),
            default_token,
            fix_token
        );
    }
}
