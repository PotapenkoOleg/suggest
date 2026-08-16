//! The abstractions a string-keyed store implements.
//!
//! [`SymbolTable`] is the map-like half: the operations any keyed store could
//! offer. [`PrefixSearch`] is the half a trie exists for — queries that exploit
//! the shared structure of the stored keys rather than probing one key at a
//! time, and which a hash-backed table could not answer efficiently.
//!
//! Both traits are dyn-compatible:
//!
//! ```
//! use tries::{PrefixSearch, SymbolTable, TernarySearchTrie};
//!
//! let mut trie = TernarySearchTrie::<u32>::new();
//! trie.put("sea".to_string(), 2);
//!
//! let table: &dyn SymbolTable<u32> = &trie;
//! assert_eq!(table.get("sea"), Some(2));
//!
//! let search: &dyn PrefixSearch = &trie;
//! assert_eq!(search.get_keys_with_prefix("s"), ["sea"]);
//! ```

/// The map-like operations of a string-keyed symbol table.
///
/// The value type `E` is `Clone` because [`get`](SymbolTable::get) returns an
/// owned copy rather than a borrow.
///
/// The empty string is not a storable key: `put` ignores it, and the lookups
/// report it as absent.
///
/// # Examples
///
/// ```
/// use tries::{SymbolTable, TernarySearchTrie};
///
/// let mut table = TernarySearchTrie::<u32>::new();
/// table.put("shore".to_string(), 7);
/// table.put("sea".to_string(), 2);
/// table.put("shells".to_string(), 3);
///
/// assert_eq!(table.get("sea"), Some(2));
/// assert_eq!(table.get_size(), 3);
/// assert_eq!(table.get_all_keys(), ["sea", "shells", "shore"]);
/// ```
pub trait SymbolTable<E: Clone> {
    /// Associates `value` with `key`, replacing any value already stored there.
    fn put(&mut self, key: String, value: E);

    /// Returns a copy of the value stored under `key`, or `None` if absent.
    fn get(&self, key: &str) -> Option<E>;

    /// Removes `key` and its value. Removing an absent key is a no-op.
    fn delete(&mut self, key: &str);

    /// Returns `true` if a value is stored under `key`.
    fn contains(&self, key: &str) -> bool;

    /// Removes every entry, leaving the table empty.
    fn clear(&mut self);

    /// Returns `true` if the table holds no entries.
    fn is_empty(&self) -> bool;

    /// Returns the number of entries.
    fn get_size(&self) -> usize;

    /// Returns every stored key, in lexicographic order.
    fn get_all_keys(&self) -> Vec<String>;
}

/// Prefix queries over a string-keyed store.
///
/// # Examples
///
/// ```
/// use tries::{PrefixSearch, SymbolTable, TernarySearchTrie};
///
/// let mut table = TernarySearchTrie::<u32>::new();
/// for (i, word) in ["she", "shells", "shore", "sea"].iter().enumerate() {
///     table.put(word.to_string(), i as u32);
/// }
///
/// assert_eq!(table.get_keys_with_prefix("sh"), ["she", "shells", "shore"]);
/// assert_eq!(table.longest_prefix_of("shellsort"), Some("shells".to_string()));
/// ```
pub trait PrefixSearch {
    /// Returns every stored key beginning with `prefix`, in lexicographic
    /// order. An empty prefix matches every key.
    fn get_keys_with_prefix(&self, prefix: &str) -> Vec<String>;

    /// Returns the longest stored key that is a prefix of `prefix`, or `None`
    /// if no stored key is one. Note the direction: this searches for keys
    /// *contained in* the argument, whereas
    /// [`get_keys_with_prefix`](PrefixSearch::get_keys_with_prefix) searches
    /// for keys *extending* it.
    fn longest_prefix_of(&self, prefix: &str) -> Option<String>;
}
