//! Collections and key traits for collecting rows in sorted order.
//!
//! Use [`SortedVec`] to accumulate rows, [`SortedSlice`] to own the collected
//! rows, and [`SortedSliceView`] to borrow groups selected by composite keys.

mod sorted;

pub use sorted::{
    ExtractKey, ExtractKeyWithContext, OrderError, SortOrder, Sorted, SortedSlice, SortedSliceView,
    SortedVec,
};
