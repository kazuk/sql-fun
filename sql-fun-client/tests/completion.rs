//! Typed materialization and shared-transport integration with a bounded wire peer.

mod completion_peer;

use self::completion_peer::{Fault, Peer, Scenario};
use ::sql_fun_client::{Collection, CompleteResponse};
use std::time::Duration;

#[rstest::rstest]
#[case::empty(0, 0, 0, true)]
#[case::complete(3, 0, 3, true)]
#[case::unretained(3, 0, 3, false)]
#[case::multiple_pages(7, 0, 1, true)]
#[case::exact_boundary(6, 0, 2, true)]
#[case::nonzero(7, 3, 2, true)]
#[case::empty_initial(4, 0, 0, true)]
#[tokio::test]
async fn completes_in_order_before_delivering_and_finishing(
    #[case] total: usize,
    #[case] offset: usize,
    #[case] initial: usize,
    #[case] release: bool,
) -> ::testresult::TestResult {
    let mut peer = Peer::start(Scenario::single(total, offset, initial, release)).await?;
    let (sent, received) = ::tokio::sync::oneshot::channel();
    peer.client()
        .invoke_typed(
            "probe",
            &::serde_json::json!({}),
            move |response: Result<Collection<u32>, _>| async move {
                let value = response.unwrap();
                assert!(value.is_completed());
                assert!(value.handle().is_none());
                assert_eq!(
                    value.items(),
                    &(0..u32::try_from(total).unwrap()).collect::<Vec<_>>()
                );
                let _ = sent.send(());
                Ok(())
            },
        )
        .await?;
    let log = ::tokio::time::timeout(Duration::from_secs(5), peer.finish()).await??;
    received.await?;
    assert_eq!(log.releases(), usize::from(release));
    log.assert_unique_ids();
    log.assert_initial_range_not_refetched(offset, initial);
    Ok(())
}

#[rstest::rstest]
#[case::no_progress(Fault::no_progress())]
#[case::wrong_handle(Fault::handle())]
#[case::wrong_offset(Fault::offset())]
#[case::wrong_length(Fault::length())]
#[case::release_flag(Fault::release_flag())]
#[case::wrong_type(Fault::item_type())]
#[case::read_error(Fault::read_error())]
#[case::release_error(Fault::release_error())]
#[case::malformed(Fault::malformed())]
#[case::disconnect(Fault::disconnect())]
#[case::oversized_page(Fault::too_many())]
#[case::initial_bounds(Fault::initial_bounds())]
#[case::initial_overflow(Fault::initial_overflow())]
#[case::cleanup_failure(Fault::cleanup_error())]
#[case::release_type(Fault::release_type())]
#[tokio::test]
async fn failures_deliver_one_error_and_attempt_safe_cleanup(
    #[case] fault: Fault,
) -> ::testresult::TestResult {
    let mut peer = Peer::start(Scenario::single(5, 0, 1, true).fault(fault)).await?;
    let (sent, received) = ::tokio::sync::oneshot::channel();
    peer.client()
        .invoke_typed(
            "probe",
            &::serde_json::json!({}),
            move |response: Result<Collection<u32>, _>| async move {
                let error = response.err().unwrap();
                if fault == Fault::cleanup_error() {
                    assert!(error.to_string().contains("cleanup failed"));
                }
                let _ = sent.send(());
                Ok(())
            },
        )
        .await?;
    let (finish, log) =
        ::tokio::time::timeout(Duration::from_secs(5), peer.finish_allow_failure()).await?;
    received.await?;
    if fault.breaks_connection() {
        assert!(finish.is_err());
        assert_eq!(log.releases(), 0);
    } else {
        finish?;
        assert_eq!(log.releases(), 1);
    }
    Ok(())
}

#[tokio::test]
async fn saturated_normal_admission_completes_three_collections_per_response()
-> ::testresult::TestResult {
    let mut peer = Peer::start(Scenario::saturated()).await?;
    let (sent, mut received) = ::tokio::sync::mpsc::unbounded_channel();
    for _ in 0..8 {
        let sent = sent.clone();
        peer.client()
            .invoke_typed(
                "probe",
                &::serde_json::json!({}),
                move |response: Result<Vec<Collection<u32>>, _>| async move {
                    let values = response.unwrap();
                    assert_eq!(values.len(), 3);
                    for value in values {
                        assert!(value.is_completed());
                        assert_eq!(value.items(), &[0, 1, 2, 3, 4]);
                    }
                    let _ = sent.send(());
                    Ok(())
                },
            )
            .await?;
    }
    let log = ::tokio::time::timeout(Duration::from_secs(5), peer.finish()).await??;
    for _ in 0..8 {
        received.recv().await.unwrap();
    }
    assert_eq!(log.releases(), 24);
    assert_eq!(log.reads(), 48);
    log.assert_unique_ids();
    Ok(())
}

#[tokio::test]
async fn nested_optional_collections_and_opaque_json_are_type_directed() -> ::testresult::TestResult
{
    let scenario = Scenario::nested();
    let mut peer = Peer::start(scenario).await?;
    let (sent, received) = ::tokio::sync::oneshot::channel();
    peer.client()
        .invoke_typed(
            "probe",
            &::serde_json::json!({}),
            move |response: Result<Option<Box<Collection<Collection<u32>>>>, _>| async move {
                let value = response.unwrap().unwrap();
                assert!(value.is_completed());
                assert_eq!(value.items().len(), 2);
                for item in value.items() {
                    assert!(item.is_completed());
                    assert_eq!(item.items(), &[0, 1, 2]);
                }
                let _ = sent.send(());
                Ok(())
            },
        )
        .await?;
    let log = ::tokio::time::timeout(Duration::from_secs(5), peer.finish()).await??;
    received.await?;
    assert_eq!(log.releases(), 3);
    Ok(())
}

#[tokio::test]
async fn no_collection_response_is_delivered_without_internal_requests() -> ::testresult::TestResult
{
    let mut peer = Peer::start(Scenario::opaque()).await?;
    let (sent, received) = ::tokio::sync::oneshot::channel();
    peer.client()
        .invoke_typed(
            "probe",
            &::serde_json::json!({}),
            move |response: Result<::serde_json::Value, _>| async move {
                assert!(response.unwrap().get("handle").is_some());
                let _ = sent.send(());
                Ok(())
            },
        )
        .await?;
    let log = ::tokio::time::timeout(Duration::from_secs(5), peer.finish()).await??;
    received.await?;
    assert_eq!(log.reads() + log.releases(), 0);
    Ok(())
}

// Ensure the public trait can express user-defined composition without transport access.
#[derive(::serde::Deserialize)]
struct CustomResponse(Vec<Collection<u32>>);
impl CompleteResponse for CustomResponse {
    fn register_collections(&self, context: &mut ::sql_fun_client::CompletionContext) {
        self.0.register_collections(context);
    }
    fn complete<'a>(
        &'a mut self,
        context: &'a mut ::sql_fun_client::CompletionContext,
    ) -> ::sql_fun_client::CompletionFuture<'a> {
        Box::pin(async move {
            self.0.complete(context).await?;
            self.0.complete(context).await
        })
    }
}

#[tokio::test]
async fn failure_cleans_unvisited_sibling_handles() -> ::testresult::TestResult {
    let mut peer = Peer::start(Scenario::siblings().fault(Fault::read_error())).await?;
    let (sent, received) = ::tokio::sync::oneshot::channel();
    peer.client()
        .invoke_typed(
            "probe",
            &::serde_json::json!({}),
            move |response: Result<CustomResponse, _>| async move {
                assert!(response.is_err());
                let _ = sent.send(());
                Ok(())
            },
        )
        .await?;
    let log = ::tokio::time::timeout(Duration::from_secs(5), peer.finish()).await??;
    received.await?;
    assert_eq!(log.releases(), 3);
    Ok(())
}

#[tokio::test]
async fn initial_server_error_is_delivered_without_internal_rpc() -> ::testresult::TestResult {
    let mut peer =
        Peer::start(Scenario::single(5, 0, 1, true).fault(Fault::initial_error())).await?;
    let (sent, received) = ::tokio::sync::oneshot::channel();
    peer.client()
        .invoke_typed(
            "probe",
            &::serde_json::json!({}),
            move |response: Result<Collection<u32>, _>| async move {
                let _ = sent.send(response.is_err());
                Ok(())
            },
        )
        .await?;
    let log = ::tokio::time::timeout(Duration::from_secs(5), peer.finish()).await??;
    assert!(received.await?);
    assert_eq!(log.reads() + log.releases(), 0);
    Ok(())
}

#[tokio::test]
async fn raw_response_keeps_manual_paging_contract() -> ::testresult::TestResult {
    let mut peer = Peer::start(Scenario::single(5, 0, 1, true)).await?;
    let (sent, received) = ::tokio::sync::oneshot::channel();
    peer.client()
        .invoke_method(
            "probe",
            &::serde_json::json!({}),
            move |response| async move {
                let page: Collection<u32> =
                    ::serde_json::from_value(response.into_result().unwrap()).unwrap();
                assert!(!page.is_completed());
                assert_eq!(page.items(), &[0]);
                let _ = sent.send(());
                Ok(())
            },
        )
        .await?;
    let log = ::tokio::time::timeout(Duration::from_secs(5), peer.finish()).await??;
    received.await?;
    assert_eq!(log.reads() + log.releases(), 0);
    Ok(())
}

#[tokio::test]
async fn dropping_client_cancels_stalled_completion_without_detached_cleanup()
-> ::testresult::TestResult {
    let mut peer = Peer::start(Scenario::single(5, 0, 1, true).fault(Fault::stall_read())).await?;
    let (sent, received) = ::tokio::sync::oneshot::channel();
    peer.client()
        .invoke_typed(
            "probe",
            &::serde_json::json!({}),
            move |response: Result<Collection<u32>, _>| async move {
                let _ = sent.send(response.is_ok());
                Ok(())
            },
        )
        .await?;
    ::tokio::time::timeout(Duration::from_secs(5), peer.wait_for_read()).await?;
    let log = ::tokio::time::timeout(Duration::from_secs(5), peer.drop_connection()).await?;
    assert!(received.await.is_err());
    assert_eq!(log.reads(), 1);
    assert_eq!(log.releases(), 0);
    Ok(())
}

#[tokio::test]
async fn custom_response_completion_is_idempotent() -> ::testresult::TestResult {
    let mut peer = Peer::start(Scenario::siblings()).await?;
    let (sent, received) = ::tokio::sync::oneshot::channel();
    peer.client()
        .invoke_typed(
            "probe",
            &::serde_json::json!({}),
            move |response: Result<CustomResponse, _>| async move {
                let values = response.unwrap().0;
                for value in values {
                    assert!(value.is_completed());
                }
                let _ = sent.send(());
                Ok(())
            },
        )
        .await?;
    let log = ::tokio::time::timeout(Duration::from_secs(5), peer.finish()).await??;
    received.await?;
    assert_eq!(log.releases(), 3);
    assert_eq!(log.reads(), 6);
    Ok(())
}

#[tokio::test]
async fn copied_api_response_completes_every_nested_analysis_field() -> ::testresult::TestResult {
    let mut peer = Peer::start(Scenario::analyzed()).await?;
    let (sent, received) = ::tokio::sync::oneshot::channel();
    peer.client().invoke_typed("probe", &::serde_json::json!({}), move |response: Result<::sql_fun_client::payloads::AnalyzeQueryResponse, _>| async move {
        let response = response.unwrap();
        assert!(response.queries().is_completed());
        let item = &response.queries().items()[0];
        assert!(item.probrems().is_completed());
        assert!(item.accessing_relations().is_completed());
        assert!(item.column_access().is_completed());
        assert!(item.parameters().is_completed());
        assert!(item.result_columns().is_completed());
        let _ = sent.send(());
        Ok(())
    }).await?;
    let log = ::tokio::time::timeout(Duration::from_secs(5), peer.finish()).await??;
    received.await?;
    assert_eq!(log.releases(), 6);
    assert_eq!(log.reads(), 0);
    Ok(())
}
