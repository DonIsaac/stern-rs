// use core::{hash::{BuildHasher, BuildHasherDefault}, num::NonZeroU32,
// ptr::NonNull};

extern crate alloc;

use core::alloc::Layout;
use core::hash::{Hash, Hasher};
use core::mem::{size_of, transmute};
use core::num::NonZeroU32;
use core::ptr::{self};
use core::slice;
use rustc_hash::FxHasher;

// use alloc::alloc;

use crate::ptr::ReadonlyNonNull;
use crate::ALIGNMENT;

// pub struct HeapAtom {
//     ptr: NonNull<u8>,
//     len: u32,
//     hash: u32,
//     store_id: NonZeroU32,
// }

// Here's what Dony does:
#[derive(Debug)]
pub struct Entry {
    string: Box<str>,
    store_id: NonZeroU32,
    hash: u64,
}

#[derive(Debug)]
#[repr(C)]
pub struct Header {
    pub(crate) len: u32,
    pub(crate) store_id: NonZeroU32,
    pub(crate) hash: u64,
}
const OVERHEAD_SIZE: usize = size_of::<Header>();
static_assertions::const_assert!(OVERHEAD_SIZE == 16);

#[repr(C)]
#[derive(Debug)]
pub(crate) struct HeapAtom {
    pub(crate) header: Header,
    // TODO: pad with 4 bytes?
    pub(crate) string: str,
}

#[repr(C)]
struct Generic<T: ?Sized> {
    hash: u64,
    store_id: NonZeroU32,
    len: u32,
    string: T,
}

// type R = *mut HeapAtom;
// type P = NonNull<HeapAtom>;
// type B = Box<HeapAtom>;
// type Q = core::ptr::Unique<HeapAtom>;
impl HeapAtom {
    pub fn new(s: &str) -> ReadonlyNonNull<Self> {
        assert!(s.len() <= u32::MAX as usize, "string is too long");
        if s.is_empty() {
            return Self::zero_sized();
        }

        unsafe { Self::new_unchecked(s) }
    }

    pub fn new_unchecked(s: &str) -> ReadonlyNonNull<HeapAtom> {
        let header = Header {
            len: s.len() as u32,                   // length of the string, in bytes
            store_id: NonZeroU32::new(1).unwrap(), // TODO
            hash: str_hash(s),                     // pre-computed hash
        };

        let size = size_of::<Header>() + s.len(); // # of bytes to allocate
        let layout = Self::get_layout(s.len());

        // SAFETY:
        // - Layout will never be zero-sized because OVERHEAD_SIZE is 16
        let ptr: *mut u8 = unsafe { alloc::alloc::alloc(layout) };
        assert!(!ptr.is_null(), "OOM:alloc returned null");
        assert!(ptr as usize % 8 == 0, "not 8-byte aligned");

        // write the data to the heap
        unsafe {
            ptr::copy_nonoverlapping(&header, ptr as *mut Header, 1);
            let string_ptr = ptr.add(size_of::<Header>());
            ptr::copy_nonoverlapping(s.as_ptr(), string_ptr, s.len());
        }

        // TODO: should we use Box semantics or NonNull semantics?
        // fat pointer to dynamically-sized type (DST)
        let fat_dst: ReadonlyNonNull<HeapAtom> = unsafe {
            let slice: &mut [u8] = slice::from_raw_parts_mut(ptr, size);
            ReadonlyNonNull::new_unchecked(slice as *mut [u8] as *mut HeapAtom)
        };

        fat_dst
    }

    fn zero_sized() -> ReadonlyNonNull<Self> {
        const EMPTY: Generic<[u8; 0]> = Generic {
            hash: 0,
            store_id: unsafe { NonZeroU32::new_unchecked(1) },
            len: 0,
            string: [],
        };

        unsafe { transmute(ReadonlyNonNull::new_unchecked(&EMPTY as &Generic<[u8]>)) }
    }

    #[inline(always)]
    pub fn hash(&self) -> u64 {
        self.header.hash
    }

    pub fn as_str(&self) -> &str {
        unsafe {
            let ptr = self.str_ptr();
            core::str::from_utf8_unchecked(slice::from_raw_parts(ptr, self.header.len as usize))
        }
    }

    fn get_layout(strlen: usize) -> Layout {
        debug_assert!(
            strlen <= u32::MAX as usize,
            "Strings longer than 2^32 are not supported"
        );

        #[cfg(target_pointer_width = "32")]
        {
            assert!(strlen.next_multiple_of(ALIGNMENT) <= isize::MAX);
        }

        // SAFETY:
        // 1. alignment is always non-zero b/c its a constant value of 8
        // 2. alignment is always a power of 2 b/c its a constant value of 8
        // 3. on 64bit machines, isize::MAX is always greater than u32::MAX. On
        //    32bit machines, the above assertion guarantees this invariant.
        unsafe { Layout::from_size_align_unchecked(size_of::<Header>() + strlen, ALIGNMENT) }
            .pad_to_align()
    }

    unsafe fn str_ptr(&self) -> *const u8 {
        (self as *const _ as *const u8).add(size_of::<Header>())
    }
}

impl Hash for HeapAtom {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.header.hash);
    }
}

impl PartialEq for HeapAtom {
    fn eq(&self, other: &Self) -> bool {
        self.header.hash == other.header.hash && self.as_str() == other.as_str()
    }
}
impl Eq for HeapAtom {}

fn str_hash(s: &str) -> u64 {
    let mut hasher = FxHasher::default();
    s.hash(&mut hasher);
    hasher.finish()
}
