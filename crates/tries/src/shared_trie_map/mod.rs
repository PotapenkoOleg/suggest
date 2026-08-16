//! A thread-safe map from name to [`PatriciaTrie`].
//!
//! One process often needs several independent tries - one per language, per
//! tenant, per index - reachable from every thread that serves a request.
//! [`SharedTrieMap`] is that registry: a `String`-keyed hash table whose values
//! are tries, with every method taking `&self` so a single instance can be
//! shared behind an [`Arc`] rather than cloned per thread.
//!
//! Locking is two levels deep, and that is the point. The map itself sits
//! behind one [`RwLock`]; each trie sits behind its own. Lookups hold the map
//! lock only long enough to clone an [`Arc`] out of it, so a long prefix scan
//! over one trie blocks neither the other tries nor threads registering new
//! ones.
//!
//! ```
//! use std::thread;
//! use tries::{PrefixSearch, SharedTrieMap, SymbolTable};
//!
//! let map = SharedTrieMap::<u32>::new();
//!
//! thread::scope(|scope| {
//!     for (name, word) in [("en", "sea"), ("en", "shore"), ("fr", "mer")] {
//!         let map = &map;
//!         scope.spawn(move || {
//!             let trie = map.get_or_create(name);
//!             trie.write().unwrap().put(word.to_string(), 1);
//!         });
//!     }
//! });
//!
//! assert_eq!(map.names(), ["en", "fr"]);
//!
//! let english = map.get("en").unwrap();
//! let keys = english.read().unwrap().get_keys_with_prefix("s");
//! assert_eq!(keys, ["sea", "shore"]);
//! ```
//!
//! No method hands back a lock guard - callers get [`Arc`] handles or owned
//! data - so the map can never hold a lock on a caller's behalf. Guards on the
//! tries themselves are ordinary [`RwLock`] guards: in async code, finish with
//! one before the next `.await` rather than holding it across a suspension
//! point.

use crate::patricia_trie::PatriciaTrie;
use std::collections::HashMap;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

mod tests;

/// A single trie in the map, shared between every holder of the handle.
///
/// Lock it with [`read`](RwLock::read) for queries and
/// [`write`](RwLock::write) for updates. A handle stays valid after the entry
/// is removed from the map - it simply becomes private to whoever still holds
/// it.
pub type SharedTrie<E> = Arc<RwLock<PatriciaTrie<E>>>;

/// A thread-safe hash table of named [`PatriciaTrie`]s.
///
/// See the [module documentation](self) for the locking scheme.
///
/// Every method panics if a thread panicked while holding one of the locks,
/// the standard poisoning behaviour of [`RwLock`].
pub struct SharedTrieMap<E> {
    tries: RwLock<HashMap<String, SharedTrie<E>>>,
}

impl<E> Default for SharedTrieMap<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> SharedTrieMap<E> {
    /// Creates an empty map.
    pub fn new() -> Self {
        SharedTrieMap {
            tries: RwLock::new(HashMap::new()),
        }
    }

    // The two accessors below are the only places poisoning is handled, so the
    // policy can be changed in one edit.
    fn read(&self) -> RwLockReadGuard<'_, HashMap<String, SharedTrie<E>>> {
        self.tries.read().expect("trie map lock poisoned")
    }

    fn write(&self) -> RwLockWriteGuard<'_, HashMap<String, SharedTrie<E>>> {
        self.tries.write().expect("trie map lock poisoned")
    }

    /// Returns a handle to the trie stored under `name`, or `None` if absent.
    ///
    /// # Examples
    ///
    /// ```
    /// use tries::{SharedTrieMap, SymbolTable};
    ///
    /// let map = SharedTrieMap::<u32>::new();
    /// assert!(map.get("en").is_none());
    ///
    /// map.get_or_create("en").write().unwrap().put("sea".to_string(), 2);
    /// assert_eq!(map.get("en").unwrap().read().unwrap().get("sea"), Some(2));
    /// ```
    pub fn get(&self, name: &str) -> Option<SharedTrie<E>> {
        self.read().get(name).map(Arc::clone)
    }

    /// Returns a handle to the trie stored under `name`, registering an empty
    /// one first if nothing is stored there.
    ///
    /// Concurrent callers naming the same trie all receive the same handle:
    /// exactly one of them creates it, the rest observe that creation.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use tries::SharedTrieMap;
    ///
    /// let map = SharedTrieMap::<u32>::new();
    /// assert!(Arc::ptr_eq(&map.get_or_create("en"), &map.get_or_create("en")));
    /// assert_eq!(map.len(), 1);
    /// ```
    pub fn get_or_create(&self, name: &str) -> SharedTrie<E> {
        // The common case is an existing trie, which a read lock serves without
        // excluding other readers.
        if let Some(trie) = self.get(name) {
            return trie;
        }

        // Another thread may have registered `name` between the two locks, so
        // insert through `entry` rather than unconditionally - otherwise the
        // loser of the race would replace the winner's trie, discarding writes
        // and invalidating handles already handed out.
        let mut tries = self.write();
        let trie = tries
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(PatriciaTrie::new())));
        Arc::clone(trie)
    }

    /// Stores `trie` under `name` and returns the handle it displaced, if any.
    ///
    /// Threads still holding the previous handle keep using the previous trie;
    /// only lookups made from now on see the new one.
    pub fn insert(&self, name: String, trie: PatriciaTrie<E>) -> Option<SharedTrie<E>> {
        self.write().insert(name, Arc::new(RwLock::new(trie)))
    }

    /// Removes the entry under `name`, returning its handle if there was one.
    pub fn remove(&self, name: &str) -> Option<SharedTrie<E>> {
        self.write().remove(name)
    }

    /// Returns `true` if a trie is registered under `name`.
    pub fn contains(&self, name: &str) -> bool {
        self.read().contains_key(name)
    }

    /// Returns the number of registered tries.
    pub fn len(&self) -> usize {
        self.read().len()
    }

    /// Returns `true` if no trie is registered.
    pub fn is_empty(&self) -> bool {
        self.read().is_empty()
    }

    /// Returns every registered name, in lexicographic order.
    ///
    /// The hash table has no order of its own; the names are sorted so repeated
    /// calls agree with each other.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.read().keys().cloned().collect();
        names.sort();
        names
    }

    /// Removes every entry, leaving the map empty.
    ///
    /// Tries whose handles are still held elsewhere stay alive for those
    /// holders; they are only unreachable through this map.
    pub fn clear(&self) {
        self.write().clear();
    }
}
