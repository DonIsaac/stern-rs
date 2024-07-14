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

fn store_with_atoms(texts: Vec<&str>) -> (AtomStore, Vec<Atom>) {
    let mut store = AtomStore::default();

    let atoms = { texts.into_iter().map(|text| store.atom(text)).collect() };

    (store, atoms)
}

#[test]
fn simple_usage() {
    let (s, atoms) = store_with_atoms(vec!["Hello, world!", "Hello, world!"]);

    drop(s);

    let a1 = atoms[0].clone();

    let a2 = atoms[1].clone();

    assert_eq!(a1.inner, a2.inner);
}

#[test]
fn eager_drop() {
    let (_, atoms1) = store_with_atoms(vec!["Hello, world!!!!"]);
    let (_, atoms2) = store_with_atoms(vec!["Hello, world!!!!"]);

    dbg!(&atoms1);
    dbg!(&atoms2);

    let a1 = atoms1[0].clone();
    let a2 = atoms2[0].clone();

    assert_ne!(
        a1.inner, a2.inner,
        "Different stores should have different addresses"
    );
    assert_eq!(a1.get_hash(), a2.get_hash(), "Same string should be equal");
    assert_eq!(a1, a2, "Same string should be equal");
}

#[test]
fn store_multiple() {
    let (_s1, atoms1) = store_with_atoms(vec!["Hello, world!!!!"]);
    let (_s2, atoms2) = store_with_atoms(vec!["Hello, world!!!!"]);

    let a1 = atoms1[0].clone();
    let a2 = atoms2[0].clone();

    assert_ne!(
        a1.inner, a2.inner,
        "Different stores should have different addresses"
    );
    assert_eq!(a1.get_hash(), a2.get_hash(), "Same string should be equal");
    assert_eq!(a1, a2, "Same string should be equal");
}
