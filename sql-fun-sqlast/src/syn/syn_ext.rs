/// conversion support trait for syn object
pub trait Opt<T> {
    /// get reference of inner value
    fn as_inner(&self) -> Option<T>;

    /// covert to inner value
    fn unwrap(&self) -> T {
        self.as_inner().unwrap()
    }

    /// get inner value
    ///
    /// # Errors
    ///
    /// - `err` if item is None
    ///
    fn ok_or_else<E, F>(&self, err: F) -> Result<T, E>
    where
        F: FnOnce() -> E,
    {
        self.as_inner().ok_or_else(err)
    }

    /// covert item with function
    fn map<U, F>(&self, f: F) -> Option<U>
    where
        F: FnOnce(T) -> U,
    {
        self.as_inner().map(f)
    }
}

/// conversion support trait for syn object list
pub trait ListOpt<T: Clone> {
    /// internal value type
    type OptType: From<Option<T>>;

    /// get reference of inner value
    fn as_inner(&self) -> Option<Vec<T>>;

    /// get zero based index entry
    fn get(&self, index: usize) -> Self::OptType {
        let Some(inner) = self.as_inner() else {
            return Self::OptType::from(None);
        };

        Self::OptType::from(inner.get(index).cloned())
    }

    /// convert list items
    fn map<U, F, V>(&self, f: F) -> Option<Vec<U>>
    where
        F: Fn(T) -> V,
        V: Opt<U>,
    {
        let inner = self.as_inner()?;
        Some(
            inner
                .iter()
                .cloned()
                .filter_map(|v| f(v).as_inner())
                .collect(),
        )
    }

    /// covert list items
    fn map_values<U, F>(&self, f: F) -> Option<Vec<U>>
    where
        F: Fn(T) -> Option<U>,
    {
        let inner = self.as_inner()?;
        Some(inner.iter().cloned().filter_map(f).collect())
    }
}
