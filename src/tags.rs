#![no_std]

use core::{num::NonZeroU8, ptr::NonNull, slice};
use assert_unchecked::assert_unchecked;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Tag {
    Owned = 0b_00,
    Inline = 0b_01,
    Static = 0b_10,
}

impl Tag {
    #[inline(always)]
    pub unsafe fn new_unchecked(value: u8) -> Self {
        assert_unchecked!(value < 0b_11);
        core::mem::transmute(value)
    }
    pub const MASK: u8 = 0b_11;
    pub const MASK_USIZE: usize = Self::MASK as usize;
    pub const INLINE_NONZERO: NonZeroU8 = unsafe { NonZeroU8::new_unchecked(Self::Inline as u8) };
    pub const INLINE_LEN_OFFSET: u8 = 4;
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

    /// Do not use
    pub unsafe fn dangling() -> Self {
        TaggedValue {
            value: RawTaggedNonZeroValue::dangling()
        }
    }

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

    pub fn new_inline(len: u8) -> Self {
        debug_assert!(len <= MAX_INLINE_LEN as u8);
        // let value = Tag::INLINE_NONZERO | len << (Tag::INLINE_LEN_OFFSET as NonZeroU8)
        let tag_byte = Tag::INLINE_NONZERO | ((len as u8) << Tag::INLINE_LEN_OFFSET);
        let value = tag_byte.get() as RawTaggedValue;
        Self {
            value: unsafe { core::mem::transmute(value) }
        }
    }
    // unsafe fn new_tag_unchecked(value: &[u8]) -> Self {
    //     let len = value.len();
    //     debug_assert!(len <= MAX_INLINE_LEN);
    //     // let tag = INLINE_TAG_INIT | ((len as u8) << LEN_OFFSET);

    //     let tag_byte = Tag::INLINE_NONZERO | ((len as u8) << Tag::INLINE_LEN_OFFSET);
    //     let raw_value = 
    // }

    #[inline(always)]
    pub(crate) fn tag(&self) -> Tag {
        // NOTE: Dony does this, but tag mask is 0x03?
        // (self.get_value() & 0xff) as u8
        unsafe {
            Tag::new_unchecked((self.get_value() & Tag::MASK_USIZE) as u8)
        }
    }

    #[inline(always)]
    fn get_value(&self) -> RawTaggedValue {
        unsafe { core::mem::transmute(Some(self.value)) }
    } 

    pub fn data(&self) -> &[u8] {
        debug_assert_eq!(self.tag(), Tag::Inline);

        let x: *const _ = &self.value;
        let mut data = x as *const u8;
        // All except the lowest byte, which is first in little-endian, last in
        // big-endian. That's where we store the tag.
        if cfg!(target_endian = "little") {
            unsafe {
                data = data.offset(1);
            }
        }
        unsafe { slice::from_raw_parts(data, Self::INLINE_DATA_LEN) }
    }

    pub unsafe fn data_mut(&mut self) -> &mut [u8] {
        debug_assert_eq!(self.tag(), Tag::Inline);

        let x: *mut _ = &mut self.value;
        let mut data = x as *mut u8;
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
