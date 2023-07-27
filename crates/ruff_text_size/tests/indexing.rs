use ruff_text_size::TextRange;

#[test]
fn main() {
    let range = TextRange::default();
    let _ = &""[range];
    let _ = &String::new()[range];
}
