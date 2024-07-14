use super::*;

/// Atom whose length is on max inline boundary
fn largest_inline() -> Atom<'static> {
    Atom::new("a".repeat(MAX_INLINE_LEN))
}

/// Atom whose length is just past the max inline boundary
fn smallest_heap() -> Atom<'static> {
    Atom::new("a".repeat(MAX_INLINE_LEN + 1))
}

#[test]
fn test_inlining_on_small() {
    assert!(!Atom::new("").is_heap());
    assert!(!Atom::new("a").is_heap());

    assert!(!largest_inline().is_heap());
    assert!(smallest_heap().is_heap());
}

#[test]
fn test_inlining_on_large() {
    assert!(
        Atom::new("a very long string that will most certainly be allocated on the heap").is_heap()
    );
}

#[test]
fn test_len() {
    assert_eq!(Atom::empty().len(), 0);
    assert_eq!(Atom::new("").len(), 0);
    assert_eq!(Atom::new("a").len(), 1);
    assert_eq!(largest_inline().len(), MAX_INLINE_LEN);
    assert_eq!(smallest_heap().len(), MAX_INLINE_LEN + 1);
}
