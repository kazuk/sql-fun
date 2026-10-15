mod test_helpers;

use self::test_helpers::SchemaColumns;
use super::Session;
use crate::{
    Api,
    schema_cache::test_helpers::SchemaProject,
    session::{SessionState, SessionWrapper},
};
use std::{
    num::NonZeroUsize,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use ::serde_json::Value;

#[rstest::rstest]
#[case::normal(false, false)]
#[case::released(false, true)]
#[case::cancelled(true, false)]
#[tokio::test]
async fn schema_borrows_survive_growth_and_drop_before_owners(
    #[case] cancel: bool,
    #[case] release: bool,
) {
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
        let mut state = SessionState::new(NonZeroUsize::new(2).unwrap());
        let mut session = Session::new(&wrapper.contexts, &mut state);
        let schema = session.context(&context).unwrap();
        assert_eq!(session.collection_count(), 0);
        let source = SchemaColumns::new(schema, &source_dropped);
        let handle = session
            .register_collection(source, SchemaColumns::len, SchemaColumns::map)
            .unwrap();
        for _ in 0..256 {
            session
                .register_context(&schema_owner.upgrade().unwrap())
                .unwrap();
            session
                .register_collection(vec![::serde_json::json!("other")], Vec::len, Value::clone)
                .unwrap();
        }
        assert_eq!(session.collection_count(), 257);
        let foreign_session = SessionWrapper::new();
        let mut other_state = SessionState::new(NonZeroUsize::new(2).unwrap());
        let other = Session::new(&foreign_session.contexts, &mut other_state);
        assert!(other.context(&context).is_none());
        assert_eq!(other.collection_count(), 0);
        ready.send(()).unwrap();
        finished.await.unwrap();
        let args = ::serde_json::from_value(::serde_json::json!({
            "collection_handle":handle, "start_offset":0, "count":null
        }))
        .unwrap();
        let page = session.read_collection(&args).unwrap();
        assert_eq!(page["items"], ::serde_json::json!(["id", "value"]));
        if release {
            let release_args =
                ::serde_json::from_value(::serde_json::json!({"collection_handle":handle}))
                    .unwrap();
            session.release_collection(&release_args).unwrap();
            assert_eq!(session.collection_count(), 256);
            assert!(session.read_collection(&args).is_err());
            assert!(session.release_collection(&release_args).is_err());
            assert_eq!(session.collection_count(), 256);
        }
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

#[tokio::test]
async fn separate_facades_share_analysis_but_cannot_resolve_each_others_contexts() {
    let project = SchemaProject::new();
    let api = Api {
        schema_cache: OnceLock::new(),
    };
    let args =
        ::serde_json::from_value(project.rpc_request("CREATE TABLE retained (id int4);")).unwrap();
    let first = SessionWrapper::new();
    let second = SessionWrapper::new();
    let limit = NonZeroUsize::new(2).unwrap();
    let mut first_state = SessionState::new(limit);
    let mut second_state = SessionState::new(limit);
    let mut one = Session::new(&first.contexts, &mut first_state);
    let mut two = Session::new(&second.contexts, &mut second_state);
    let first_response = one.initialize_schema_context(&api, &args).await.unwrap();
    let second_response = two.initialize_schema_context(&api, &args).await.unwrap();
    let first_handle = first_response.context_handle().as_ref().unwrap();
    let second_handle = second_response.context_handle().as_ref().unwrap();
    assert!(one.context(second_handle).is_none());
    assert!(two.context(first_handle).is_none());
    assert!(std::ptr::eq(
        one.context(first_handle).unwrap().analysis(),
        two.context(second_handle).unwrap().analysis(),
    ));
    assert_eq!(one.collection_count(), 1);
    assert_eq!(two.collection_count(), 1);
}

#[tokio::test]
async fn failed_initialization_registers_no_collection() {
    let project = SchemaProject::new();
    let api = Api {
        schema_cache: OnceLock::new(),
    };
    let mut request = project.rpc_request("CREATE TABLE retained (id int4);");
    request["expected_hash"] = ::serde_json::json!("0".repeat(64));
    let wrapper = SessionWrapper::new();
    let mut state = SessionState::new(NonZeroUsize::new(2).unwrap());
    let mut session = Session::new(&wrapper.contexts, &mut state);
    assert!(
        session
            .initialize_schema_context(&api, &::serde_json::from_value(request).unwrap())
            .await
            .is_err()
    );
    assert_eq!(session.collection_count(), 0);
}
