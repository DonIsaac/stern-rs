extern crate alloc;

use alloc::sync::Arc;
use core::cell::RefCell;
use core::hash::{BuildHasherDefault, Hasher};
use core::marker::PhantomData;
use core::num::NonZeroU32;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::heap::{str_hash, HeapAtom};
use crate::tags::{Tag, TaggedValue, MAX_INLINE_LEN};
use crate::Atom;

pub(crate) fn atom(text: &str) -> Atom<'static> {
    thread_local! {
        static GLOBAL_DATA: RefCell<AtomStore> = Default::default();
    }

    GLOBAL_DATA.with(|global| {
        let mut store = global.borrow_mut();

        store.atom(text)
    })
}

pub struct AtomStore {
    pub(crate) id: Option<NonZeroU32>,
    pub(crate) data: hashbrown::HashMap<Arc<HeapAtom>, (), BuildEntryHasher>,
}

impl Default for AtomStore {
    fn default() -> Self {
        static ATOM_STORE_ID: AtomicU32 = AtomicU32::new(1);
        const STORE_CAPACITY: usize = 256;

        Self {
            id: Some(unsafe {
                NonZeroU32::new_unchecked(ATOM_STORE_ID.fetch_add(1, Ordering::SeqCst))
            }),
            data: hashbrown::HashMap::with_capacity_and_hasher(STORE_CAPACITY, Default::default()),
        }
    }
}

impl AtomStore {
    pub fn atom<S: AsRef<str>>(&mut self, s: S) -> Atom<'static> {
        let s = s.as_ref();
        if s.len() <= MAX_INLINE_LEN {
            return Atom::new_inline_impl(s);
        }
        let hash = str_hash(s);
        let entry = self.insert_entry(s, hash);
        let entry = Arc::into_raw(entry);

        // Safety: Arc::into_raw returns a non-null pointer
        let ptr: NonNull<HeapAtom> = unsafe { NonNull::new_unchecked(entry as *mut HeapAtom) };
        debug_assert!(0 == (ptr.as_ptr() as *const u8 as usize) & Tag::MASK_USIZE);
        Atom {
            inner: TaggedValue::new_ptr(ptr),
            marker: PhantomData,
        }
    }

    #[inline(never)]
    fn insert_entry(&mut self, text: &str, hash: u64) -> Arc<HeapAtom> {
        let store_id = self.id;
        let (entry, _) = self
            .data
            .raw_entry_mut()
            .from_hash(hash, |key| key.hash() == hash && key.as_str() == text)
            .or_insert_with(move || (HeapAtom::new(text, store_id), ()));

        entry.clone()
    }
}

type BuildEntryHasher = BuildHasherDefault<EntryHasher>;

/// A "no-op" hasher for [Entry] that returns [Entry::hash]. The design is
/// inspired by the `nohash-hasher` crate.
///
/// Assumption: [Arc]'s implementation of [Hash] is a simple pass-through.
#[derive(Default)]
pub(crate) struct EntryHasher {
    hash: u64,
    #[cfg(debug_assertions)]
    write_called: bool,
}

impl Hasher for EntryHasher {
    fn finish(&self) -> u64 {
        #[cfg(debug_assertions)]
        debug_assert!(
            self.write_called,
            "EntryHasher expects write_u64 to have been called",
        );
        self.hash
    }

    fn write(&mut self, _bytes: &[u8]) {
        panic!("EntryHasher expects to be called with write_u64");
    }

    fn write_u64(&mut self, val: u64) {
        #[cfg(debug_assertions)]
        {
            debug_assert!(
                !self.write_called,
                "EntryHasher expects write_u64 to be called only once",
            );
            self.write_called = true;
        }

        self.hash = val;
    }
}
