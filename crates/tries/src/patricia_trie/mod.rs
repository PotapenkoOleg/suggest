//! A Patricia (radix) trie: a second implementation of [`SymbolTable`] and
//! [`PrefixSearch`].
//!
//! Where [`TernarySearchTrie`](crate::TernarySearchTrie) stores one character
//! per node, this stores a whole run of characters per edge and merges any node
//! that has a single child, so a key contributes nodes only where it actually
//! diverges from its neighbours. Storing "application" and "apply" costs three
//! nodes here (`appl`, `ication`, `y`) against twelve in the ternary trie.
//!
//! Both types are interchangeable through the traits:
//!
//! ```
//! use tries::{PatriciaTrie, PrefixSearch, SymbolTable};
//!
//! let mut trie = PatriciaTrie::<u32>::new();
//! trie.put("application".to_string(), 1);
//! trie.put("apply".to_string(), 2);
//!
//! assert_eq!(trie.get("apply"), Some(2));
//! assert_eq!(trie.get_keys_with_prefix("appl"), ["application", "apply"]);
//! assert_eq!(trie.longest_prefix_of("applying"), Some("apply".to_string()));
//! ```

use crate::symbol_table::{PrefixSearch, SymbolTable};
use std::collections::BTreeMap;

mod tests;

struct Node<E> {
    // Characters on the edge leading into this node; empty only for the root
    label: Vec<char>,
    // Value if the path from the root to this node spells a stored key
    value: Option<E>,
    // Keyed by the first character of each child's label, so traversing the map
    // in order yields keys in lexicographic order. No two children can share a
    // first character - that is exactly the case a split avoids.
    children: BTreeMap<char, Node<E>>,
}

impl<E> Node<E> {
    fn new(label: Vec<char>) -> Self {
        Node {
            label,
            value: None,
            children: BTreeMap::new(),
        }
    }
}

pub struct PatriciaTrie<E> {
    root: Node<E>,
    size: usize,
}

impl<E> Default for PatriciaTrie<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> PatriciaTrie<E> {
    pub fn new() -> Self {
        PatriciaTrie {
            root: Node::new(Vec::new()),
            size: 0,
        }
    }
}

// Helpers that never touch values, so they need no `Clone` bound.
impl<E> PatriciaTrie<E> {
    // Number of leading characters two slices agree on
    fn common_prefix_len(a: &[char], b: &[char]) -> usize {
        a.iter().zip(b).take_while(|(x, y)| x == y).count()
    }

    // Splits the edge into this node after `at` characters, pushing the
    // remainder of the label - along with this node's value and children -
    // down into a new child. Leaves `node` holding just the shared head.
    fn split(node: &mut Node<E>, at: usize) {
        let mut tail = Node::new(node.label[at..].to_vec());
        tail.value = node.value.take();
        tail.children = std::mem::take(&mut node.children);

        let tail_first = tail.label[0];
        node.label.truncate(at);
        node.children.insert(tail_first, tail);
    }

    // Walks to the node whose path from the root spells exactly `rest`
    fn find<'a>(node: &'a Node<E>, rest: &[char]) -> Option<&'a Node<E>> {
        if rest.is_empty() {
            return Some(node);
        }

        let child = node.children.get(&rest[0])?;
        let len = child.label.len();

        if rest.len() >= len && rest[..len] == child.label[..] {
            Self::find(child, &rest[len..])
        } else {
            None
        }
    }

    // Appends every key in this subtree to `out`. `prefix` is the path from the
    // root to `node` inclusive; children are visited in key order, and a node's
    // own key precedes its descendants', so `out` comes out lexicographic.
    fn collect(node: &Node<E>, prefix: &mut String, out: &mut Vec<String>) {
        if node.value.is_some() {
            out.push(prefix.clone());
        }

        for child in node.children.values() {
            let restore_to = prefix.len();
            prefix.extend(child.label.iter());
            Self::collect(child, prefix, out);
            prefix.truncate(restore_to);
        }
    }

    // Descends until `rest` is exhausted, then collects the subtree reached.
    // `rest` running out mid-edge is fine: every key below that edge still
    // starts with it.
    fn collect_with_prefix(
        node: &Node<E>,
        rest: &[char],
        prefix: &mut String,
        out: &mut Vec<String>,
    ) {
        if rest.is_empty() {
            Self::collect(node, prefix, out);
            return;
        }

        let Some(child) = node.children.get(&rest[0]) else {
            return;
        };

        let shared = Self::common_prefix_len(&child.label, rest);
        let restore_to = prefix.len();

        if shared == rest.len() {
            prefix.extend(child.label.iter());
            Self::collect(child, prefix, out);
            prefix.truncate(restore_to);
        } else if shared == child.label.len() {
            prefix.extend(child.label.iter());
            Self::collect_with_prefix(child, &rest[shared..], prefix, out);
            prefix.truncate(restore_to);
        }
        // Otherwise the key diverges from the edge and nothing below matches
    }

    // Clears the value at `rest`, then restores the radix invariant on the way
    // back up: a valueless node with no children is dropped, and one with a
    // single child is merged into that child.
    fn delete_in(node: &mut Node<E>, rest: &[char]) -> bool {
        if rest.is_empty() {
            return node.value.take().is_some();
        }

        let first = rest[0];
        let Some(child) = node.children.get_mut(&first) else {
            return false;
        };

        let len = child.label.len();
        if rest.len() < len || rest[..len] != child.label[..] {
            return false;
        }

        let was_deleted = Self::delete_in(child, &rest[len..]);

        if was_deleted && child.value.is_none() {
            if child.children.is_empty() {
                node.children.remove(&first);
            } else if child.children.len() == 1 {
                let (_, mut only) = child.children.pop_first().unwrap();
                let mut merged = std::mem::take(&mut child.label);
                merged.append(&mut only.label);
                only.label = merged;
                // The merged label still starts with `first`, so the key this
                // child is filed under in the parent stays correct.
                *child = only;
            }
        }

        was_deleted
    }
}

impl<E: Clone> SymbolTable<E> for PatriciaTrie<E> {
    fn put(&mut self, key: String, value: E) {
        if key.is_empty() {
            return;
        }

        let chars: Vec<char> = key.chars().collect();
        if Self::put_in(&mut self.root, &chars, value) {
            self.size += 1;
        }
    }

    fn get(&self, key: &str) -> Option<E> {
        if key.is_empty() {
            return None;
        }

        let chars: Vec<char> = key.chars().collect();
        Self::find(&self.root, &chars).and_then(|node| node.value.clone())
    }

    fn delete(&mut self, key: &str) {
        if key.is_empty() {
            return;
        }

        let chars: Vec<char> = key.chars().collect();
        if Self::delete_in(&mut self.root, &chars) {
            self.size -= 1;
        }
    }

    fn contains(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    fn clear(&mut self) {
        self.root = Node::new(Vec::new());
        self.size = 0;
    }

    fn is_empty(&self) -> bool {
        self.size == 0
    }

    fn get_size(&self) -> usize {
        self.size
    }

    fn get_all_keys(&self) -> Vec<String> {
        let mut result = Vec::new();
        Self::collect(&self.root, &mut String::new(), &mut result);
        result
    }
}

impl<E: Clone> PatriciaTrie<E> {
    // Inserts `rest` below `node`, splitting an edge where the key diverges.
    // Returns whether this added a key rather than replacing a value.
    fn put_in(node: &mut Node<E>, rest: &[char], value: E) -> bool {
        if rest.is_empty() {
            let is_new_key = node.value.is_none();
            node.value = Some(value);
            return is_new_key;
        }

        let first = rest[0];

        let Some(child) = node.children.get_mut(&first) else {
            let mut leaf = Node::new(rest.to_vec());
            leaf.value = Some(value);
            node.children.insert(first, leaf);
            return true;
        };

        // At least the first character matches, so `shared` is never 0
        let shared = Self::common_prefix_len(&child.label, rest);
        if shared < child.label.len() {
            Self::split(child, shared);
        }

        Self::put_in(child, &rest[shared..], value)
    }
}

impl<E> PrefixSearch for PatriciaTrie<E> {
    fn get_keys_with_prefix(&self, prefix: &str) -> Vec<String> {
        let chars: Vec<char> = prefix.chars().collect();
        let mut result = Vec::new();
        Self::collect_with_prefix(&self.root, &chars, &mut String::new(), &mut result);
        result
    }

    fn longest_prefix_of(&self, prefix: &str) -> Option<String> {
        if prefix.is_empty() {
            return None;
        }

        let chars: Vec<char> = prefix.chars().collect();
        let mut node = &self.root;
        let mut consumed = 0;
        let mut longest = 0;

        while consumed < chars.len() {
            let Some(child) = node.children.get(&chars[consumed]) else {
                break;
            };

            let remaining = &chars[consumed..];
            let len = child.label.len();
            if remaining.len() < len || remaining[..len] != child.label[..] {
                break;
            }

            consumed += len;
            node = child;

            if node.value.is_some() {
                longest = consumed;
            }
        }

        if longest == 0 {
            None
        } else {
            // Rebuilt from characters rather than sliced by byte offset, so
            // multi-byte keys survive
            Some(chars[..longest].iter().collect())
        }
    }
}
