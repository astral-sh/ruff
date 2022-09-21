/// Generate a Markdown-compatible table of supported lint rules.
use ruff::checks::{CheckCode, ALL_CHECK_CODES};

fn main() {
    let mut check_codes: Vec<CheckCode> = ALL_CHECK_CODES.to_vec();
    check_codes.sort();

    println!("| Code | Name | Message |");
    println!("| ---- | ----- | ------- |");
    for check_code in check_codes {
        let check_kind = check_code.kind();
        println!(
            "| {} | {} | {} |",
            check_kind.code().as_str(),
            check_kind.name(),
            check_kind.body()
        );
    }
}
