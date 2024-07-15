#![allow(clippy::cast_possible_truncation)]

use core::{mem::transmute, num::NonZeroU8, ptr::NonNull, slice};
use std::os::raw::c_void;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[allow(dead_code)]
pub(crate) enum Tag {
    HeapOwned = 0b_00,
    Inline = 0b_01,
    Static = 0b_10,
}

impl Tag {
    #[inline(always)]
    #[must_use]
    pub const unsafe fn new_unchecked(value: u8) -> Self {
        debug_assert!(value < 0b_11);
        core::mem::transmute(value)
    }
    pub const TAG_MASK: u8 = 0b_11;
    pub const MASK_USIZE: usize = Self::TAG_MASK as usize;
    pub const INLINE_NONZERO: NonZeroU8 = unsafe { NonZeroU8::new_unchecked(Self::Inline as u8) };
    pub const INLINE_LEN_OFFSET: u8 = 4;

    #[inline(always)]
    pub const fn is_heap_owned(self) -> bool {
        matches!(self, Self::HeapOwned)
    }

    #[inline(always)]
    pub const fn is_inline(self) -> bool {
        matches!(self, Self::Inline)
    }
}
/*
## Base representation:

0000 0000 | 0000 0000 | 0000 0000 | 0000 0000 | 0000 0000 | 0000 0000 | 0000 0000 | 0000 00tt
    0           1           2           3           4           5           6           7

--------------------------------------------------------------------------------

## Variant type 1: Dynamic (Heap)
This is a pointer to a heap-allocated value (HeapAtom). I'd like to also support
borrows, where this becomes a pointer over stack allocations or unowned heap
objects.

Tag is 0b00

pppp pppp | pppp pppp | pppp pppp | pppp pppp | pppp pppp | pppp pppp | pppp pppp | pppp pp00
    0           1           2           3           4           5           6           7
- p: pointer

--------------------------------------------------------------------------------

## Variant type 2: Inline
An interned string that can hold (sizeof(TaggedValue) - 1) bytes for character
data (actual # of chars may be less b/c UTF-8)

Tag is 0b01

cccc cccc | cccc cccc | cccc cccc | cccc cccc | cccc cccc | cccc cccc | cccc cccc | llll 0001
    0           1           2           3           4           5           6           7
- c: character
- l: length
- t: tag

--------------------------------------------------------------------------------

## Variant type 3: Static
idfk lmfao

Tag is 0b10

0000 0000 | 0000 0000 | 0000 0000 | 0000 0000 | 0000 0000 | 0000 0000 | 0000 0000 | 0000 0010
    0           1           2           3           4           5           6           7

--------------------------------------------------------------------------------

## Variant type 4: Borrow

Pointer to a string that is not owned by the atom.

NOTE: Current HeapAtom implementaiton may be problematic for pre-computed hashes.

Tag is 0b11

pppp pppp | pppp pppp | pppp pppp | pppp pppp | pppp pppp | pppp pppp | pppp pppp | pppp pp11
    0           1           2           3           4           5           6           7
*/
#[cfg(feature = "atom_size_128")]
type RawTaggedValue = u128;
#[cfg(any(
    target_pointer_width = "32",
    target_pointer_width = "16",
    feature = "atom_size_64"
))]
type RawTaggedValue = u64;
#[cfg(not(any(
    target_pointer_width = "32",
    target_pointer_width = "16",
    feature = "atom_size_64",
    feature = "atom_size_128"
)))]
type RawTaggedValue = usize;

#[cfg(feature = "atom_size_128")]
type RawTaggedNonZeroValue = core::num::NonZeroU128;
#[cfg(any(
    target_pointer_width = "32",
    target_pointer_width = "16",
    feature = "atom_size_64"
))]
type RawTaggedNonZeroValue = core::num::NonZeroU64;
#[cfg(not(any(
    target_pointer_width = "32",
    target_pointer_width = "16",
    feature = "atom_size_64",
    feature = "atom_size_128"
)))]
type RawTaggedNonZeroValue = core::ptr::NonNull<()>;

pub(crate) const MAX_INLINE_LEN: usize = core::mem::size_of::<TaggedValue>() - 1;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct TaggedValue {
    value: RawTaggedNonZeroValue,
}
static_assertions::assert_eq_align!(TaggedValue, u64);

impl TaggedValue {
    const INLINE_DATA_LEN: usize = core::mem::size_of::<TaggedValue>() - 1;

    #[inline(always)]
    pub fn new_ptr<T: ?Sized>(value: NonNull<T>) -> Self {
        #[cfg(any(
            target_pointer_width = "32",
            target_pointer_width = "16",
            feature = "atom_size_64",
            feature = "atom_size_128"
        ))]
        unsafe {
            let value: std::num::NonZeroUsize = std::mem::transmute(value);
            Self {
                value: RawTaggedNonZeroValue::new_unchecked(value.get() as _),
            }
        }

        #[cfg(not(any(
            target_pointer_width = "32",
            target_pointer_width = "16",
            feature = "atom_size_64",
            feature = "atom_size_128"
        )))]
        {
            Self {
                value: value.cast(),
            }
        }
    }

    pub const fn new_inline(len: u8) -> Self {
        debug_assert!(len <= MAX_INLINE_LEN as u8);
        // let value = Tag::INLINE_NONZERO | len << (Tag::INLINE_LEN_OFFSET as NonZeroU8)
        let tag_byte = unsafe {
            NonZeroU8::new_unchecked(Tag::INLINE_NONZERO.get() | (len << Tag::INLINE_LEN_OFFSET))
        };
        let value = tag_byte.get() as RawTaggedValue;
        Self {
            value: unsafe { core::mem::transmute(value) },
        }
    }

    #[inline(always)]
    pub const fn get_ptr(self) -> *const c_void {
        #[cfg(any(
            target_pointer_width = "32",
            target_pointer_width = "16",
            feature = "atom_size_64",
            feature = "atom_size_128"
        ))]
        {
            self.value.get() as usize as _
        }
        #[cfg(not(any(
            target_pointer_width = "32",
            target_pointer_width = "16",
            feature = "atom_size_64",
            feature = "atom_size_128"
        )))]
        unsafe {
            transmute(Some(self.value))
        }
    }

    #[inline(always)]
    pub const fn hash(self) -> u64 {
        self.get_value() as u64
    }

    #[inline(always)]
    pub(crate) const fn tag_byte(self) -> u8 {
        (self.get_value() & 0xff) as u8
    }

    #[inline(always)]
    pub(crate) const fn tag(self) -> Tag {
        // NOTE: Dony does this, but tag mask is 0x03?
        // (self.get_value() & 0xff) as u8
        unsafe { Tag::new_unchecked((self.get_value() & Tag::MASK_USIZE) as u8) }
    }

    pub(crate) const fn len(self) -> usize {
        debug_assert!(self.tag().is_inline());

        (self.tag_byte() >> Tag::INLINE_LEN_OFFSET) as usize
    }

    #[inline(always)]
    const fn get_value(self) -> RawTaggedValue {
        unsafe { core::mem::transmute(Some(self.value)) }
    }

    /// Get a slice to the data inlined in this [`TaggedValue`]
    pub fn as_bytes(&self) -> &[u8] {
        // debug_assert_eq!(self.tag(), Tag::Inline);
        debug_assert!(self.tag().is_inline());

        let x: *const _ = &self.value;
        let mut data = x.cast::<u8>();
        // All except the lowest byte, which is first in little-endian, last in
        // big-endian. That's where we store the tag.
        if cfg!(target_endian = "little") {
            unsafe {
                data = data.offset(1);
            }
        }
        unsafe { slice::from_raw_parts(data, Self::INLINE_DATA_LEN) }
    }

    /// Get a mutable slice to the data inlined in this [`TaggedValue`]
    pub unsafe fn as_bytes_mut(&mut self) -> &mut [u8] {
        debug_assert!(self.tag().is_inline());

        let x: *mut _ = &mut self.value;
        let mut data = x.cast::<u8>();
        // All except the lowest byte, which is first in little-endian, last in
        // big-endian. That's where we store the tag.
        if cfg!(target_endian = "little") {
            unsafe {
                data = data.offset(1);
            }
        }
        slice::from_raw_parts_mut(data, Self::INLINE_DATA_LEN)
    }
}
