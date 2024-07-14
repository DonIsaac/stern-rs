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
extern crate alloc;

mod heap;
mod ptr;
mod store;
mod tags;
#[cfg(test)]
mod test;

use core::{hash::Hash, marker::PhantomData, ops::Deref, ptr::NonNull};

use alloc::sync::Arc;
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
        let atom = HeapAtom::new(s);
        let inner = unsafe {
            let raw = Arc::into_raw(atom) as *mut HeapAtom;
            TaggedValue::new_ptr(NonNull::new_unchecked(raw))
        };

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
            value.as_bytes_mut()[..len].copy_from_slice(s.as_bytes());
        }

        Self {
            inner: value,
            marker: PhantomData,
        }
    }
}

impl<'a> Atom<'a> {
    pub const fn len(&self) -> usize {
        match self.inner.tag() {
            Tag::HeapOwned => unsafe { HeapAtom::deref_from(self.inner) }.len(),
            Tag::Inline => (self.inner.tag_byte() >> Tag::INLINE_LEN_OFFSET) as usize,
            Tag::Static => {
                panic!("TODO: Atom#len() for Tag::Static")
            }
        }
    }

    fn get_hash(&self) -> u64 {
        match self.inner.tag() {
            Tag::HeapOwned => unsafe { HeapAtom::deref_from(self.inner) }.hash(),
            Tag::Inline => self.inner.hash(),
            Tag::Static => {
                panic!("TODO: Atom#get_hash() for Tag::Static")
            }
        }
    }

    #[inline(always)]
    pub(crate) fn is_heap(&self) -> bool {
        self.inner.tag().is_heap_owned()
    }

    pub fn as_str(&self) -> &str {
        match self.inner.tag() {
            Tag::HeapOwned => unsafe { HeapAtom::deref_from(self.inner) }.as_str(),
            Tag::Inline => unsafe {
                let len = self.inner.len();
                core::str::from_utf8_unchecked(&self.inner.as_bytes()[..len])
            },
            Tag::Static => {
                panic!("TODO: Atom#as_str() for Tag::Static")
            }
        }
    }
}

impl Deref for Atom<'_> {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl Hash for Atom<'_> {
    #[inline]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.get_hash())
    }
}

impl PartialEq for Atom<'_> {
    #[inline(never)]
    fn eq(&self, other: &Self) -> bool {
        if self.inner == other.inner {
            return true;
        }

        if self.inner.tag() != other.inner.tag() {
            return false;
        }

        if self.get_hash() != other.get_hash() {
            return false;
        }

        self.as_str() == self.as_str()
    }
}

// impl<S: AsRef<str>> PartialEq<S> for Atom<'_> {
//     fn eq(&self, other: &S) -> bool {
//         self.as_str() == other.as_ref()
//     }
// }
impl PartialEq<str> for Atom<'_> {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&'_ str> for Atom<'_> {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<Atom<'_>> for str {
    #[inline]
    fn eq(&self, other: &Atom<'_>) -> bool {
        self == other.as_str()
    }
}

impl AsRef<str> for Atom<'_> {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
