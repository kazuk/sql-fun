use super::ContextHandleRegistry;
use crate::{schema_cache::test_helpers::SchemaProject, session::SessionWrapper};
use std::sync::Arc;

use ::sql_fun_server_api::payloads::ContextHandle;
use ::sql_fun_sqlast::sem::{BaseParseContext, FullName, TableName};

#[tokio::test]
async fn registration_resolves_original_schema_and_rejects_unknown_handles() {
    let project = SchemaProject::new();
    let service = project.service();
    let schema = project.initialized_schema(&service).await;
    let registry = ContextHandleRegistry::new();
    let handle = registry.register(&schema).unwrap();

    let resolved = registry.resolve(&handle).unwrap();
    assert!(std::ptr::eq(resolved, schema.as_ref()));
    assert!(std::ptr::eq(resolved.base_context(), schema.base_context()));
    assert!(
        registry
            .resolve(&ContextHandle::from("unregistered".to_owned()))
            .is_none()
    );
    assert!(
        registry
            .resolve(&ContextHandle::from(String::new()))
            .is_none()
    );
}

#[tokio::test]
async fn repeated_registration_retains_the_same_schema_under_distinct_handles() {
    let project = SchemaProject::new();
    let service = project.service();
    let schema = project.initialized_schema(&service).await;
    let registry = ContextHandleRegistry::new();
    let first = registry.register(&schema).unwrap();
    let second = registry.register(&schema).unwrap();

    assert!(first != second);
    assert!(std::ptr::eq(
        registry.resolve(&first).unwrap(),
        registry.resolve(&second).unwrap()
    ));
    assert_eq!(Arc::strong_count(&schema), 3);
    drop(registry);
    assert_eq!(Arc::strong_count(&schema), 1);
}

#[tokio::test]
async fn wrappers_share_schemas_without_sharing_handle_authority() {
    let project = SchemaProject::new();
    let service = project.service();
    let schema = project.initialized_schema(&service).await;
    let first = SessionWrapper::new();
    let second = SessionWrapper::new();
    let first_handle = first.contexts.register(&schema).unwrap();
    let second_handle = second.contexts.register(&schema).unwrap();

    assert!(first_handle != second_handle);
    assert!(first.contexts.resolve(&second_handle).is_none());
    assert!(second.contexts.resolve(&first_handle).is_none());
    assert!(std::ptr::eq(
        first
            .contexts
            .resolve(&first_handle)
            .unwrap()
            .base_context(),
        second
            .contexts
            .resolve(&second_handle)
            .unwrap()
            .base_context()
    ));
    drop(first);
    let replacement = SessionWrapper::new();
    let replacement_handle = replacement.contexts.register(&schema).unwrap();
    assert!(replacement_handle != first_handle);
    assert!(replacement.contexts.resolve(&first_handle).is_none());
    assert!(second.contexts.resolve(&replacement_handle).is_none());
}

#[tokio::test]
async fn live_schema_and_cursor_borrows_survive_registration_growth() {
    let project = SchemaProject::new();
    let service = project.service();
    let schema = project.initialized_schema(&service).await;
    let wrapper = SessionWrapper::new();
    let handle = wrapper.contexts.register(&schema).unwrap();
    let retained = wrapper.contexts.resolve(&handle).unwrap();
    let table_name = TableName::from(FullName::with_schema("public", "retained"));
    let table = retained.base_context().get_table(&table_name).unwrap();
    // Stand in for a future collection cursor with an iterator borrowing real schema objects.
    let mut columns = (0..2).map(|index| table.get_column_at(index).unwrap());
    let first_column = columns.next().unwrap();

    // Grow one registry while its original schema references and cursor remain live.
    for _ in 0..256 {
        let next = wrapper.contexts.register(&schema).unwrap();
        assert!(next != handle);
    }
    assert!(columns.next().unwrap().name().is_some());
    assert!(columns.next().is_none());
    assert!(std::ptr::eq(first_column, table.get_column_at(0).unwrap()));
    assert!(std::ptr::eq(
        retained,
        wrapper.contexts.resolve(&handle).unwrap()
    ));
}

#[tokio::test]
async fn registrations_survive_cache_drop_and_release_the_final_owner_on_teardown() {
    let project = SchemaProject::new();
    let service = project.service();
    let schema = project.initialized_schema(&service).await;
    let weak = Arc::downgrade(&schema);
    let first = SessionWrapper::new();
    let second = SessionWrapper::new();
    let first_handle = first.contexts.register(&schema).unwrap();
    let second_handle = second.contexts.register(&schema).unwrap();
    // The cache owns the inner analysis, while these Arcs own the initialized result.
    drop(schema);
    drop(service);
    assert_eq!(weak.strong_count(), 2);
    let table_name = TableName::from(FullName::with_schema("public", "retained"));
    assert!(
        first
            .contexts
            .resolve(&first_handle)
            .unwrap()
            .base_context()
            .get_table(&table_name)
            .is_some()
    );
    drop(first);
    assert_eq!(weak.strong_count(), 1);
    assert!(
        second
            .contexts
            .resolve(&second_handle)
            .unwrap()
            .base_context()
            .get_table(&table_name)
            .is_some()
    );
    drop(second);
    assert!(weak.upgrade().is_none());
}

#[tokio::test]
async fn cancelling_a_connection_task_drops_borrowed_state_before_schema_owners() {
    fn assert_shareable<T: Send + Sync>() {}
    assert_shareable::<ContextHandleRegistry>();
    assert_shareable::<SessionWrapper>();

    let project = SchemaProject::new();
    let service = project.service();
    let schema = project.initialized_schema(&service).await;
    let weak = Arc::downgrade(&schema);
    let wrapper = SessionWrapper::new();
    let handle = wrapper.contexts.register(&schema).unwrap();
    drop(schema);
    drop(service);

    let (ready, started) = ::tokio::sync::oneshot::channel();
    let task = ::tokio::spawn(async move {
        let schema = wrapper.contexts.resolve(&handle).unwrap();
        let table_name = TableName::from(FullName::with_schema("public", "retained"));
        let table = schema.base_context().get_table(&table_name).unwrap();
        let mut columns = (0..2).map(|index| table.get_column_at(index).unwrap());
        ready.send(()).unwrap();
        std::future::pending::<()>().await;
        // Keeping the borrow live across await also verifies that the future is Send.
        assert!(columns.next().is_some());
    });
    started.await.unwrap();
    assert_eq!(weak.strong_count(), 1);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(weak.upgrade().is_none());
}

#[tokio::test]
async fn sessions_share_a_schema_without_sharing_rewrite_policy() {
    let project = SchemaProject::new();
    let service = project.service();
    let schema = project.initialized_schema(&service).await;
    let enabled: ::sql_fun_core::RewriteOptions =
        ::serde_json::from_value(::serde_json::json!({"expand_asterisk": true})).unwrap();
    let disabled: ::sql_fun_core::RewriteOptions =
        ::serde_json::from_value(::serde_json::json!({"expand_asterisk": false})).unwrap();
    let first = SessionWrapper::new();
    let second = SessionWrapper::new();
    let first_handle = first
        .contexts
        .register_with_options(&schema, &enabled)
        .unwrap();
    let second_handle = second
        .contexts
        .register_with_options(&schema, &disabled)
        .unwrap();
    let omitted = second.contexts.register(&schema).unwrap();

    assert!(std::ptr::eq(
        first.contexts.resolve(&first_handle).unwrap(),
        second.contexts.resolve(&second_handle).unwrap(),
    ));
    assert_eq!(
        first
            .contexts
            .rewrite_options(&first_handle)
            .unwrap()
            .expand_asterisk(),
        Some(true)
    );
    assert_eq!(
        second
            .contexts
            .rewrite_options(&second_handle)
            .unwrap()
            .expand_asterisk(),
        Some(false)
    );
    assert_eq!(
        second
            .contexts
            .rewrite_options(&omitted)
            .unwrap()
            .expand_asterisk(),
        None
    );
    assert!(first.contexts.rewrite_options(&second_handle).is_none());
    assert!(second.contexts.rewrite_options(&first_handle).is_none());
    drop(second);
    assert_eq!(
        first
            .contexts
            .rewrite_options(&first_handle)
            .unwrap()
            .expand_asterisk(),
        Some(true)
    );
}
