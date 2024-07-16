#![allow(clippy::cast_ptr_alignment)]
extern crate alloc;

use core::alloc::Layout;
use core::hash::{Hash, Hasher};
use core::mem::{size_of, transmute};
use core::num::NonZeroU32;
use core::ptr::{self};
use core::sync::atomic;
use core::{fmt, slice};
use std::mem::MaybeUninit;
use std::ptr::NonNull;
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
        let data_ptr = ptr.byte_add(ARC_OVERHEAD) as *const T;
        // debug_assert!(data_ptr.addr() % ALIGNMENT == 0);
        // debug_assert_ne!(slice::from_raw_parts_mut(data, len)).len(), (ptr.as_ref as &[u8]).len());
        data_ptr
    }

    #[inline(always)]
    #[must_use]
    const unsafe fn into_data_ptr_mut(ptr: *mut Self) -> *mut T {
        ptr.byte_add(ARC_OVERHEAD) as *mut _
    }
}
type EmptyArcInner = SneakyArcInner<()>;
const ARC_OVERHEAD: usize = size_of::<EmptyArcInner>();

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
    #[no_mangle]
    pub unsafe fn try_new_unchecked(
        s: &str,
        store_id: Option<NonZeroU32>,
    ) -> Result<Arc<HeapAtom>, &'static str> {
        assert_unchecked!(s.len() < u32::MAX as usize);
        let header = Header::new_unchecked(s, store_id);

        let layout = Self::get_layout(header.len);
        debug_assert_eq!(layout.align(), 8);
        debug_assert!(layout.size() > 0); // should never happen
        println!("layout {:?}", layout);

        // SAFETY:
        // - Layout will never be zero-sized because Header's size is non-zero
        // let ptr: *mut u8 = unsafe { alloc::alloc::alloc(layout) };
        let ptr = unsafe { alloc::alloc::alloc(layout) as *mut ()};
        if ptr.is_null() {
            return Err("OOM: HeapAtom allocation returned null");
        }
        debug_assert!(
            ptr as *const _ as usize % 8 == 0,
            "pointer for new HeapAtom is not 8-byte aligned"
        );
        // let ptr = MaybeUninit::new(NonNull::new_unchecked(ptr));

        let arc_inner: EmptyArcInner = SneakyArcInner {
            strong: atomic::AtomicUsize::new(1),
            weak: atomic::AtomicUsize::new(1),
            data: (),
        };

        // write the data to the heap
        unsafe {
            // ArcInner
            ptr::copy_nonoverlapping(&arc_inner, ptr as *mut EmptyArcInner, 1);
            // Header
            let header_ptr = ptr.byte_add(ARC_OVERHEAD) as *mut Header;
            ptr::copy_nonoverlapping(&header, header_ptr, 1);
            // HeapAtom
            let string_ptr = header_ptr.byte_add(size_of::<Header>()) as *mut u8;
            ptr::copy_nonoverlapping(s.as_ptr(), string_ptr, s.len());
        }
        // ptr.as_mut().strong = atomic::AtomicUsize::new(1);
        // ptr.write

        // TODO: should we use Box semantics or NonNull semantics?
        // fat pointer to dynamically-sized type (DST)
        let fat_ptr: Arc<HeapAtom> = unsafe {
            // let slice: &mut [usize] = slice::from_raw_parts_mut(ptr as *mut usize, layout.size() / size_of::<usize>());
            let slice: &mut [u8] = slice::from_raw_parts_mut(ptr as *mut u8, layout.size());
            // let fat
            // println!("slice: {:?}", Layout::for_value(slice));
            // let fat_raw = ptr as *mut _ as *mut SneakyArcInner<HeapAtom>;
            // println!("fat_raw ptr: {:?}", Layout::for_value(fat_raw.as_ref().unwrap()));
            // let fat_raw = slice as *mut [u8] as *mut
            // SneakyArcInner<HeapAtom>;
            let fat_raw: *mut SneakyArcInner<HeapAtom> = transmute::<_, &mut SneakyArcInner<HeapAtom>>(slice);
            println!("fat_raw ptr: {:?}", Layout::for_value(fat_raw.as_ref().unwrap()));
            let mut fat_raw = NonNull::new_unchecked(fat_raw);
            println!("fat_raw NonNull: {:?}", Layout::for_value(fat_raw.as_ref()));

            // // fat_raw's size changes after this cast. It's increased by 32
            // // bytes for some reason.
            // let casted_layout = Layout::for_value(fat_raw.as_ref());
            // println!("casted_layout: {:?}", casted_layout);
            // if layout.size() != casted_layout.size() {
            //     debug_assert!(casted_layout.size() > layout.size(), "expected: {} > {}", casted_layout.size(), layout.size());
            //     let offset_needed = casted_layout.size() - layout.size();
            //     println!("offset needed: {offset_needed}");
            //     let new = NonNull::new_unchecked(fat_raw.as_ptr().byte_sub(offset_needed));
            //     println!("fat_raw after shift: {:?}", Layout::for_value(new.as_ref()));
            //     fat_raw = new
            // }

            let fat_atom = SneakyArcInner::into_data_ptr_mut(fat_raw.as_ptr());
            // println!("fat_atom: {:?}", Layout::for_value(fat_atom.as_ref().unwrap()));
            debug_assert!(!fat_atom.is_null());

            let arc = Arc::from_raw(fat_atom);
            debug_assert!(ptr::addr_eq(arc.as_ref() as *const _, fat_atom));
            debug_assert_eq!(Layout::for_value(arc.as_ref()).size(), layout.size() - ARC_OVERHEAD);
            debug_assert_eq!(Layout::for_value(arc.as_ref()).align(), layout.align());

            arc
        };

        // ensure layout integrity
        debug_assert_eq!(Arc::strong_count(&fat_ptr), 1);
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
        debug_assert!(atom_ptr.is_aligned());

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
        const OVERHEAD: usize = ARC_OVERHEAD + size_of::<Header>();
        // TODO: use pad_to_align(). See rust issue
        // https://github.com/rust-lang/rust/issues/67521

        let size_used = OVERHEAD + (strlen as usize);
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
        assert_eq!(Arc::strong_count(&atom), 1);
        assert_eq!(Arc::weak_count(&atom), 0); // FIXME: should this be 1?

        let atom2 = HeapAtom::new("", None);
        assert_eq!(atom2.as_str(), "");
        assert_eq!(atom, atom2);
        assert_eq!(atom.as_str(), atom2.as_str());

        assert_eq!(atom.as_str(), atom2.as_str());
        assert!(!ptr::addr_eq(
            atom.as_ref() as *const _,
            atom2.as_ref() as *const _
        ));
        assert_eq!(Arc::strong_count(&atom), 1);
        assert_eq!(Arc::weak_count(&atom), 0);
    }

    #[test]
    fn test_smol() {
        // println!("usize: {}", size_of::<usize>());
        // println!("atomic usize: {}", size_of::<atomic::AtomicUsize>());
        // println!("tagged value: {}", size_of::<TaggedValue>());
        // println!("u: {}", size_of::<usize>());
        let foo = HeapAtom::new("foo", None);
        // Arc initialized through public API
        let normal_arc = Arc::new("bar");

        assert_eq!(foo.len(), 3);
        assert_eq!(foo.as_str(), "foo");
        assert_eq!(foo, foo);

        // Our SneakyArcInner hack should result in an Arc with the same
        // reference counts as if it was created normally.
        assert_eq!(Arc::strong_count(&foo), Arc::strong_count(&normal_arc));
        assert_eq!(Arc::weak_count(&foo), Arc::weak_count(&normal_arc));
    }
}
