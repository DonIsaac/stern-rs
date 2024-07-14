extern crate alloc;

use core::hash::{BuildHasher, BuildHasherDefault, Hasher};
use core::num::NonZeroU32;
use core::sync::atomic::{AtomicU32, Ordering}
use std::borrow::Cow;
use alloc::sync::{Arc};
use alloc::borrow::Borrow;
use hashbrown::HashTable;
use rustc_hash::FxBuildHasher;

use crate::heap::HeapAtom;
use crate::Atom;


pub struct AtomStore {
    pub(crate) id: Option<NonZeroU32>,
    pub(crate) data: hashbrown::HashMap<Arc<HeapAtom>, (), BuildEntryHasher>,
}

impl Default for AtomStore {
    fn default() -> Self {
        static ATOM_STORE_ID: AtomicU32 = AtomicU32::new(1);
        const STORE_CAPACITY: usize = 64;

        Self {
            id: Some(unsafe { NonZeroU32::new_unchecked(ATOM_STORE_ID.fetch_add(1, Ordering::SeqCst)) }),
            data: hashbrown::HashMap::with_capacity_and_hasher(STORE_CAPACITY, Default::default()),
        }
    }
}

impl AtomStore {
    #[inline(always)]
    pub fn atom<'a>(&mut self, text: impl Into<Cow<'a, str>>) -> Atom {
        new_atom(self, text.into())
    }

    #[inline(never)]
    fn insert_entry<'s>(&'s mut self, text: Cow<str>, hash: u64) -> Arc<Atom<'s>> {
        let store_id = self.id;
        let (entry, _) = self
            .data
            .raw_entry_mut()
            .from_hash(hash, |key| key.hash == hash && *key.string == *text)
            .or_insert_with(move || {
                (
                    Arc::new(Entry {
                        string: text.into_owned().into_boxed_str(),
                        hash,
                        store_id,
                    }),
                    (),
                )
            });
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
