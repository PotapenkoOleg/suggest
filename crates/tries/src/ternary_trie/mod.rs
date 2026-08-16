use crate::symbol_table::{PrefixSearch, SymbolTable};
use std::cmp::Ordering;

mod tests;
mod tests_integration;
mod tests_original;

struct Node<E> {
    c: char,                      // Character stored at this node
    value: Option<E>,             // Value if this node represents a key
    left: Option<Box<Node<E>>>,   // Left child (character < node's character)
    middle: Option<Box<Node<E>>>, // Middle child (next character in key)
    right: Option<Box<Node<E>>>,  // Right child (character > node's character)
}

pub struct TernarySearchTrie<E> {
    root: Option<Box<Node<E>>>,
    size: usize,
}

impl<E> Default for TernarySearchTrie<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> TernarySearchTrie<E> {
    pub fn new() -> Self {
        TernarySearchTrie {
            root: None,
            size: 0,
        }
    }
}

impl<E: Clone> SymbolTable<E> for TernarySearchTrie<E> {
    fn put(&mut self, key: String, value: E) {
        if key.is_empty() {
            return;
        }

        let chars: Vec<char> = key.chars().collect();
        let was_new_key = Self::put_recursive(&mut self.root, &chars, value, 0);

        if was_new_key {
            self.size += 1;
        }
    }

    fn get(&self, key: &str) -> Option<E> {
        if key.is_empty() {
            return None;
        }

        let chars: Vec<char> = key.chars().collect();
        let x = Self::get_recursive(&self.root, &chars, 0);

        if x.is_some() {
            Some(x.unwrap().clone())
        } else {
            None
        }
    }

    fn delete(&mut self, key: &str) {
        if key.is_empty() {
            return;
        }

        let chars: Vec<char> = key.chars().collect();
        if Self::delete_recursive(&mut self.root, &chars, 0) {
            self.size -= 1;
        }
    }

    fn contains(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    fn clear(&mut self) {
        self.root = None;
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
        let prefix = String::new();
        Self::collect_keys(&self.root, &mut result, &prefix);
        result
    }
}

impl<E: Clone> PrefixSearch for TernarySearchTrie<E> {
    fn get_keys_with_prefix(&self, prefix: &str) -> Vec<String> {
        if prefix.is_empty() {
            return self.get_all_keys();
        }

        let mut result = Vec::new();

        let chars: Vec<char> = prefix.chars().collect();

        // Find the node that corresponds to the end of the prefix
        if let Some(subtree) = Self::find_prefix_node(&self.root, &chars, 0) {
            // If this node has a value, it's a complete key
            if subtree.value.is_some() {
                result.push(prefix.to_string());
            }

            // Now collect all keys in the middle subtree
            Self::collect_all_keys(&subtree.middle, &mut result, prefix);
        }

        result
    }

    fn longest_prefix_of(&self, prefix: &str) -> Option<String> {
        if prefix.is_empty() {
            return None;
        }

        let chars: Vec<char> = prefix.chars().collect();
        let length = Self::longest_prefix_length(&self.root, &chars, 0, 0);

        if length == 0 {
            None
        } else {
            Some(prefix[0..length].to_string())
        }
    }
}

impl<E: Clone> TernarySearchTrie<E> {
    // Helper function for inserting a key-value pair
    fn put_recursive(node: &mut Option<Box<Node<E>>>, key: &[char], value: E, pos: usize) -> bool {
        if pos >= key.len() {
            return false;
        }

        let current_char = key[pos];
        let is_last_char = pos == key.len() - 1;

        match node {
            None => {
                // Create a new node
                *node = Some(Box::new(Node {
                    c: current_char,
                    value: if is_last_char {
                        Some(value.clone())
                    } else {
                        None
                    },
                    left: None,
                    middle: None,
                    right: None,
                }));

                if is_last_char {
                    true // New key was added
                } else {
                    let next_node = node.as_mut().unwrap();
                    Self::put_recursive(&mut next_node.middle, key, value, pos + 1)
                }
            }
            Some(current_node) => match current_char.cmp(&current_node.c) {
                Ordering::Less => Self::put_recursive(&mut current_node.left, key, value, pos),
                Ordering::Greater => Self::put_recursive(&mut current_node.right, key, value, pos),
                Ordering::Equal => {
                    if is_last_char {
                        let is_new_key = current_node.value.is_none();
                        current_node.value = Some(value);
                        is_new_key
                    } else {
                        Self::put_recursive(&mut current_node.middle, key, value, pos + 1)
                    }
                }
            },
        }
    }

    // Helper function for retrieving a value
    fn get_recursive<'a>(
        node: &'a Option<Box<Node<E>>>,
        key: &'a [char],
        pos: usize,
    ) -> Option<&'a E> {
        if node.is_none() || pos >= key.len() {
            return None;
        }

        let current_node = node.as_ref().unwrap();
        let current_char = key[pos];

        match current_char.cmp(&current_node.c) {
            Ordering::Less => Self::get_recursive(&current_node.left, key, pos),
            Ordering::Greater => Self::get_recursive(&current_node.right, key, pos),
            Ordering::Equal => {
                if pos == key.len() - 1 {
                    current_node.value.as_ref()
                } else {
                    Self::get_recursive(&current_node.middle, key, pos + 1)
                }
            }
        }
    }

    // Helper function for deleting a key
    fn delete_recursive(node: &mut Option<Box<Node<E>>>, key: &[char], pos: usize) -> bool {
        if node.is_none() || pos >= key.len() {
            return false;
        }

        let was_deleted;
        let current_char = key[pos];

        {
            let current_node = node.as_mut().unwrap();

            match current_char.cmp(&current_node.c) {
                Ordering::Less => {
                    was_deleted = Self::delete_recursive(&mut current_node.left, key, pos);
                }
                Ordering::Greater => {
                    was_deleted = Self::delete_recursive(&mut current_node.right, key, pos);
                }
                Ordering::Equal => {
                    if pos == key.len() - 1 {
                        was_deleted = current_node.value.is_some();
                        current_node.value = None;
                    } else {
                        was_deleted =
                            Self::delete_recursive(&mut current_node.middle, key, pos + 1);
                    }
                }
            }
        }

        // Check if we can remove this node (no value and no children)
        let should_remove = if let Some(current_node) = node.as_ref() {
            current_node.value.is_none()
                && current_node.left.is_none()
                && current_node.middle.is_none()
                && current_node.right.is_none()
        } else {
            false
        };

        if should_remove {
            *node = None;
        }

        was_deleted
    }

    // Helper function for collecting all keys
    fn collect_keys(node: &Option<Box<Node<E>>>, result: &mut Vec<String>, prefix: &String) {
        if node.is_none() {
            return;
        }

        let current_node = node.as_ref().unwrap();

        // Recursively collect keys from left subtree
        Self::collect_keys(&current_node.left, result, prefix);

        // Add this node's character to the prefix
        let mut new_prefix = prefix.clone();
        new_prefix.push(current_node.c);

        // If this node has a value, it's a complete key
        if current_node.value.is_some() {
            result.push(new_prefix.clone());
        }

        // Recursively collect keys from middle subtree
        Self::collect_keys(&current_node.middle, result, &new_prefix);

        // Recursively collect keys from right subtree
        Self::collect_keys(&current_node.right, result, prefix);
    }

    // Helper function for finding a node that corresponds to a prefix
    fn find_prefix_node<'a>(
        node: &'a Option<Box<Node<E>>>,
        prefix: &[char],
        pos: usize,
    ) -> Option<&'a Box<Node<E>>> {
        if node.is_none() || pos >= prefix.len() {
            return None;
        }

        let current_node = node.as_ref().unwrap();
        let current_char = prefix[pos];

        match current_char.cmp(&current_node.c) {
            Ordering::Less => Self::find_prefix_node(&current_node.left, prefix, pos),
            Ordering::Greater => Self::find_prefix_node(&current_node.right, prefix, pos),
            Ordering::Equal => {
                if pos == prefix.len() - 1 {
                    // We found the node that corresponds to the last character of the prefix
                    Some(current_node)
                } else {
                    // Continue searching
                    Self::find_prefix_node(&current_node.middle, prefix, pos + 1)
                }
            }
        }
    }

    // Helper function for collecting all keys with a given prefix
    fn collect_all_keys(node: &Option<Box<Node<E>>>, result: &mut Vec<String>, prefix: &str) {
        if node.is_none() {
            return;
        }

        let current_node = node.as_ref().unwrap();

        // Recursively collect keys from left subtree
        Self::collect_all_keys(&current_node.left, result, prefix);

        // Add this node's character to the prefix
        let mut new_prefix = prefix.to_string();
        new_prefix.push(current_node.c);

        // If this node has a value, it's a complete key
        if current_node.value.is_some() {
            result.push(new_prefix.clone());
        }

        // Recursively collect keys from middle subtree
        Self::collect_all_keys(&current_node.middle, result, &new_prefix);

        // Recursively collect keys from right subtree
        Self::collect_all_keys(&current_node.right, result, prefix);
    }

    // Helper function for finding the longest prefix
    fn longest_prefix_length(
        node: &Option<Box<Node<E>>>,
        key: &[char],
        pos: usize,
        length: usize,
    ) -> usize {
        if node.is_none() || pos >= key.len() {
            return length;
        }

        let current_node = node.as_ref().unwrap();
        let current_char = key[pos];

        match current_char.cmp(&current_node.c) {
            Ordering::Less => Self::longest_prefix_length(&current_node.left, key, pos, length),
            Ordering::Greater => Self::longest_prefix_length(&current_node.right, key, pos, length),
            Ordering::Equal => {
                // We've matched this character
                let new_length = if current_node.value.is_some() {
                    pos + 1 // Update length if this node has a value
                } else {
                    length
                };

                if pos == key.len() - 1 {
                    // We've reached the end of the key
                    new_length
                } else {
                    // Continue searching
                    Self::longest_prefix_length(&current_node.middle, key, pos + 1, new_length)
                }
            }
        }
    }
}
