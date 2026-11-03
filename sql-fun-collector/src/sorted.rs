use std::{marker::PhantomData, ops::Range};

/// A growable collection for rows supplied in nondecreasing key order.
///
/// Rows are stored in insertion order; callers must supply them in sorted order.
pub struct SortedVec<T, O>
where
    O: std::cmp::Ord + std::fmt::Debug + Clone,
{
    vec: Vec<T>,
    last_key: Option<O>,
}

impl<T, O> Default for SortedVec<T, O>
where
    O: std::cmp::Ord + std::fmt::Debug + Clone,
{
    fn default() -> Self {
        Self {
            vec: Vec::new(),
            last_key: None,
        }
    }
}

/// An owned, fixed-size collection of rows ordered by keys of type `O`.
pub struct SortedSlice<T, O>
where
    O: std::cmp::Ord + std::fmt::Debug + Clone,
{
    slice: Box<[T]>,
    _phantom: PhantomData<O>,
}

/// A borrowed view of rows ordered by keys of type `O`.
///
/// The key type may represent the remaining components after selecting a prefix.
pub struct SortedSliceView<'a, T, O>
where
    O: std::cmp::Ord + std::fmt::Debug + Clone,
{
    slice: &'a [T],
    _phantom: PhantomData<O>,
}

/// Extracts an ordering key from a row.
pub trait ExtractKey<O> {
    /// Returns the key used to order or locate this row.
    fn get_key(&self) -> O;
}

/// Extracts an ordering key using external context, such as related rows.
pub trait ExtractKeyWithContext<Ctx, O> {
    /// Returns this row's ordering key using `ctx` to resolve external data.
    fn get_key_with_context(&self, ctx: &Ctx) -> O;
}

/// An error caused by appending a key in decreasing order.
#[derive(thiserror::Error, Debug)]
pub enum OrderError<O>
where
    O: std::cmp::Ord + std::fmt::Debug,
{
    /// The previously recorded key is greater than the next key.
    ///
    /// The fields contain the previous key and the next key, respectively.
    #[error("invalid order: {0:?} > {1:?} ")]
    InvalidOrder(O, O),
}

impl<O> OrderError<O>
where
    O: std::cmp::Ord + std::fmt::Debug + Clone,
{
    /// Returns an [`OrderError::InvalidOrder`] containing clones of both keys.
    ///
    /// # Errors
    ///
    /// Always returns [`OrderError::InvalidOrder`]; it does not compare the keys.
    pub fn raise_order_error<T>(last_key: &O, next_key: &O) -> Result<T, Self> {
        Err(Self::InvalidOrder(last_key.clone(), next_key.clone()))
    }
}

impl<T, O> SortedVec<T, O>
where
    O: std::cmp::Ord + std::fmt::Debug + Clone,
{
    /// Appends a row, checking its extracted key against any recorded last key.
    ///
    /// # Errors
    ///
    /// Returns [`OrderError::InvalidOrder`] without appending the row if its key
    /// is less than the recorded last key. Equal keys are accepted.
    pub fn push(&mut self, item: T) -> Result<(), OrderError<O>>
    where
        T: ExtractKey<O>,
    {
        let next_key = item.get_key();
        if let Some(last_key) = &self.last_key
            && *last_key > next_key
        {
            return OrderError::raise_order_error(last_key, &next_key);
        }
        self.last_key = Some(next_key);
        self.vec.push(item);
        Ok(())
    }

    /// Appends a row using the supplied key for the order check.
    ///
    /// The caller must supply a key consistent with the collection's ordering.
    ///
    /// # Errors
    ///
    /// Returns [`OrderError::InvalidOrder`] without appending the row if `key`
    /// is less than the recorded last key. Equal keys are accepted.
    pub fn push_with_key(&mut self, item: T, key: &O) -> Result<(), OrderError<O>> {
        if let Some(last_key) = &self.last_key
            && *last_key > *key
        {
            return OrderError::raise_order_error(last_key, key);
        }
        self.last_key = Some(key.clone());
        self.vec.push(item);
        Ok(())
    }

    /// Consumes the collection and moves its rows into an owned sorted slice.
    ///
    /// The existing row order is preserved without sorting.
    pub fn into_sorted_slice(self) -> SortedSlice<T, O> {
        SortedSlice {
            slice: self.vec.into_boxed_slice(),
            _phantom: PhantomData,
        }
    }
}

impl<'a, T, O> SortedSliceView<'a, T, O>
where
    O: std::cmp::Ord + std::fmt::Debug + Clone,
{
    const fn new(sorted_slice: &'a [T]) -> Self {
        Self {
            slice: sorted_slice,
            _phantom: PhantomData,
        }
    }

    /// Selects rows by head key, preserving the lifetime of the underlying rows.
    ///
    /// The returned view is ordered by the tail key and can outlive this view.
    /// This allows chained selections through temporary views.
    pub fn head(
        &self,
        head: <O as SortOrder<O>>::HeadOrder,
    ) -> SortedSliceView<'a, T, <O as SortOrder<O>>::TailOrder>
    where
        O: SortOrder<O>,
        T: ExtractKey<O>,
    {
        let range = self.head_range(head);
        SortedSliceView::new(&self.slice[range])
    }

    /// Borrows rows whose head keys are in the given half-open range.
    ///
    /// Preserves the full ordering key.
    /// Returns an empty view if the range is empty, reversed, or matches no rows.
    #[must_use]
    pub fn head_between(&self, range: Range<<O as SortOrder<O>>::HeadOrder>) -> Self
    where
        O: SortOrder<O>,
        T: ExtractKey<O>,
    {
        let rows = self.as_slice();
        if range.start >= range.end {
            return Self::new(&self.slice[..0]);
        }
        let start = rows.partition_point(|row| row.get_key().head() < range.start);
        let end = rows.partition_point(|row| row.get_key().head() < range.end);
        Self::new(&self.slice[start..end])
    }
}

#[cfg(test)]
mod test_view_head {
    use crate::sorted::{ExtractKey, Sorted, SortedSlice, SortedSliceView, SortedVec};

    struct Row(i32, i32, i32);

    impl ExtractKey<(i32, i32, i32)> for Row {
        fn get_key(&self) -> (i32, i32, i32) {
            (self.0, self.1, self.2)
        }
    }

    impl ExtractKey<(i32, i32)> for Row {
        fn get_key(&self) -> (i32, i32) {
            (self.1, self.2)
        }
    }

    fn order_lines(rows: &SortedSlice<Row, (i32, i32, i32)>) -> SortedSliceView<'_, Row, i32> {
        rows.head(1).head(10)
    }

    #[test]
    fn selection_outlives_intermediate_view() {
        let mut rows = SortedVec::default();
        for row in [Row(1, 10, 1), Row(1, 10, 2), Row(1, 11, 3), Row(2, 10, 4)] {
            rows.push(row).unwrap();
        }
        let rows = rows.into_sorted_slice();
        let lines = order_lines(&rows);

        assert_eq!(lines.iter().map(|row| row.2).collect::<Vec<_>>(), [1, 2]);
        assert!(std::ptr::eq(
            lines.as_slice().as_ptr(),
            rows.as_slice().as_ptr()
        ));
        assert!(rows.head(1).head(99).as_slice().is_empty());
    }
}

/// Provides access, lookup, and grouping for rows sorted by keys of type `O`.
///
/// Implementations must expose rows in nondecreasing key order. Binary searches
/// and range lookups require the extracted keys to agree with that order.
/// Parent and head projections must also be nondecreasing.
pub trait Sorted<T, O>
where
    O: std::cmp::Ord + std::fmt::Debug + Clone,
{
    /// Borrows all rows in their stored order.
    fn as_slice(&self) -> &[T];

    /// Borrows the entire collection as a sorted view.
    fn as_view(&self) -> SortedSliceView<'_, T, O> {
        SortedSliceView::new(self.as_slice())
    }

    /// Iterates over borrowed rows in stored order.
    fn iter<'a>(&'a self) -> impl Iterator<Item = &'a T>
    where
        T: 'a,
    {
        self.as_slice().iter()
    }

    /// Returns the row at `index`, or `None` if the index is out of bounds.
    fn get(&self, index: usize) -> Option<&T> {
        self.as_slice().get(index)
    }

    /// Finds a row by key using binary search.
    ///
    /// Returns `None` if no row matches. If several rows have the same key,
    /// any matching row may be returned.
    fn get_by(&self, key: O) -> Option<&T>
    where
        T: ExtractKey<O>,
    {
        let slice = self.as_slice();
        if let Ok(index) = slice.binary_search_by(|item| item.get_key().cmp(&key)) {
            Some(&slice[index])
        } else {
            None
        }
    }

    /// Returns the half-open index range of rows with the given parent key.
    ///
    /// Returns an empty range at the insertion position if no row matches.
    /// Extracted parent keys must agree with the parent projection of `O`.
    fn parent_range(&self, key: <O as SortOrder<O>>::ParentOrder) -> Range<usize>
    where
        O: SortOrder<O>,
        T: ExtractKey<<O as SortOrder<O>>::ParentOrder>,
    {
        let rows = self.as_slice();
        let start = rows.partition_point(|row| row.get_key() < key);
        let end = rows.partition_point(|row| row.get_key() <= key);
        Range { start, end }
    }

    /// Iterates over rows with the given parent key in stored order.
    fn iter_range<'a>(
        &'a self,
        key: <O as SortOrder<O>>::ParentOrder,
    ) -> impl Iterator<Item = &'a T>
    where
        O: SortOrder<O>,
        T: 'a + ExtractKey<<O as SortOrder<O>>::ParentOrder>,
    {
        let rows = self.as_slice();
        let range = self.parent_range(key);
        rows[range].iter()
    }

    /// Yields borrowed views of consecutive rows sharing the same parent key.
    ///
    /// Each view retains the full ordering key type `O`.
    fn group_by_parent<'a>(&'a self) -> impl Iterator<Item = SortedSliceView<'a, T, O>>
    where
        T: 'a + ExtractKey<O>,
        O: SortOrder<O>,
    {
        self.as_slice()
            .chunk_by(|item1, item2| item1.get_key().parent() == item2.get_key().parent())
            .map(|g| SortedSliceView::new(g))
    }

    /// Returns the half-open index range of rows with the given head key.
    ///
    /// Returns an empty range at the insertion position if no row matches.
    fn head_range(&self, head: <O as SortOrder<O>>::HeadOrder) -> Range<usize>
    where
        O: SortOrder<O>,
        T: ExtractKey<O>,
    {
        let rows = self.as_slice();
        let start = rows.partition_point(|row| row.get_key().head() < head);
        let end = rows.partition_point(|row| row.get_key().head() <= head);
        Range { start, end }
    }

    /// Borrows rows with the given head key as a view ordered by the tail key.
    ///
    /// The rows themselves are unchanged; only the view's ordering key type
    /// changes. Returns an empty view if no row matches.
    fn head<'a>(
        &'a self,
        head: <O as SortOrder<O>>::HeadOrder,
    ) -> SortedSliceView<'a, T, <O as SortOrder<O>>::TailOrder>
    where
        O: SortOrder<O>,
        T: 'a + ExtractKey<O>,
    {
        let range = self.head_range(head);
        let range_slice = &self.as_slice()[range];
        SortedSliceView::new(range_slice)
    }

    /// Borrows rows whose head keys are in the given half-open range.
    ///
    /// Preserves the full ordering key.
    /// Returns an empty view if the range is empty, reversed, or matches no rows.
    fn head_between(
        &self,
        range: Range<<O as SortOrder<O>>::HeadOrder>,
    ) -> SortedSliceView<'_, T, O>
    where
        O: SortOrder<O>,
        T: ExtractKey<O>,
    {
        let rows = self.as_slice();
        if range.start >= range.end {
            return SortedSliceView::new(&rows[..0]);
        }
        let start = rows.partition_point(|row| row.get_key().head() < range.start);
        let end = rows.partition_point(|row| row.get_key().head() < range.end);
        SortedSliceView::new(&rows[start..end])
    }

    /// Returns the half-open range matching a head key resolved using `ctx`.
    ///
    /// Context-derived keys must agree with the stored order. Returns an empty
    /// range at the insertion position if no row matches.
    fn head_range_with_context<Ctx>(
        &self,
        ctx: &Ctx,
        head: <O as SortOrder<O>>::HeadOrder,
    ) -> Range<usize>
    where
        O: SortOrder<O>,
        T: ExtractKeyWithContext<Ctx, O>,
    {
        let rows = self.as_slice();
        let start = rows.partition_point(|row| row.get_key_with_context(ctx).head() < head);
        let end = rows.partition_point(|row| row.get_key_with_context(ctx).head() <= head);
        Range { start, end }
    }

    /// Borrows rows matching a context-derived head key, ordered by the tail key.
    ///
    /// Context-derived keys must agree with the stored order. The rows themselves
    /// are unchanged. Returns an empty view if no row matches.
    fn head_with_context<'a, Ctx>(
        &'a self,
        ctx: &Ctx,
        head: <O as SortOrder<O>>::HeadOrder,
    ) -> SortedSliceView<'a, T, <O as SortOrder<O>>::TailOrder>
    where
        O: SortOrder<O>,
        T: ExtractKeyWithContext<Ctx, O>,
    {
        let range = self.head_range_with_context(ctx, head);
        let range_slice = &self.as_slice()[range];
        SortedSliceView::new(range_slice)
    }

    /// Returns the head key of the first row whose extracted tail matches `tail`.
    ///
    /// Searches linearly in stored order and returns `None` if no row matches.
    /// Extracted tail keys must agree with the tail component of the full key.
    fn resolve_prefix_key(
        &self,
        tail: <O as SortOrder<O>>::TailOrder,
    ) -> Option<<O as SortOrder<O>>::HeadOrder>
    where
        O: SortOrder<O>,
        T: ExtractKey<<O as SortOrder<O>>::TailOrder> + ExtractKey<O>,
    {
        self.as_slice()
            .iter()
            .find(|item| ExtractKey::<<O as SortOrder<O>>::TailOrder>::get_key(*item) == tail)
            .map(|item| ExtractKey::<O>::get_key(item).head())
    }
}

impl<T, O> Sorted<T, O> for SortedSlice<T, O>
where
    O: std::cmp::Ord + std::fmt::Debug + Clone,
{
    fn as_slice(&self) -> &[T] {
        self.slice.as_ref()
    }
}

impl<T, O> Sorted<T, O> for SortedSliceView<'_, T, O>
where
    O: std::cmp::Ord + std::fmt::Debug + Clone,
{
    fn as_slice(&self) -> &[T] {
        self.slice
    }
}

impl<T, O> Sorted<T, O> for SortedVec<T, O>
where
    O: std::cmp::Ord + std::fmt::Debug + Clone,
{
    fn as_slice(&self) -> &[T] {
        self.vec.as_slice()
    }
}

/// Decomposes a composite ordering key into prefixes and suffixes.
///
/// For `(A, B, C)`, the parent is `(A, B)`, the least component is `C`,
/// the head is `A`, and the tail is `(B, C)`. Implementations are provided
/// for tuples containing two through ten components. For pairs, the parent
/// and tail are individual components rather than one-element tuples.
pub trait SortOrder<O>
where
    O: std::cmp::Ord + std::fmt::Debug + Clone,
{
    /// The prefix containing every component except the last.
    type ParentOrder: std::cmp::Ord + std::fmt::Debug + Clone;
    /// The last, least significant ordering component.
    type LeastOrder: std::cmp::Ord + std::fmt::Debug + Clone;
    /// The first, most significant ordering component.
    type HeadOrder: std::cmp::Ord + std::fmt::Debug + Clone;
    /// The suffix containing every component except the first.
    type TailOrder: std::cmp::Ord + std::fmt::Debug + Clone;

    /// Returns all ordering components except the last.
    fn parent(&self) -> Self::ParentOrder;
    /// Returns the last ordering component.
    fn least(&self) -> Self::LeastOrder;
    /// Returns the first ordering component.
    fn head(&self) -> Self::HeadOrder;
    /// Splits the key into its first component and remaining components.
    fn split_head(&self) -> (Self::HeadOrder, Self::TailOrder);
}

impl<O, O1, O2> SortOrder<O> for (O1, O2)
where
    O: Ord + std::fmt::Debug + Clone,
    O1: Ord + std::fmt::Debug + Clone,
    O2: Ord + std::fmt::Debug + Clone,
{
    type ParentOrder = O1;
    type LeastOrder = O2;
    type HeadOrder = O1;
    type TailOrder = O2;

    fn parent(&self) -> Self::ParentOrder {
        self.0.clone()
    }

    fn least(&self) -> Self::LeastOrder {
        self.1.clone()
    }

    fn head(&self) -> Self::HeadOrder {
        self.0.clone()
    }

    fn split_head(&self) -> (Self::HeadOrder, Self::TailOrder) {
        (self.0.clone(), self.1.clone())
    }
}

macro_rules! last_ident {
($last:ident) => {
    $last
};
($head:ident, $($tail:ident),+) => {
    last_ident!($($tail),+)
};
}

macro_rules! first_ident {
    ($first:ident $(, $rest:ident)*) => {
        $first
    };
}

macro_rules! impl_sort_order_tuple {
(
    ($($all:ident),+)
    =>
    ($($parent:ident),+)
    ;
    ($($idx:tt),+)
    ,
    $least_idx:tt,
    ($($tail:ident),+),
    ($($tail_idx:tt),+)
) => {
    impl<O, $($all),+> SortOrder<O> for ($($all,)+)
    where
        O: Ord + std::fmt::Debug + Clone,
        $(
            $all: Ord + std::fmt::Debug + Clone,
        )+
    {
        type LeastOrder = last_ident!($($all),+);
        type HeadOrder = first_ident!($($all),+);

        type ParentOrder = ($($parent,)+);
        type TailOrder = ($($tail),+);

        fn parent(&self) -> Self::ParentOrder {
            (
                $(self.$idx.clone(),)+
            )
        }

        fn least(&self) -> Self::LeastOrder {
            self.$least_idx.clone()
        }

        fn head(&self) -> Self::HeadOrder {
            self.0.clone()
        }

        fn split_head(&self) -> (Self::HeadOrder, Self::TailOrder) {
            (self.0.clone(),
                (
                    $(self.$tail_idx.clone(),)+
                )
            )
        }
    }
};
}

impl_sort_order_tuple!(
    (O1, O2, O3)
    =>
    (O1, O2);
    (0, 1),
    2,
    (O2,O3),
    (1,2)
);

impl_sort_order_tuple!(
    (O1, O2, O3, O4)
    =>
    (O1, O2, O3);
    (0, 1, 2),
    3,
    (O2,O3,O4),
    (1,2,3)
);

impl_sort_order_tuple!(
    (O1, O2, O3, O4, O5)
    =>
    (O1, O2, O3, O4);
    (0, 1, 2, 3),
    4,
    (O2,O3,O4,O5),
    (1,2,3,4)
);

impl_sort_order_tuple!(
    (O1, O2, O3, O4, O5, O6)
    =>
    (O1, O2, O3, O4, O5);
    (0, 1, 2, 3, 4),
    5,
    (O2, O3, O4, O5, O6),
    (1,2,3,4,5)
);

impl_sort_order_tuple!(
    (O1, O2, O3, O4, O5, O6, O7)
    =>
    (O1, O2, O3, O4, O5, O6);
    (0, 1, 2, 3, 4, 5),
    6,
    (O2, O3, O4, O5, O6, O7),
    (1,2,3,4,5,6)
);

impl_sort_order_tuple!(
    (O1, O2, O3, O4, O5, O6, O7, O8)
    =>
    (O1, O2, O3, O4, O5, O6, O7);
    (0, 1, 2, 3, 4, 5, 6),
    7,
    (O2, O3, O4, O5, O6, O7, O8),
    (1,2,3,4,5,6,7)
);

impl_sort_order_tuple!(
    (O1, O2, O3, O4, O5, O6, O7, O8, O9)
    =>
    (O1, O2, O3, O4, O5, O6, O7, O8);
    (0, 1, 2, 3, 4, 5, 6, 7),
    8,
    (O2, O3, O4, O5, O6, O7, O8, O9),
    (1,2,3,4,5,6,7, 8)
);

impl_sort_order_tuple!(
    (O1, O2, O3, O4, O5, O6, O7, O8, O9, O10)
    =>
    (O1, O2, O3, O4, O5, O6, O7, O8, O9);
    (0, 1, 2, 3, 4, 5, 6, 7, 8),
    9,
    (O2, O3, O4, O5, O6, O7, O8, O9, O10),
    (1,2,3,4,5,6,7,8,9)
);
