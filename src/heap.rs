#![allow(clippy::cast_ptr_alignment)]
extern crate alloc;

use core::alloc::Layout;
use core::hash::{Hash, Hasher};
use core::mem::{size_of, transmute};
use core::num::NonZeroU32;
use core::ptr::{self};
use core::sync::atomic;
use core::{fmt, slice};
use rustc_hash::FxHasher;

use alloc::sync::Arc;

use alloc::boxed::Box;

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

        #[allow(clippy::cast_possible_truncation)]
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

#[repr(C)]
#[derive(Debug)]
pub(crate) struct HeapAtom {
    pub(crate) header: Header,
    pub(crate) string: str,
}

/// This has the same layout and representation as [`HeapAtom`], but can be
/// constructed directly. Especially useful for sized slices.
///
/// The only use for this is to construct a [`HeapAtom`]. It gets casted
/// immediately after construction.
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

impl<T: ?Sized> SneakyArcInner<T> {
    #[inline(always)]
    #[must_use]
    const unsafe fn into_data_ptr(ptr: *const Self) -> *const T {
        ptr.byte_add(size_of::<SneakyArcInner<()>>()) as *const _
    }

    #[inline(always)]
    #[must_use]
    const unsafe fn into_data_ptr_mut(ptr: *mut Self) -> *mut T {
        ptr.byte_add(size_of::<SneakyArcInner<()>>()) as *mut _
    }
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
    pub fn new(s: &str, store_id: Option<NonZeroU32>) -> Arc<HeapAtom> {
        assert!(u32::try_from(s.len()).is_ok(), "string is too long");
        if s.is_empty() {
            return unsafe { Self::zero_sized() };
        }

        unsafe { Self::try_new_unchecked(s, store_id) }.unwrap()
    }

    #[inline(never)]
    pub unsafe fn try_new_unchecked(
        s: &str,
        store_id: Option<NonZeroU32>,
    ) -> Result<Arc<HeapAtom>, &'static str> {
        assert_unchecked!(s.len() < u32::MAX as usize);
        let header = Header::new_unchecked(s, store_id);

        let layout = Self::get_layout(header.len);
        debug_assert_eq!(layout.align(), 8);
        debug_assert!(layout.size() > 0); // should never happen

        // SAFETY:
        // - Layout will never be zero-sized because Header's size is non-zero
        let ptr: *mut u8 = unsafe { alloc::alloc::alloc(layout) };
        if ptr.is_null() {
            return Err("OOM: HeapAtom allocation returned null");
        }
        debug_assert!(
            ptr as usize % 8 == 0,
            "pointer for new HeapAtom is not 8-byte aligned"
        );

        let arc_inner: SneakyArcInner<()> = SneakyArcInner {
            strong: atomic::AtomicUsize::new(1),
            weak: atomic::AtomicUsize::new(1),
            data: (),
        };

        // write the data to the heap
        unsafe {
            // ArcInner
            ptr::copy_nonoverlapping(&arc_inner, ptr as *mut SneakyArcInner<()>, 1);
            // Header
            let header_ptr = ptr.byte_add(size_of::<SneakyArcInner<()>>()) as *mut Header;
            ptr::copy_nonoverlapping(&header, header_ptr, 1);
            // HeapAtom
            let string_ptr = header_ptr.byte_add(size_of::<Header>()) as *mut u8;
            ptr::copy_nonoverlapping(s.as_ptr(), string_ptr, s.len());
        }

        // TODO: should we use Box semantics or NonNull semantics?
        // fat pointer to dynamically-sized type (DST)
        let fat_ptr: Arc<HeapAtom> = unsafe {
            let slice: &mut [u8] = slice::from_raw_parts_mut(ptr, layout.size());
            let fat_raw = slice as *mut [u8] as *mut SneakyArcInner<HeapAtom>;
            // let fat_atom: *mut HeapAtom =
            // fat_raw.byte_add(Self::data_offset()) as *mut _;
            let fat_atom = SneakyArcInner::into_data_ptr_mut(fat_raw);
            Arc::from_raw(fat_atom)
        };

        // ensure layout integrity
        debug_assert_eq!(fat_ptr.len(), s.len());
        debug_assert_eq!(fat_ptr.as_str(), s);

        Ok(fat_ptr)
    }

    // FIXME: I don't think we actually need this function b/c zero-sized
    // strings get inlined
    #[must_use]
    unsafe fn zero_sized() -> Arc<HeapAtom> {
        let empty: Generic<[u8; 0]> = Generic {
            header: Header::default(),
            string: [],
        };

        // must be put on the heap b/c Arc expects to own its own heap
        // allocation and will free() it. If it's on the stack, we'll get a SIGSEGV
        let raw_ptr: *mut SneakyArcInner<Generic<[u8; 0]>> = Box::leak(Box::new(SneakyArcInner {
            strong: atomic::AtomicUsize::new(1),
            weak: atomic::AtomicUsize::new(1),
            data: empty,
        })) as *mut _;
        // get pointer to our string struct. Arc::from_raw will find strong/weak
        // by subtracting from the pointer passed to it, so we need to
        // compensate by adding the same offset. This only works if, among other
        // things, the pointer offset is 8.
        let atom_ptr = unsafe { SneakyArcInner::into_data_ptr(raw_ptr) };

        // ensure layout is correct
        #[cfg(debug_assertions)]
        unsafe {
            let atom = atom_ptr.as_ref().unwrap();
            assert_eq!(atom.header.len, 0);
            assert_eq!(atom.header.store_id, None);
            assert_eq!(atom.string.as_ref(), "".as_bytes());
        }

        let raw = unsafe { Arc::from_raw(atom_ptr) };
        let fat = raw as Arc<Generic<[u8]>>;

        // cast Generic into a HeapAtom and ensure layout is consistent after
        // Arc::from_raw
        let arc: Arc<HeapAtom> = unsafe { transmute(fat) };
        debug_assert_eq!(arc.len(), 0);
        debug_assert_eq!(arc.as_str(), "");

        arc
    }

    #[must_use]
    pub const unsafe fn deref_from<'a>(tagged_ptr: TaggedValue) -> &'a HeapAtom {
        debug_assert!(
            matches!(tagged_ptr.tag(), Tag::HeapOwned),
            "cannot deref a non heap-owned tagged value"
        );

        let len: u32 = ptr::read(tagged_ptr.get_ptr().cast());
        let fat_ptr = slice::from_raw_parts(tagged_ptr.get_ptr(), Self::sizeof(len));
        transmute::<_, &'a HeapAtom>(fat_ptr)
    }

    #[must_use]
    pub unsafe fn restore_arc(tagged_ptr: TaggedValue) -> Arc<HeapAtom> {
        let raw_ref = Self::deref_from(tagged_ptr);
        Arc::from_raw(raw_ref as *const HeapAtom)
    }

    #[inline]
    pub const fn store_id(&self) -> Option<NonZeroU32> {
        self.header.store_id
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.header.len as usize
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
        let size_used = size_of::<SneakyArcInner<()>>() + size_of::<Header>() + strlen as usize;
        let size = size_used.next_multiple_of(ALIGNMENT);

        debug_assert!(
            size % ALIGNMENT == 0,
            "While getting HeapAtom layout, computed a size that was not 8-byte aligned"
        );
        #[cfg(target_pointer_width = "32")]
        {
            assert!(size <= isize::MAX);
        }

        // SAFETY:
        // 1. alignment is always non-zero b/c its a constant value of 8
        // 2. alignment is always a power of 2 b/c its a constant value of 8
        // 3. on 64bit machines, isize::MAX is always greater than u32::MAX. On
        //    32bit machines, the above assertion guarantees this invariant.
        unsafe { Layout::from_size_align_unchecked(size, ALIGNMENT) }
    }

    #[inline(always)]
    const fn sizeof(strlen: u32) -> usize {
        Self::get_layout(strlen).size()
    }

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

pub(crate) fn str_hash(s: &str) -> u64 {
    let mut hasher = FxHasher::default();
    s.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_empty() {
        let atom = HeapAtom::new("", None);
        assert_eq!(atom.len(), 0);
        assert_eq!(atom.as_str(), "");

        let atom2 = HeapAtom::new("", None);
        assert_eq!(atom2.as_str(), "");
        assert_eq!(atom, atom2);
        assert_eq!(atom.as_str(), atom2.as_str());

        assert_eq!(atom.as_str(), atom2.as_str());
        assert!(!ptr::addr_eq(
            atom.as_ref() as *const _,
            atom2.as_ref() as *const _
        ));
    }

    #[test]
    fn test_smol() {
        let foo = HeapAtom::new("foo", None);
        assert_eq!(foo.len(), 3);
        assert_eq!(foo.as_str(), "foo");
        assert_eq!(foo, foo);
    }
}
