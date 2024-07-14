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
            Self::new_inline(s)
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
        // let tag_byte = Tag::INLINE_NONZERO | ((len as u8) <<
        // Tag::INLINE_LEN_OFFSET);
        // let tag_byte = Tag::INLINE_NONZERO |
        // let mut unsafe_data = TaggedValue::new_tag(tag);
        let mut value = TaggedValue::new_inline(len as u8);
        unsafe {
            value.data_mut()[..len].copy_from_slice(s.as_bytes());
        }

        Self {
            inner: value,
            marker: PhantomData,
        }
    }

    #[inline(always)]
    pub(crate) fn is_heap(&self) -> bool {
        self.inner.tag() == Tag::HeapOwned
    }
}

impl<'a> Atom<'a> {}

// impl<'a> From<&'a str> for Atom<'a> {
//     fn from(value: &'a str) -> Self {
//         let ptr = value.as_ptr();
//         let mut hasher = BuildHasherDefault::default();
//         let hash = hasher.hash_one(value);
//         let atom = Box::new(HeapAtom {
//             hash,
//             store_id: NonZerou32::new(1).unwrap(),
//             string: *value
//         });

//         todo!()
//     }
// }
// impl Atom {}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_inlining() {
        // let a = Atom::new("");
        // let b = Atom::new("foo");
        // let c = Atom::new("abcdefg");
        // let c = Atom::new("abcdefgh");
        assert!(!Atom::new("").is_heap());
        assert!(!Atom::new("a").is_heap());
        assert!(!Atom::new("abcdefg").is_heap());
        assert!(Atom::new("abcdefgh").is_heap());
    }
}
