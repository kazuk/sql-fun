use super::Conversation;
use crate::init::{
    InitializationState,
    step::{Direction, InitStep, StepCompletion, StepFuture},
    test_helpers::{common_args, state},
};
use std::{cell::RefCell, rc::Rc};

struct RecordStep(&'static str, Rc<RefCell<Vec<&'static str>>>);
impl InitStep for RecordStep {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            self.1.borrow_mut().push(self.0);
            state.prepare_sqlite();
            Ok(direction.completion())
        })
    }
}

#[rstest::rstest]
#[case::forward(["a", "b", "new"])]
#[case::reordered(["new", "b", "a"])]
#[tokio::test]
async fn registration_alone_defines_order_and_updates_shared_state(
    #[case] order: [&'static str; 3],
) {
    let common = common_args();
    let mut state = state(&common, &[]);
    let log = Rc::new(RefCell::new(Vec::new()));
    let steps = order
        .into_iter()
        .map(|name| Box::new(RecordStep(name, log.clone())) as Box<dyn InitStep>)
        .collect();
    Conversation::new(steps)
        .run(&mut state, Direction::forward())
        .await
        .unwrap();
    assert_eq!(*log.borrow(), order);
    assert_eq!(state.plans.len(), 3);
    assert!(
        state.versions.is_none(),
        "engine must not invent missing settings"
    );
}

struct StopStep(bool);
impl InitStep for StopStep {
    fn prompt<'a>(
        &'a mut self,
        _state: &'a mut InitializationState<'_>,
        _direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            if self.0 {
                Err(anyhow::anyhow!("step failed"))
            } else {
                Ok(StepCompletion::cancelled())
            }
        })
    }
}

#[rstest::rstest]
#[case::error(true)]
#[case::cancelled(false)]
#[tokio::test]
async fn stopping_a_step_prevents_later_execution(#[case] error: bool) {
    let common = common_args();
    let mut state = state(&common, &[]);
    let log = Rc::new(RefCell::new(Vec::new()));
    let completion = Conversation::new(vec![
        Box::new(StopStep(error)),
        Box::new(RecordStep("later", log.clone())),
    ])
    .run(&mut state, Direction::forward())
    .await;
    assert_eq!(completion.is_err(), error);
    if let Ok(completion) = completion {
        assert!(completion.is_cancelled());
    }
    assert!(log.borrow().is_empty());
    assert!(state.plans.is_empty());
}

#[rstest::rstest]
#[case::forward(Direction::forward())]
#[case::backward(Direction::backward())]
#[tokio::test]
async fn empty_conversation_preserves_entry_direction_without_changing_state(
    #[case] direction: Direction,
) {
    let common = common_args();
    let mut state = state(&common, &[]);
    let completion = Conversation::new(vec![])
        .run(&mut state, direction)
        .await
        .unwrap();
    assert!(!completion.is_cancelled());
    assert_eq!(completion.is_back(), direction.is_backward());
    assert!(state.selection.is_none());
    assert!(state.plans.is_empty());
}

struct BranchStep(bool, Rc<RefCell<Vec<&'static str>>>);
impl InitStep for BranchStep {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            let names = if self.0 {
                vec!["child a", "child b"]
            } else {
                vec!["other child"]
            };
            let children = names
                .into_iter()
                .map(|name| Box::new(RecordStep(name, self.1.clone())) as Box<dyn InitStep>)
                .collect();
            Conversation::new(children).run(state, direction).await
        })
    }
}

#[rstest::rstest]
#[case::two_children(true, vec!["before", "child a", "child b", "after"])]
#[case::different_branch(false, vec!["before", "other child", "after"])]
#[tokio::test]
async fn a_step_composes_its_own_child_vector(#[case] branch: bool, #[case] expected: Vec<&str>) {
    let common = common_args();
    let mut state = state(&common, &[]);
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut parent = Conversation::new(vec![
        Box::new(RecordStep("before", log.clone())),
        Box::new(BranchStep(branch, log.clone())),
        Box::new(RecordStep("after", log.clone())),
    ]);
    parent.run(&mut state, Direction::forward()).await.unwrap();
    assert_eq!(*log.borrow(), expected);
    assert_eq!(state.plans.len(), expected.len());
}

struct DeferredChild(Rc<RefCell<Vec<&'static str>>>);
impl InitStep for DeferredChild {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        _direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            let mut suspended = false;
            std::future::poll_fn(|cx| {
                if suspended {
                    return std::task::Poll::Ready(());
                }
                suspended = true;
                self.0.borrow_mut().push("pending child");
                cx.waker().wake_by_ref();
                std::task::Poll::Pending
            })
            .await;
            state.args.select_docker();
            self.0.borrow_mut().push("completed child");
            Ok(StepCompletion::next())
        })
    }
}

struct NestedStep(Conversation);
impl InitStep for NestedStep {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move { self.0.run(state, direction).await })
    }
}

struct ObserveChild(Rc<RefCell<Vec<&'static str>>>);
impl InitStep for ObserveChild {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        _direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            assert!(
                state.args.docker(),
                "parent must observe completed child settings"
            );
            self.0.borrow_mut().push("parent resumed");
            Ok(StepCompletion::next())
        })
    }
}

#[tokio::test]
async fn parent_waits_for_child_and_observes_its_accepted_settings() {
    let common = common_args();
    let mut state = state(&common, &[]);
    let log = Rc::new(RefCell::new(Vec::new()));
    let child = Conversation::new(vec![Box::new(DeferredChild(log.clone()))]);
    let mut parent = Conversation::new(vec![
        Box::new(NestedStep(child)),
        Box::new(ObserveChild(log.clone())),
    ]);
    let mut execution = Box::pin(parent.run(&mut state, Direction::forward()));
    assert!(futures_util::poll!(execution.as_mut()).is_pending());
    assert_eq!(*log.borrow(), ["pending child"]);
    execution.await.unwrap();
    assert_eq!(
        *log.borrow(),
        ["pending child", "completed child", "parent resumed"]
    );
    assert!(state.args.docker());
}

#[rstest::rstest]
#[case::error(true)]
#[case::cancelled(false)]
#[tokio::test]
async fn incomplete_child_stops_its_containing_parent_step(#[case] error: bool) {
    let common = common_args();
    let mut state = state(&common, &[]);
    let log = Rc::new(RefCell::new(Vec::new()));
    let child = Conversation::new(vec![
        Box::new(RecordStep("accepted child", log.clone())),
        Box::new(StopStep(error)),
    ]);
    let completion = Conversation::new(vec![
        Box::new(NestedStep(child)),
        Box::new(RecordStep("parent", log.clone())),
    ])
    .run(&mut state, Direction::forward())
    .await;
    assert_eq!(completion.is_err(), error);
    if let Ok(completion) = completion {
        assert!(completion.is_cancelled());
    }
    assert_eq!(*log.borrow(), ["accepted child"]);
    assert_eq!(state.plans.len(), 1, "accepted state is not rolled back");
}

struct QuestionStep(&'static str);
impl InitStep for QuestionStep {
    fn prompt<'a>(
        &'a mut self,
        state: &'a mut InitializationState<'_>,
        _direction: Direction,
    ) -> StepFuture<'a> {
        Box::pin(async move {
            state
                .input
                .as_mut()
                .answer(&crate::init::prompt::PromptRequest::confirm(self.0))?;
            Ok(StepCompletion::next())
        })
    }
}

#[tokio::test]
async fn back_crosses_nested_vectors_and_nonprompt_steps_without_question_dispatch() {
    use crate::init::test_helpers::{Answer, Script};

    let common = common_args();
    let mut state = state(&common, &[]);
    let script = Script::install(
        &mut state,
        vec![
            Answer::confirm("first", true),
            Answer::confirm("child", true),
            Answer::confirm("inserted", true),
            Answer::cancelled("last"),
            Answer::cancelled("inserted"),
            Answer::cancelled("child"),
            Answer::confirm("first", true),
            Answer::confirm("child", true),
            Answer::confirm("inserted", true),
            Answer::confirm("last", true),
        ],
    );
    let log = Rc::new(RefCell::new(vec![]));
    let child = Conversation::new(vec![
        Box::new(RecordStep("child entry", log.clone())),
        Box::new(QuestionStep("child")),
        Box::new(QuestionStep("inserted")),
        Box::new(RecordStep("child exit", log.clone())),
    ]);
    let mut parent = Conversation::new(vec![
        Box::new(QuestionStep("first")),
        Box::new(RecordStep("nonprompt", log.clone())),
        Box::new(NestedStep(child)),
        Box::new(QuestionStep("last")),
    ]);
    let completion = parent.run(&mut state, Direction::forward()).await.unwrap();
    assert!(!completion.is_back() && !completion.is_cancelled());
    assert_eq!(log.borrow().len(), 9);
    script.assert_finished();
}
