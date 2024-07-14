extern crate alloc;

use core::alloc::Layout;
use core::hash::{Hash, Hasher};
use core::mem::{size_of, transmute};
use core::num::NonZeroU32;
use core::ptr::{self};
use core::sync::atomic;
use core::{fmt, slice};
use rustc_hash::FxHasher;

use crate::ptr::ReadonlyNonNull;
use crate::tags::{Tag, TaggedValue};
use crate::ALIGNMENT;

#[derive(Debug)]
#[repr(C)]
pub struct Header {
    /// Length of the string
    pub(crate) len: u32,
    pub(crate) store_id: Option<NonZeroU32>,
    /// Pre-computed hash
    pub(crate) hash: u64,
}
static_assertions::const_assert!(size_of::<Header>() == 16);
static_assertions::assert_eq_align!(Header, u64);

impl Header {
    unsafe fn new_unchecked(s: &str, store_id: Option<NonZeroU32>) -> Self {
        assert_unchecked!(s.len() < u32::MAX as usize, "string's length overflows u32");
        Self {
            len: s.len() as u32,
            store_id,
            hash: str_hash(s),
        }
    }
}
impl Default for Header {
    fn default() -> Self {
        Self {
            len: 0,
            store_id: None,
            hash: str_hash(""),
        }
    }
}

// type Header = SneakyArcInner<StringMeta>;

#[repr(C)]
#[derive(Debug)]
pub(crate) struct HeapAtom {
    pub(crate) header: Header,
    // TODO: pad with 4 bytes?
    pub(crate) string: str,
}

#[repr(C)]
#[derive(Debug)]
struct Generic<T: ?Sized> {
    header: Header,
    string: T,
}

#[repr(C)]
struct SneakyArcInner<T: ?Sized> {
    strong: atomic::AtomicUsize,

    // the value usize::MAX acts as a sentinel for temporarily "locking" the
    // ability to upgrade weak pointers or downgrade strong ones; this is used
    // to avoid races in `make_mut` and `get_mut`.
    weak: atomic::AtomicUsize,

    data: T,
}
impl<T: ?Sized + fmt::Debug> fmt::Debug for SneakyArcInner<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SneakyArcInner")
            .field("strong", &self.strong)
            .field("weak", &self.weak)
            .field("data", &&self.data)
            .finish()
    }
}

impl HeapAtom {
    #[must_use]
    pub fn new(s: &str) -> ReadonlyNonNull<HeapAtom> {
        assert!(s.len() <= u32::MAX as usize, "string is too long");
        if s.is_empty() {
            return unsafe { Self::zero_sized() };
        }

        unsafe { Self::try_new_unchecked(s) }.unwrap()
    }

    pub unsafe fn try_new_unchecked(s: &str) -> Result<ReadonlyNonNull<HeapAtom>, &'static str> {
        assert_unchecked!(s.len() < u32::MAX as usize);
        let header = Header::new_unchecked(s, None);

        // let size = size_of::<Header>() + s.len(); // # of bytes to allocate
        let layout = Self::get_layout(header.len);
        debug_assert_eq!(layout.align(), 8);
        debug_assert!(layout.size() > 0); // should never happen

        // SAFETY:
        // - Layout will never be zero-sized because Header's size is non-zero
        let ptr: *mut u8 = unsafe { alloc::alloc::alloc(layout) };
        if ptr.is_null() {
            return Err("OOM: HeapAtom allocation returned null");
        }
        debug_assert!(ptr as usize % 8 == 0, "not 8-byte aligned");

        // write the data to the heap
        unsafe {
            ptr::copy_nonoverlapping(&header, ptr as *mut Header, 1);
            let string_ptr = ptr.add(size_of::<Header>());
            ptr::copy_nonoverlapping(s.as_ptr(), string_ptr, s.len());
        }

        // TODO: should we use Box semantics or NonNull semantics?
        // fat pointer to dynamically-sized type (DST)
        let fat_ptr: ReadonlyNonNull<HeapAtom> = unsafe {
            let slice: &mut [u8] = slice::from_raw_parts_mut(ptr, layout.size());
            ReadonlyNonNull::new_unchecked(slice as *mut [u8] as *mut HeapAtom)
        };

        Ok(fat_ptr)
    }

    #[must_use]
    unsafe fn zero_sized() -> ReadonlyNonNull<HeapAtom> {
        let empty: Generic<[u8; 0]> = Generic {
            header: Header::default(),
            string: [],
        };

        let fat_ptr = &empty as &Generic<[u8]>;
        ReadonlyNonNull::new_unchecked(transmute::<_, &HeapAtom>(fat_ptr) as *const _)
    }

    pub const unsafe fn deref_from<'a>(tagged_ptr: TaggedValue) -> &'a HeapAtom {
        // &*(tagged_ptr.get_ptr() as *const _)
        debug_assert!(!matches!(tagged_ptr.tag(),Tag::HeapOwned), "cannot deref a non heap-owned tagged value");

        let len: u32 = ptr::read(tagged_ptr.get_ptr().cast());
        let fat_ptr = slice::from_raw_parts(tagged_ptr.get_ptr(), Self::sizeof(len));
        transmute::<_, &'a HeapAtom>(fat_ptr)
    }

    // unsafe fn thin_dst(tagged_ptr: TaggedValue) -> *const HeapAtom {
    //     debug_assert!(tagged_ptr.tag() == Tag::HeapOwned, "cannot deref a non heap-owned tagged value");
    //     debug_assert!(!tagged_ptr.get_ptr().is_null());

    //     let len: u32 = ptr::read(tagged_ptr.get_ptr().cast());
    //     let fat_ptr = slice::from_raw_parts(tagged_ptr.get_ptr(), Self::sizeof(len));

    //     transmute::<_, &HeapAtom>(fat_ptr.as_ref()) as *const _
    // }

    #[inline]
    pub const fn len(&self) -> usize {
        self.header.len as usize
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.header.len == 0
    }

    #[inline(always)]
    pub const fn hash(&self) -> u64 {
        self.header.hash
    }

    pub const fn as_str(&self) -> &str {
        unsafe {
            let ptr = self.str_ptr();
            core::str::from_utf8_unchecked(slice::from_raw_parts(ptr, self.header.len as usize))
        }
    }

    #[must_use]
    const fn get_layout(strlen: u32) -> Layout {

        // TODO: use pad_to_align(). See rust issue https://github.com/rust-lang/rust/issues/67521
        let size_used = size_of::<Header>() + strlen as usize;
        let size = size_used.next_multiple_of(ALIGNMENT);

        debug_assert!(size % ALIGNMENT == 0, "While getting HeapAtom layout, computed a size that was not 8-byte aligned");
        #[cfg(target_pointer_width = "32")]
        {
            assert!(size <= isize::MAX);
        }

        // SAFETY:
        // 1. alignment is always non-zero b/c its a constant value of 8
        // 2. alignment is always a power of 2 b/c its a constant value of 8
        // 3. on 64bit machines, isize::MAX is always greater than u32::MAX. On
        //    32bit machines, the above assertion guarantees this invariant.
        unsafe { Layout::from_size_align_unchecked(size_of::<Header>() + strlen as usize, ALIGNMENT) }
    }

    #[inline(always)]
    const fn sizeof(strlen: u32) -> usize {
        Self::get_layout(strlen).size()
    }

    #[inline(always)]
    const fn alloc_len(&self) -> usize {
        Self::sizeof(self.header.len)
    }

    // const fn len_offset() -> usize {
    //     Layout::from_size_align_unchecked(size_of::<Header>, ALIGNMENT)
    // }

    const unsafe fn str_ptr(&self) -> *const u8 {
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_empty() {
        let empty = HeapAtom::new("");
        let atom = unsafe { empty.as_ref() };
        assert_eq!(atom.len(), 0);
        assert!(atom.is_empty());
        assert_eq!(atom.as_str(), "");

        let empty2 = HeapAtom::new("");
        let atom2 = unsafe { empty2.as_ref() };
        assert_eq!(atom2.as_str(), "");
        assert_eq!(atom, atom2);

        // FIXME: causing sigsev
        // assert_eq!(atom.as_str(), atom2.as_str());
        // assert!(
        //         !ptr::addr_eq(empty.as_ptr(), empty2.as_ptr())
        // );
    }

    #[test]
    fn test_smol() {
        let foo_ptr = HeapAtom::new("foo");
        let foo = unsafe { foo_ptr.as_ref() };
        assert_eq!(foo.len(), 3);
        assert_eq!(foo.as_str(), "foo");
        assert_eq!(foo, foo);
    }
}
