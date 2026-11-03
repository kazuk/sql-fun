mod test_helpers;

use self::test_helpers::SchemaColumns;
use crate::{
    Api,
    schema_cache::test_helpers::SchemaProject,
    session::{SessionState, SessionWrapper, facade::Session},
};
use std::{
    num::NonZeroUsize,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

#[tokio::test]
async fn schema_registrations_own_separate_reusable_analysis_caches() {
    let project = SchemaProject::new();
    let service = project.service();
    let schema = project.initialized_schema(&service).await;
    let wrapper = SessionWrapper::new();
    let first = wrapper.contexts.register(&schema).unwrap();
    let second = wrapper.contexts.register(&schema).unwrap();
    let mut state = SessionState::new(NonZeroUsize::new(2).unwrap());
    {
        let mut session = Session::new(&wrapper.contexts, &mut state);
        // Complete both insertions before comparing borrows, so map growth
        // cannot invalidate the addresses used in this ownership assertion.
        session.cache_context(&first);
        session.cache_context(&second);
    }
    assert_eq!(state.cache_contexts.len(), 2);
    let first_cache = &state.cache_contexts[&first];
    let second_cache = &state.cache_contexts[&second];
    assert!(!std::ptr::eq(first_cache, second_cache));
    let mut session = Session::new(&wrapper.contexts, &mut state);
    let first_cache = std::ptr::from_ref(session.cache_context(&first));
    // A new request facade retains the same session-owned cache for this handle.
    let mut next = Session::new(&wrapper.contexts, &mut state);
    assert!(std::ptr::eq(first_cache, next.cache_context(&first)));
    assert_eq!(state.cache_contexts.len(), 2);
}

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
                .register_collection(
                    vec![::serde_json::json!("other")],
                    Vec::len,
                    ::serde_json::Value::clone,
                )
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

#[tokio::test]
async fn retained_presentation_survives_source_file_changes() {
    let project = SchemaProject::new();
    let api = Api {
        schema_cache: OnceLock::new(),
    };
    let original = "CREATE TABLE retained (id int4);\nCREATE TABLE retained (id int4);";
    let args: ::sql_fun_server_api::payloads::InitializeSchemaArgs =
        ::serde_json::from_value(project.rpc_request(original)).unwrap();
    let wrapper = SessionWrapper::new();
    let mut state = SessionState::new(NonZeroUsize::new(2).unwrap());
    let mut session = Session::new(&wrapper.contexts, &mut state);
    let response = session
        .initialize_schema_context(&api, &args)
        .await
        .unwrap();
    // Presentation must use the analyzed statement, even after file replacement.
    std::fs::write(args.schema_dump().as_ref().unwrap(), "SELECT 42;").unwrap();
    let value = ::serde_json::to_value(response).unwrap();
    let items = value["problems"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    let presentation = &items[0]["presentation"];
    assert!(
        presentation["source"]
            .as_str()
            .unwrap()
            .contains("CREATE TABLE retained")
    );
    assert_eq!(presentation["dialect"], "postgres");
    assert!(presentation["statement_offset"].as_u64().unwrap() > 0);
    assert!(presentation["problem"].is_object());
}

#[rstest::rstest]
#[case::empty("")]
#[case::whitespace(" \t\r\n")]
#[case::unicode(" SELECT '日本語', 名 + 1; -- comment\r\n")]
#[case::multiline("/* first\n日本語\nlast */ SELECT 'two\nlines'; ")]
#[case::syntax_error("SELECT FROM (")]
#[case::semantic_error("SELECT missing FROM unknown_table")]
#[tokio::test]
async fn tokenization_pages_match_postgres_without_schema(#[case] source: &str) {
    verify_tokenization(source, ::sql_fun_core::SqlDialect::PostgreSQL).await;
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::empty("")]
#[case::whitespace(" \t\r\n")]
#[case::unicode(" SELECT '日本語', 名 + 1; -- comment\r\n")]
#[case::multiline("/* first\n日本語\nlast */ SELECT 'two\nlines'; ")]
#[case::syntax_error("SELECT FROM (")]
#[case::semantic_error("SELECT missing FROM unknown_table")]
#[tokio::test]
async fn tokenization_pages_match_sqlite_without_schema(#[case] source: &str) {
    verify_tokenization(source, ::sql_fun_core::SqlDialect::SQLite).await;
}

async fn verify_tokenization(source: &str, dialect: ::sql_fun_core::SqlDialect) {
    use ::sql_fun_server_api::payloads::{SqlToken, TokenizeSqlArgs};
    let wrapper = SessionWrapper::new();
    let mut state = SessionState::new(NonZeroUsize::new(2).unwrap());
    let mut session = Session::new(&wrapper.contexts, &mut state);
    let api = Api {
        schema_cache: OnceLock::new(),
    };
    let request = ::serde_json::from_value(::serde_json::json!({
        "jsonrpc":"2.0","id":1,"method":"tokenize_sql",
        "params":TokenizeSqlArgs::new(source, dialect)
    }))
    .unwrap();
    let first = session
        .process_request(&api, &request)
        .await
        .unwrap_or_else(|_| panic!("tokenize request failed"));
    let handle = first["handle"].clone();
    let mut items = first["items"].as_array().unwrap().clone();
    let total = usize::try_from(first["total_length"].as_u64().unwrap()).unwrap();
    while items.len() < total {
        let args = ::serde_json::from_value(::serde_json::json!({
            "collection_handle":handle, "start_offset":items.len(), "count":null
        }))
        .unwrap();
        let page = session.read_collection(&args).unwrap();
        let next = page["items"].as_array().unwrap();
        assert_ne!(next.len(), 0);
        assert!(next.len() <= 2);
        items.extend_from_slice(next);
    }
    let tokens: Vec<SqlToken> =
        ::serde_json::from_value(::serde_json::Value::Array(items)).unwrap();
    let expected = ::sql_fun_sqlast::tokenize_sql(source, dialect).unwrap();
    let expected: Vec<_> = expected.tokens().map(SqlToken::from).collect();
    assert_eq!(tokens, expected);
    assert_eq!(session.collection_count(), 1);
    // Reading the last page does not release the collection.
    let release =
        ::serde_json::from_value(::serde_json::json!({"collection_handle":handle})).unwrap();
    session.release_collection(&release).unwrap();
    assert_eq!(session.collection_count(), 0);
    assert!(session.release_collection(&release).is_err());
}

#[tokio::test]
async fn lexical_failure_is_a_distinct_api_error_without_retained_tokens() {
    verify_lexical_failure(::sql_fun_core::SqlDialect::PostgreSQL).await;
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn sqlite_lexical_failure_is_a_distinct_api_error() {
    verify_lexical_failure(::sql_fun_core::SqlDialect::SQLite).await;
}

async fn verify_lexical_failure(dialect: ::sql_fun_core::SqlDialect) {
    let wrapper = SessionWrapper::new();
    let mut state = SessionState::new(NonZeroUsize::new(2).unwrap());
    let mut session = Session::new(&wrapper.contexts, &mut state);
    let api = Api {
        schema_cache: OnceLock::new(),
    };
    let request = ::serde_json::from_value(::serde_json::json!({
        "jsonrpc":"2.0","id":1,"method":"tokenize_sql",
        "params":{"source":"SELECT 'unterminated", "dialect":dialect}
    }))
    .unwrap();
    let error = session.process_request(&api, &request).await.err().unwrap();
    let wire = ::serde_json::to_value(error).unwrap();
    assert!(
        wire["data"]["TokenizationFailed"]
            .as_str()
            .unwrap()
            .contains("cannot tokenize SQL")
    );
    assert!(
        wire["message"]
            .as_str()
            .unwrap()
            .starts_with("SQL tokenization failed:")
    );
    assert_eq!(session.collection_count(), 0);
}

#[tokio::test]
async fn long_sql_is_not_truncated() {
    let source = "-- repeated comment\n".repeat(80);
    verify_tokenization(&source, ::sql_fun_core::SqlDialect::PostgreSQL).await;
    #[cfg(feature = "sqlite")]
    verify_tokenization(&source, ::sql_fun_core::SqlDialect::SQLite).await;
}
