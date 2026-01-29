use super::{BoxedKeyFragment, KeyFragment, TrieLeafIter, TrieLeafs};

#[derive(Debug)]
pub(super) struct TrieNode<TValue> {
    /// key for this node
    key: BoxedKeyFragment,
    /// value of full match of `key_bytes`
    pub(super) value: Option<TValue>,
    /// values for continuing the key
    pub(super) children: TrieLeafs<TValue>,
}

impl<TValue> TrieNode<TValue> {
    pub(super) fn new(key: KeyFragment<'_>, value: TValue) -> Self {
        Self {
            key: BoxedKeyFragment::from(key),
            value: Some(value),
            children: TrieLeafs::with_capacity(0),
        }
    }

    pub(super) fn start_byte(&self) -> u8 {
        self.key.start_byte()
    }

    /// Insert a value with the given key. Returns the old value if it exists.
    pub(super) fn insert(&mut self, new_key: &KeyFragment<'_>, value: TValue) -> Option<TValue> {
        let self_key = &self.key.as_ref();

        // Find the length of the common prefix between the input key and self.key
        let common_prefix_len = new_key.common_prefix_len(self_key);

        // Handle case where new_key is a strict prefix of self_key
        if common_prefix_len == new_key.len() && common_prefix_len < self_key.len() {
            // Split existing node: promote input key as this node, child holds remainder of existing key
            let (common_key, other_key, _) = self_key.split_key(new_key, common_prefix_len);
            let mut old_children = TrieLeafs::new();
            std::mem::swap(&mut self.children, &mut old_children);
            let child = TrieNode {
                key: BoxedKeyFragment::from(other_key),
                value: self.value.take(),
                children: old_children,
            };
            self.key = BoxedKeyFragment::from(common_key);
            self.value = Some(value);
            self.children.insert(child);
            return None;
        }

        if common_prefix_len == self_key.len() {
            let least_key = new_key.least_key(common_prefix_len);

            if least_key.is_empty() {
                // The key matches self completely
                return self.value.replace(value);
            }

            // Search for a matching child node
            let start_byte = least_key.start_byte();
            if let Some(match_node) = self.children.find_by_start_byte_mut(start_byte) {
                // Found a match, insert into child
                match_node.insert(&least_key, value)
            } else {
                // No match found, create a new child node
                let new_node = TrieNode::new(least_key, value);
                self.children.insert(new_node);
                None
            }
        } else {
            // Partial match between self.key and input key
            let (common_key, other_key, least_key) = self_key.split_key(new_key, common_prefix_len);

            let mut children = TrieLeafs::with_capacity(2);
            std::mem::swap(&mut self.children, &mut children);
            let new_self = Self {
                key: BoxedKeyFragment::from(other_key),
                value: self.value.take(),
                children,
            };
            let new_other = Self {
                key: BoxedKeyFragment::from(least_key),
                value: Some(value),
                children: TrieLeafs::new(),
            };
            self.key = BoxedKeyFragment::from(common_key);
            self.children.insert(new_self);
            self.children.insert(new_other);
            // No existing value, return None
            None
        }
    }

    pub(super) fn iter<'a>(&'a self, mut path: Vec<KeyFragment<'a>>) -> TrieNodeIter<'a, TValue> {
        path.push(self.key.as_ref());

        TrieNodeIter {
            keys: path.clone(),
            value: self.value.as_ref(),
            child_iter: self.children.iter(path),
        }
    }

    pub(super) fn len(&self) -> usize {
        if self.value.is_some() {
            self.children.len() + 1
        } else {
            self.children.len()
        }
    }

    /// Split a fragment by this node's key, returning the remaining fragment if full match
    pub(super) fn split_key<'a>(&self, key: &KeyFragment<'a>) -> Option<KeyFragment<'a>> {
        let common = key.common_prefix_len(&self.key.as_ref());
        if common != self.key.as_ref().len() {
            return None;
        }
        Some(key.least_key(common))
    }
}

pub(super) struct TrieNodeIter<'a, TValue> {
    keys: Vec<KeyFragment<'a>>,
    value: Option<&'a TValue>,
    child_iter: TrieLeafIter<'a, TValue>,
}

impl<'a, TValue: 'a> Iterator for TrieNodeIter<'a, TValue> {
    type Item = (Vec<KeyFragment<'a>>, &'a TValue);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(value) = self.value.take() {
            return Some((self.keys.clone(), value));
        }

        self.child_iter.next()
    }
}
