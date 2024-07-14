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
mod store;
mod tags;
#[cfg(test)]
mod test;

use core::{hash::Hash, marker::PhantomData, ops::Deref};

use alloc::{borrow::Cow, sync::Arc};
use heap::HeapAtom;
use store::atom;
use tags::{Tag, TaggedValue, MAX_INLINE_LEN};

use alloc::string::String;

pub(crate) const ALIGNMENT: usize = 8;

pub use store::AtomStore;

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
            atom(s)
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

    pub(crate) fn new_inline_impl(s: &str) -> Self {
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

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
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

    #[must_use]
    unsafe fn alias(&self) -> Self {
        debug_assert!(self.is_heap());
        let heap_atom = HeapAtom::deref_from(self.inner);
        Arc::increment_strong_count(heap_atom as *const _);

        Self {
            inner: self.inner,
            marker: PhantomData,
        }
    }
}

impl<'a> Clone for Atom<'a> {
    fn clone(&self) -> Self {
        match self.inner.tag() {
            Tag::HeapOwned => unsafe { self.alias() },
            Tag::Inline => Self {
                inner: self.inner,
                marker: PhantomData,
            },
            Tag::Static => {
                panic!("todo: Atom#clone() for Tag::Static")
            }
        }
    }
}

impl From<&str> for Atom<'static> {
    fn from(value: &str) -> Self {
        atom(value)
    }
}
impl From<&&str> for Atom<'static> {
    fn from(value: &&str) -> Self {
        atom(value)
    }
}
impl From<String> for Atom<'static> {
    fn from(value: String) -> Self {
        atom(&value)
    }
}
impl From<&String> for Atom<'static> {
    fn from(value: &String) -> Self {
        atom(value.as_ref())
    }
}
impl From<Cow<'_, str>> for Atom<'static> {
    fn from(value: Cow<'_, str>) -> Self {
        atom(&value)
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

        if self.is_heap() && other.is_heap() {
            let self_heap = unsafe { HeapAtom::deref_from(self.inner) };
            let other_heap = unsafe { HeapAtom::deref_from(other.inner) };
            // If the store is the same, the same string has same `unsafe_data``
            match (&self_heap.store_id(), &other_heap.store_id()) {
                (Some(this_store), Some(other_store)) => {
                    if this_store == other_store {
                        return false;
                    }
                }
                (None, None) => {
                    return false;
                }
                _ => {}
            }
        }

        self.as_str() == self.as_str()
    }
}

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

impl Drop for Atom<'_> {
    fn drop(&mut self) {
        if self.is_heap() {
            let heap_atom = unsafe { HeapAtom::restore_arc(self.inner) };
            drop(heap_atom);
        }
    }
}
