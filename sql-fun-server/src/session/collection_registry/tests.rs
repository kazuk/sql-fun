mod test_helpers;

use self::test_helpers::{BorrowedTrie, ObservedSource, registry};
use super::{HandleIdAllocator, MappedSource};
use std::sync::{Arc, atomic::AtomicU64};

use ::serde_json::Value;
use ::sql_fun_server_api::CollectionHandle;
use ::sql_fun_sqlast::TrieMap;

#[rstest::rstest]
#[case::first(0, Some(2), vec![0, 1])]
#[case::middle(2, Some(1), vec![2])]
#[case::clipped(3, Some(2), vec![3])]
#[case::exhausted(4, Some(1), vec![])]
#[case::beyond(usize::MAX, None, vec![])]
#[case::zero(0, Some(0), vec![])]
#[case::to_end(0, None, vec![0, 1])]
#[case::oversized(1, Some(usize::MAX), vec![1, 2])]
fn bounds_pages_and_preserves_metadata(
    #[case] offset: usize,
    #[case] count: Option<usize>,
    #[case] expected: Vec<usize>,
) {
    let mut registry = registry(2);
    let handle = registry
        .register(vec![0, 1, 2, 3], Vec::len, |value| {
            ::serde_json::json!(value)
        })
        .unwrap();
    let page = registry.read(&handle, offset, count).unwrap();
    assert_eq!(page.handle(), &handle);
    assert_eq!(*page.total_length(), 4);
    assert_eq!(*page.offset(), offset);
    assert!(*page.require_release());
    assert_eq!(
        ::serde_json::to_value(page.items()).unwrap(),
        ::serde_json::json!(expected)
    );
    assert_eq!(registry.len(), 1);
}

#[test]
fn reads_empty_collection() {
    let mut registry = registry(2);
    let handle = registry
        .register(Vec::<Value>::new(), Vec::len, Value::clone)
        .unwrap();
    let page = registry.read(&handle, 0, None).unwrap();
    assert_eq!(*page.total_length(), 0);
    assert!(page.items().is_empty());
    assert_eq!(registry.len(), 1);
}

#[test]
fn every_read_recreates_iterator_and_mapping_is_lazy() {
    let source = ObservedSource::new();
    let observation = source.observation();
    let mut registry = registry(2);
    let handle = registry
        .register(source, ObservedSource::len, ObservedSource::map)
        .unwrap();
    assert_eq!(observation.iterator_count(), 0);
    assert_eq!(observation.mapping_count(), 0);
    registry.read(&handle, 0, Some(1)).unwrap();
    assert_eq!(observation.mapping_count(), 1);
    for offset in [2, 0, 1, 2] {
        let page = registry.read(&handle, offset, Some(1)).unwrap();
        assert_eq!(page.items(), &[::serde_json::json!({"problem": offset})]);
    }
    assert_eq!(observation.iterator_count(), 5);
    assert_eq!(registry.len(), 1);
    registry.release(&handle).unwrap();
    assert!(observation.was_dropped());
    assert_eq!(registry.len(), 0);
}

#[test]
fn borrowed_trie_and_owned_results_use_the_same_registration_path() {
    let mut trie = TrieMap::new();
    for key in ["é", "z", "", "a", "あ"] {
        trie.insert(key, ::serde_json::json!({"table": key}));
    }
    let mut registry = registry(2);
    let borrowed = registry
        .register(BorrowedTrie::from(&trie), BorrowedTrie::len, Value::clone)
        .unwrap();
    let owned = registry
        .register(
            vec![::serde_json::json!({"problem": {"code": 7}})],
            Vec::len,
            Value::clone,
        )
        .unwrap();
    for (offset, expected) in [(3, "é"), (0, ""), (2, "z"), (1, "a"), (4, "あ"), (0, "")] {
        let page = registry.read(&borrowed, offset, Some(1)).unwrap();
        assert_eq!(page.items(), &[::serde_json::json!({"table": expected})]);
        assert_eq!(*page.total_length(), 5);
    }
    let problems = registry.read(&owned, 0, None).unwrap();
    assert_eq!(
        problems.items(),
        &[::serde_json::json!({"problem": {"code": 7}})]
    );
    registry.release(&borrowed).unwrap();
    assert!(registry.read(&borrowed, 0, None).is_err());
    assert!(registry.read(&owned, 0, None).is_ok());
}

#[test]
fn count_and_handle_authority_are_connection_local() {
    let mut first = registry(2);
    let mut second = registry(2);
    assert_eq!(first.len(), 0);
    assert_eq!(second.len(), 0);
    let first_handle = first
        .register(vec![1], Vec::len, |value| ::serde_json::json!(value))
        .unwrap();
    let next_handle = first
        .register(vec![2], Vec::len, |value| ::serde_json::json!(value))
        .unwrap();
    let second_handle = second
        .register(vec![3], Vec::len, |value| ::serde_json::json!(value))
        .unwrap();
    assert_ne!(first_handle, next_handle);
    assert_ne!(first_handle, second_handle);
    assert_eq!(first.len(), 2);
    assert_eq!(second.len(), 1);
    for invalid in [
        CollectionHandle::from(String::new()),
        CollectionHandle::from("unknown".to_owned()),
        second_handle,
    ] {
        assert!(first.read(&invalid, 0, None).is_err());
        assert!(first.release(&invalid).is_err());
        assert_eq!(first.len(), 2);
    }
    first.release(&first_handle).unwrap();
    assert_eq!(first.len(), 1);
    assert!(first.release(&first_handle).is_err());
    assert!(first.read(&first_handle, 0, None).is_err());
    assert_eq!(first.len(), 1);
    drop(first);
    let mut replacement = registry(2);
    let replacement_handle = replacement
        .register(vec![4], Vec::len, |value| ::serde_json::json!(value))
        .unwrap();
    assert_ne!(first_handle, replacement_handle);
    assert!(replacement.read(&first_handle, 0, None).is_err());
    assert!(second.read(&next_handle, 0, None).is_err());
}

#[test]
fn issuance_failure_drops_rejected_source_without_changing_count() {
    let mut registry = registry(2);
    let source = ObservedSource::new();
    let observation = source.observation();
    let ids = HandleIdAllocator(AtomicU64::new(u64::MAX));
    let source = MappedSource {
        source,
        length: ObservedSource::len,
        map: ObservedSource::map,
    };
    assert!(registry.register_source(Box::new(source), &ids).is_err());
    assert_eq!(registry.len(), 0);
    assert!(observation.was_dropped());
    assert_eq!(observation.iterator_count(), 0);
}

#[test]
fn teardown_drops_unreleased_sources() {
    let mut registry = registry(2);
    let source = ObservedSource::new();
    let observation = source.observation();
    registry
        .register(source, ObservedSource::len, ObservedSource::map)
        .unwrap();
    drop(registry);
    assert!(observation.was_dropped());
}

#[rstest::rstest]
#[case::normal(false)]
#[case::cancelled(true)]
#[tokio::test]
async fn schema_borrows_survive_growth_and_drop_before_owners(#[case] cancel: bool) {
    use self::test_helpers::SchemaColumns;
    use crate::{schema_cache::test_helpers::SchemaProject, session::SessionWrapper};
    use std::sync::atomic::{AtomicBool, Ordering};

    // The existing synthetic catalog fixture avoids full builtin parsing/services.
    let project = SchemaProject::new();
    let service = project.service();
    let schema = project.initialized_schema(&service).await;
    let owner = Arc::downgrade(&schema);
    let wrapper = SessionWrapper::new();
    let context = wrapper.contexts.register(&schema).unwrap();
    drop(service);
    drop(schema);
    let dropped = Arc::new(AtomicBool::new(false));
    let source_dropped = Arc::clone(&dropped);
    let (ready, started) = ::tokio::sync::oneshot::channel();
    let (finish, finished) = ::tokio::sync::oneshot::channel();
    let schema_owner = owner.clone();
    let task = ::tokio::spawn(async move {
        let schema = wrapper.contexts.resolve(&context).unwrap();
        let mut collections = registry(2);
        let source = SchemaColumns::new(schema, &source_dropped);
        let handle = collections
            .register(source, SchemaColumns::len, SchemaColumns::map)
            .unwrap();
        for _ in 0..256 {
            wrapper
                .contexts
                .register(&schema_owner.upgrade().unwrap())
                .unwrap();
            collections
                .register(vec![::serde_json::json!("other")], Vec::len, Value::clone)
                .unwrap();
        }
        ready.send(()).unwrap();
        finished.await.unwrap();
        let page = collections.read(&handle, 0, None).unwrap();
        assert_eq!(
            page.items(),
            &[::serde_json::json!("id"), ::serde_json::json!("value")]
        );
    });
    started.await.unwrap();
    assert!(!dropped.load(Ordering::Relaxed));
    if cancel {
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    } else {
        finish.send(()).unwrap();
        task.await.unwrap();
    }
    assert!(dropped.load(Ordering::Relaxed));
    assert!(owner.upgrade().is_none());
}
