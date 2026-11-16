use unicode_names2::{character, name};

#[test]
fn character_names() {
    for (ch, expected) in [
        ('a', "LATIN SMALL LETTER A"),
        ('★', "BLACK STAR"),
        ('☃', "SNOWMAN"),
    ] {
        assert_eq!(name(ch).unwrap().to_string(), expected);
        assert_eq!(character(expected), Some(ch));
    }
    assert!(name('\0').is_none());
    assert!(name('\u{10ffff}').is_none());
}

#[test]
fn character_lookup() {
    assert_eq!(character("latin SMALL letter A"), Some('a'));
    assert_eq!(character("latin small letter a"), Some('a'));
    assert_eq!(character("white smiling face"), Some('☺'));
    assert_eq!(character("BACKSPACE"), Some('\x08'));
    assert_eq!(character("nonsense"), None);
}
