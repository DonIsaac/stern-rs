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
mod store;
mod tags;
mod ptr;

use core::hash::{BuildHasher, BuildHasherDefault, Hash, Hasher as _};
use core::marker::PhantomData;
use core::num::NonZeroU32;
use core::ptr::NonNull;
use std::borrow::Cow;
// use std::hash::DefaultHasher;
use heap::{Header, HeapAtom};
use tags::{Tag, TaggedValue, MAX_INLINE_LEN};

pub(crate) const ALIGNMENT: usize = 8;

#[derive(Debug)]
pub struct Atom<'a> {
    inner: TaggedValue,
    marker: PhantomData<&'a ()>,
}

impl Atom<'static> {
    fn new_heap<S: AsRef<str>>(s: &S) -> Self {
        let atom = HeapAtom::new(s.as_ref());
        let inner = unsafe { TaggedValue::new_ptr(atom.as_non_null()) };

        Self {
            inner,
            marker: PhantomData
        }
    }
    fn new_inline<S: AsRef<str>>(s: &S) -> Self {
        let s = s.as_ref();
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
            marker: PhantomData
        }
    }
}

impl<'a> Atom<'a> {
}

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
