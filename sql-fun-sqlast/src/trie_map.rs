//! A trie-based map used internally by `sql-fun` for efficient prefix-ordered key storage.
//!
//! This structure is primarily used to store schema-qualified
//! SQL object names or generated query identifiers in a prefix-compressed
//! and lexically sortable form.
//!
//! This structure stores key-value pairs in lexicographic order using a trie,
//! allowing for efficient prefix-based lookups, ordered iteration, and
//! storage of hierarchical keys like schema paths or query identifiers.
//!
//! Keys are byte strings (`&str`), stored and compared by UTF-8 byte order.
//!
//! Internally, the trie is implemented as a tree of `TrieNode`s,
//! with shared prefix compression and stable iteration order.
//! Empty-string keys are supported via a separate storage slot.
//!
//! This module is not part of the public API, but is exposed through source code
//! and licensed accordingly. Users may extract and reuse this implementation
//! under the terms of the `sql-fun` license.

mod key_fragment;
mod leaf;
mod node;

use self::{
    key_fragment::{BoxedKeyFragment, KeyFragment},
    leaf::{TrieLeafIter, TrieLeaves},
    node::{TrieNode, TrieNodeIter},
};

/// Map string to value
///
#[derive(Debug)]
pub struct TrieMap<TValue> {
    /// Value for the empty key
    empty_node: Option<TValue>,
    /// Top-level nodes
    nodes: TrieLeaves<TValue>,
}

impl<TValue> Default for TrieMap<TValue> {
    fn default() -> Self {
        Self::new()
    }
}

impl<TValue> TrieMap<TValue> {
    /// create a empty instance
    #[must_use]
    pub fn new() -> Self {
        Self {
            empty_node: None,
            nodes: TrieLeaves::new(),
        }
    }

    /// insert value to key
    pub fn insert(&mut self, key: &str, value: TValue) -> Option<TValue> {
        if key.is_empty() {
            return self.empty_node.replace(value);
        }
        let key = KeyFragment::from(key.as_bytes());
        let start_byte = key.start_byte();
        if let Some(match_node) = self.nodes.find_by_start_byte_mut(start_byte) {
            match_node.insert(&key, value)
        } else {
            let new_node = TrieNode::new(key, value);
            self.nodes.insert(new_node);
            None
        }
    }

    /// get iterator
    pub fn iter(&self) -> impl Iterator<Item = (String, &TValue)> {
        TrieMapIter {
            empty_node: self.empty_node.as_ref(),
            leaf_iter: self.nodes.iter(Vec::default()),
        }
    }

    /// get value by key
    pub fn get(&self, key: &str) -> Option<&TValue> {
        if key.is_empty() {
            return self.empty_node.as_ref();
        }
        let mut frag = KeyFragment::from(key.as_bytes());
        let mut node = self.nodes.find_by_fragment(&frag)?;
        frag = node.split_key(&frag)?;
        if frag.is_empty() {
            return node.value.as_ref();
        }

        loop {
            node = node.children.find_by_fragment(&frag)?;
            frag = node.split_key(&frag)?;
            if frag.is_empty() {
                return node.value.as_ref();
            }
        }
    }

    /// get mutable reference by key
    pub fn get_mut(&mut self, key: &str) -> Option<&mut TValue> {
        if key.is_empty() {
            return self.empty_node.as_mut();
        }
        let mut frag = KeyFragment::from(key.as_bytes());
        let mut node = self.nodes.find_by_fragment_mut(&frag)?;
        frag = node.split_key(&frag)?;
        if frag.is_empty() {
            return node.value.as_mut();
        }

        loop {
            node = node.children.find_by_fragment_mut(&frag)?;
            frag = node.split_key(&frag)?;
            if frag.is_empty() {
                return node.value.as_mut();
            }
        }
    }

    /// Returns the number of entries in the map.
    pub fn len(&self) -> usize {
        if self.empty_node.is_some() {
            self.nodes.len() + 1
        } else {
            self.nodes.len()
        }
    }

    /// returns true if empty
    pub fn is_empty(&self) -> bool {
        self.empty_node.is_none() && self.nodes.is_empty()
    }
}

struct TrieMapIter<'a, TValue> {
    empty_node: Option<&'a TValue>,
    leaf_iter: TrieLeafIter<'a, TValue>,
}

impl<'a, TValue> Iterator for TrieMapIter<'a, TValue> {
    type Item = (String, &'a TValue);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(empty_node) = self.empty_node.take() {
            return Some((String::new(), empty_node));
        }
        let (fragments, item) = self.leaf_iter.next()?;

        // safe to use from_utf8_unchecked, but unsafe
        Some((KeyFragment::to_string_key(&fragments).unwrap(), item))
    }
}

#[cfg(test)]
mod tests {
    use super::TrieMap;

    #[test]
    fn test_insert_and_get() {
        let mut map = TrieMap::new();
        assert_eq!(map.len(), 0);
        assert!(map.get("key1").is_none());

        map.insert("key1", 10);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("key1"), Some(&10));
        assert!(map.get("key2").is_none());

        // overwrite existing key
        let old = map.insert("key1", 20);
        assert_eq!(old, Some(10));
        assert_eq!(map.get("key1"), Some(&20));
    }

    #[test]
    fn test_empty_key() {
        let mut map = TrieMap::new();
        map.insert("", 5);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(""), Some(&5));
        let old = map.insert("", 7);
        assert_eq!(old, Some(5));
        assert_eq!(map.get(""), Some(&7));
    }

    #[test]
    fn test_iter_order() {
        let mut map = TrieMap::new();
        map.insert("b", 2);
        map.insert("a", 1);
        map.insert("aa", 11);
        map.insert("ab", 12);
        let items: Vec<_> = map.iter().collect();
        let keys: Vec<String> = items.iter().map(|(k, _)| k.clone()).collect();
        assert_eq!(
            keys,
            vec![
                "a".to_string(),
                "aa".to_string(),
                "ab".to_string(),
                "b".to_string()
            ]
        );
        let values: Vec<i32> = items.iter().map(|(_, v)| **v).collect();
        assert_eq!(values, vec![1, 11, 12, 2]);
    }

    #[test]
    fn test_get_common_prefix_loop() {
        let mut map = TrieMap::new();
        map.insert("foobarbaz", 3);
        map.insert("foo", 1);
        map.insert("foobar", 2);
        map.insert("fool_ploof_needed", 4);
        assert_eq!(map.get("foo"), Some(&1));
        assert_eq!(map.get("foobar"), Some(&2));
        assert_eq!(map.get("foobarbaz"), Some(&3));
        assert_eq!(map.get("fool_ploof_needed"), Some(&4));
    }

    #[test]
    fn test_partial_split() {
        let mut map = TrieMap::new();
        // insert in normal order
        assert_eq!(map.insert("abcd", 1), None);
        assert_eq!(map.insert("abxy", 2), None);
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("abcd"), Some(&1));
        assert_eq!(map.get("abxy"), Some(&2));
        // reverse order
        let mut map2 = TrieMap::new();
        assert_eq!(map2.insert("abxy", 20), None);
        assert_eq!(map2.insert("abcd", 10), None);
        assert_eq!(map2.len(), 2);
        assert_eq!(map2.get("abcd"), Some(&10));
        assert_eq!(map2.get("abxy"), Some(&20));
        // add third partial key
        assert_eq!(map.insert("abmn", 3), None);
        assert_eq!(map.len(), 3);
        assert_eq!(map.get("abmn"), Some(&3));
    }

    #[test]
    fn test_iter_empty_key_first() {
        let mut map = TrieMap::new();
        // only empty key
        map.insert("", 100);
        assert_eq!(map.len(), 1);
        let mut iter = map.iter();
        assert_eq!(iter.next(), Some(("".to_string(), &100)));
        assert_eq!(iter.next(), None);

        // mix empty key with others in fresh map
        let mut map2 = TrieMap::new();
        map2.insert("", 100);
        map2.insert("a", 1);
        map2.insert("b", 2);
        assert_eq!(map2.len(), 3);
        let mut iter2 = map2.iter();
        assert_eq!(iter2.next(), Some(("".to_string(), &100)));
        let rest: Vec<_> = iter2.collect();
        assert_eq!(rest, vec![("a".to_string(), &1), ("b".to_string(), &2)]);
    }
}
