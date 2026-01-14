//! Basic generic collections for `sql-fun`
//!

///
/// Fixed length immutable vector (Boxed slice)
///
#[derive(Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash, Debug)]
#[repr(transparent)]
#[serde(transparent)]
pub struct IVec<T>(Box<[T]>);

impl<T> Default for IVec<T> {
    fn default() -> Self {
        Self(Box::default())
    }
}

impl<T> From<Vec<T>> for IVec<T> {
    fn from(value: Vec<T>) -> Self {
        Self(value.into_boxed_slice())
    }
}

impl<T> FromIterator<T> for IVec<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        IVec(iter.into_iter().collect::<Vec<_>>().into_boxed_slice())
    }
}

impl<T> std::ops::Deref for IVec<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a, T> IntoIterator for &'a IVec<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut IVec<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T> IntoIterator for IVec<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<T> IVec<T> {
    /// get reference of internal slice
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    /// Returns the number of elements in the slice.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if the slice has a length of 0.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// convert into Vec
    #[must_use]
    pub fn info_vec(self) -> Vec<T> {
        Vec::from(self.0)
    }

    /// Returns an iterator over the slice.
    ///
    /// The iterator yields all items from start to end.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.0.iter()
    }

    /// get mutateble iterator
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.0.iter_mut()
    }
}
