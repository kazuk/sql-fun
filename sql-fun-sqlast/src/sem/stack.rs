//! Stack checkpoints shared by recursive conversion and semantic resolution.

// Leave room for the largest frame between checkpoints, including debug builds
// and semantic factories that clone their inputs. Keep the checkpoint wrappers
// small and the recursive bodies out of line so the check precedes those frames.
const RED_ZONE: usize = 256 * 1024;
const SEGMENT_SIZE: usize = 2 * 1024 * 1024;

#[inline]
pub(super) fn with_stack<T>(operation: impl FnOnce() -> T) -> T {
    ::stacker::maybe_grow(RED_ZONE, SEGMENT_SIZE, operation)
}

#[cfg(test)]
mod tests {
    #[rstest::rstest]
    #[case::returns(false)]
    #[case::unwinds(true)]
    fn grown_stack_preserves_thread_and_unwinding(#[case] panic: bool) {
        crate::test_helpers::on_small_stack(|| {
            let thread = std::thread::current().id();
            let dropped = std::cell::Cell::new(false);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // Force the production checkpoint to allocate a segment.
                ::stacker::grow(super::RED_ZONE / 2, || {
                    super::with_stack(|| {
                        let _guard = DropMarker(&dropped);
                        assert!(
                            ::stacker::remaining_stack().expect("supported stack backend")
                                >= super::RED_ZONE
                        );
                        assert_eq!(std::thread::current().id(), thread);
                        if panic {
                            panic!("panic on a grown stack");
                        }
                        42
                    })
                })
            }));
            assert!(dropped.get());
            if panic {
                assert!(result.is_err());
            } else {
                assert_eq!(result.expect("normal return"), 42);
            }
        });
    }

    struct DropMarker<'a>(&'a std::cell::Cell<bool>);

    impl Drop for DropMarker<'_> {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
}
