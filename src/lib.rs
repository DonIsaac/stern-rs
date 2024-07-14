#![cfg_attr(not(feature = "std"), no_std)]

//! # Desired Behavior & Traits
//! 1. Smallest-possible compact representation
//! 2. O(1) comparisons
//! 3. Pre-computed hashes
//!
//! # Invariants
//! 1. The longest possible str is 2^32 - 1 bytes long.
//!   - length can be stored with a u32
//! 2. Atoms are immutable
//! 3. Everything is 8-byte aligned
//!
//! # Assumptions
//! 1. Variable names tend to be small
//! 2. Strings will be frequently re-used
//!   - happens every time a variable/function/class is referenced

#[macro_use]
extern crate assert_unchecked;

mod heap;
mod ptr;
mod store;
mod tags;

use core::marker::PhantomData;

// use std::hash::DefaultHasher;
use heap::HeapAtom;
use tags::{Tag, TaggedValue, MAX_INLINE_LEN};

pub(crate) const ALIGNMENT: usize = 8;

#[derive(Debug)]
pub struct Atom<'a> {
    inner: TaggedValue,
    marker: PhantomData<&'a ()>,
}

impl Atom<'static> {
    pub fn new<S: AsRef<str>>(s: S) -> Self {
        let s = s.as_ref();
        if s.len() <= MAX_INLINE_LEN {
            Self::new_inline_impl(s)
        } else {
            Self::new_heap(s)
        }
    }
    pub const fn empty() -> Self {
        const EMPTY: TaggedValue = TaggedValue::new_inline(0);
        Self {
            inner: EMPTY,
            marker: PhantomData,
        }
    }

    pub fn new_inline(s: &str) -> Self {
        if s.len() > MAX_INLINE_LEN {
            panic!("Cannot inline string '{s}' because its length exceeds the maximum inlineable length of {MAX_INLINE_LEN}");
        }
        Self::new_inline_impl(s)
    }

    fn new_heap(s: &str) -> Self {
        let atom = HeapAtom::new(s.as_ref());
        let inner = unsafe { TaggedValue::new_ptr(atom.as_non_null()) };

        Self {
            inner,
            marker: PhantomData,
        }
    }

    fn new_inline_impl(s: &str) -> Self {
        let len = s.len();
        debug_assert!(len <= MAX_INLINE_LEN);
        let mut value = TaggedValue::new_inline(len as u8);
        unsafe {
            value.data_mut()[..len].copy_from_slice(s.as_bytes());
        }

        Self {
            inner: value,
            marker: PhantomData,
        }
    }

    pub fn len(&self) -> usize {
        match self.inner.tag() {
            Tag::HeapOwned => unsafe { HeapAtom::deref_from(self.inner) }.len(),
            Tag::Inline => (self.inner.tag_byte() >> Tag::INLINE_LEN_OFFSET) as usize,
            Tag::Static => {
                todo!("Atom#len() for Tag::Static")
            }
        }
    }

    #[inline(always)]
    pub(crate) fn is_heap(&self) -> bool {
        self.inner.tag() == Tag::HeapOwned
    }
}

impl<'a> Atom<'a> {}

#[cfg(test)]
mod test {
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
            Atom::new("a very long string that will most certainly be allocated on the heap")
                .is_heap()
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
}
