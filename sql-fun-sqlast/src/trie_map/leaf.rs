use super::{KeyFragment, TrieNode, TrieNodeIter};

#[derive(Debug)]
pub(super) struct TrieLeaves<TValue>(Vec<TrieNode<TValue>>);

impl<TValue> TrieLeaves<TValue> {
    pub(super) fn new() -> Self {
        Self::with_capacity(0)
    }

    pub(super) fn with_capacity(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }

    pub(super) fn find_by_start_byte_mut(
        &mut self,
        start_byte: u8,
    ) -> Option<&mut TrieNode<TValue>> {
        self.0.iter_mut().find(|n| n.start_byte() == start_byte)
    }

    fn find_by_start_byte(&self, start_byte: u8) -> Option<&TrieNode<TValue>> {
        self.0.iter().find(|n| n.start_byte() == start_byte)
    }

    /// Lookup by key fragment.
    pub(super) fn find_by_fragment(&self, frag: &KeyFragment<'_>) -> Option<&TrieNode<TValue>> {
        self.find_by_start_byte(frag.start_byte())
    }

    /// Lookup by key fragment mut.
    pub(super) fn find_by_fragment_mut(
        &mut self,
        frag: &KeyFragment<'_>,
    ) -> Option<&mut TrieNode<TValue>> {
        self.find_by_start_byte_mut(frag.start_byte())
    }

    pub(super) fn insert(&mut self, item: TrieNode<TValue>) {
        if self.0.is_empty() {
            self.0.push(item);
        } else {
            let pos = self
                .0
                .binary_search_by(|n| n.start_byte().cmp(&item.start_byte()))
                .expect_err("duplicate start_byte found: internal logic error");
            self.0.insert(pos, item);
        }
    }

    pub(super) fn iter<'a>(&'a self, keys: Vec<KeyFragment<'a>>) -> TrieLeafIter<'a, TValue> {
        TrieLeafIter {
            keys,
            child_iter: self.0.iter(),
            item_iter: None,
        }
    }

    pub(super) fn len(&self) -> usize {
        self.0.iter().map(TrieNode::len).sum()
    }

    pub(super) const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub(super) struct TrieLeafIter<'a, TValue> {
    keys: Vec<KeyFragment<'a>>,
    child_iter: std::slice::Iter<'a, TrieNode<TValue>>,
    item_iter: Option<Box<TrieNodeIter<'a, TValue>>>,
}

impl<'a, TValue: 'a> Iterator for TrieLeafIter<'a, TValue> {
    type Item = (Vec<KeyFragment<'a>>, &'a TValue);

    fn next(&mut self) -> Option<Self::Item> {
        // when item_iter is some, iterating item from item_iter
        if let Some(item_iter) = self.item_iter.as_mut() {
            if let Some(item) = item_iter.next() {
                return Some(item);
            }
            // all item is iterated clear item_iter
            self.item_iter = None;
        }
        // fetch next item_iter
        let current = self.child_iter.next()?;
        let mut item_iter = current.iter(self.keys.clone());
        let result = item_iter.next();
        self.item_iter = Some(Box::new(item_iter));
        result
    }
}
