use {
    ruff_text_size::{TextRange, TextSize},
    std::{cmp::Ordering, ops},
};

fn size(x: u32) -> TextSize {
    TextSize::from(x)
}

fn range(x: ops::Range<u32>) -> TextRange {
    TextRange::new(x.start.into(), x.end.into())
}

#[test]
fn byte_size() {
    let four = TextSize::new(4);
    assert_eq!(four, size(4));
    assert_eq!(four.to_u32(), 4);
    assert_eq!(four.to_usize(), 4);
    assert_eq!(TextSize::of('🦀'), four);
    assert_eq!(TextSize::of("rust-analyzer"), size(13));
}

#[test]
fn range_constructors() {
    let span = range(5..10);
    assert_eq!(span.start(), size(5));
    assert_eq!(span.end(), size(10));
    assert_eq!(span.len(), span.end() - span.start());

    let span = TextRange::at(size(2), size(5));
    assert_eq!(span, range(2..7));
    assert_eq!(&"0123456789"[span], "23456");

    let empty = TextRange::empty(size(3));
    assert!(empty.is_empty());
    assert_eq!(empty, range(3..3));

    let prefix = TextRange::up_to(size(12));
    assert_eq!(prefix.len(), size(12));
    assert_eq!(prefix, range(0..12));
    assert_eq!(prefix, TextRange::at(size(0), size(12)));
}

#[test]
fn range_endpoints() {
    let span = range(5..10);
    assert_eq!(span.with_start(size(8)), range(8..10));
    assert_eq!(span.with_end(size(8)), range(5..8));
    assert_eq!(span.sub_start(size(2)), range(3..10));
    assert_eq!(span.add_start(size(3)), range(8..10));
    assert_eq!(span.sub_end(size(2)), range(5..8));
    assert_eq!(span.add_end(size(2)), range(5..12));
}

#[test]
fn ordering() {
    let span = range(0..3);
    assert_eq!(span.ordering(range(4..5)), Ordering::Less);
    assert_eq!(span.ordering(range(3..5)), Ordering::Less);
    assert_eq!(span.ordering(range(2..5)), Ordering::Equal);
    assert_eq!(span.ordering(range(2..2)), Ordering::Equal);
    assert_eq!(range(2..3).ordering(range(2..2)), Ordering::Greater);
}

#[test]
fn sum() {
    let xs: Vec<TextSize> = vec![size(0), size(1), size(2)];
    assert_eq!(xs.iter().sum::<TextSize>(), size(3));
    assert_eq!(xs.into_iter().sum::<TextSize>(), size(3));
}

#[test]
fn math() {
    assert_eq!(size(10) + size(5), size(15));
    assert_eq!(size(10) - size(5), size(5));
}

#[test]
fn checked_math() {
    assert_eq!(size(1).checked_add(size(1)), Some(size(2)));
    assert_eq!(size(1).checked_sub(size(1)), Some(size(0)));
    assert_eq!(size(1).checked_sub(size(2)), None);
    assert_eq!(size(!0).checked_add(size(1)), None);
}

#[test]
#[rustfmt::skip]
fn contains() {
    assert!(   range(2..4).contains_range(range(2..3)));
    assert!( ! range(2..4).contains_range(range(1..3)));
    assert!(   range(0..20).contains_range(range(5..15)));
    assert!( ! range(5..15).contains_range(range(0..20)));
    assert!(   range(0..20).contains_range(range(0..20)));
}

#[test]
fn intersect() {
    assert_eq!(range(0..10).intersect(range(5..15)), Some(range(5..10)));
    assert_eq!(range(1..2).intersect(range(2..3)), Some(range(2..2)));
    assert_eq!(range(1..5).intersect(range(2..3)), Some(range(2..3)));
    assert_eq!(range(1..2).intersect(range(3..4)), None);
}

#[test]
fn cover() {
    assert_eq!(range(1..2).cover(range(2..3)), range(1..3));
    assert_eq!(range(1..5).cover(range(2..3)), range(1..5));
    assert_eq!(range(1..2).cover(range(4..5)), range(1..5));
}

#[test]
fn cover_offset() {
    assert_eq!(
        TextRange::empty(size(0)).cover_offset(size(20)),
        range(0..20)
    );
    assert_eq!(range(1..3).cover_offset(size(0)), range(0..3));
    assert_eq!(range(1..3).cover_offset(size(1)), range(1..3));
    assert_eq!(range(1..3).cover_offset(size(2)), range(1..3));
    assert_eq!(range(1..3).cover_offset(size(3)), range(1..3));
    assert_eq!(range(1..3).cover_offset(size(4)), range(1..4));
}

#[test]
#[rustfmt::skip]
fn contains_point() {
    assert!( ! range(1..3).contains(size(0)));
    assert!(   range(1..3).contains(size(1)));
    assert!(   range(1..3).contains(size(2)));
    assert!( ! range(1..3).contains(size(3)));
    assert!( ! range(1..3).contains(size(4)));

    assert!( ! range(1..3).contains_inclusive(size(0)));
    assert!(   range(1..3).contains_inclusive(size(1)));
    assert!(   range(1..3).contains_inclusive(size(2)));
    assert!(   range(1..3).contains_inclusive(size(3)));
    assert!( ! range(1..3).contains_inclusive(size(4)));
}
