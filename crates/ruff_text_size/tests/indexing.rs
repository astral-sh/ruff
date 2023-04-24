use ruff_text_size::TextRange;

#[test]
fn main() {
    let range = TextRange::default();
    _ = &""[range];
    _ = &String::new()[range];
}
