use super::{Initialization, PromptOutcome, PromptResponse, Step, Workflow};
use ::clap::Parser;
use ::statig::blocking::{IntoStateMachineExt, StateMachine};
use std::path::Path;

struct Conversation {
    machine: StateMachine<Workflow>,
    context: Initialization,
}
impl Conversation {
    fn new(home: &Path, extra: &[&str]) -> Self {
        let args = sql_fun_core::InitializeArgs::try_parse_from(
            std::iter::once("init").chain(extra.iter().copied()),
        )
        .unwrap();
        let common = sql_fun_core::SqlFunArgs::try_parse_from(["sql-fun"]).unwrap();
        let mut context = Initialization::new(&common, &args, false).unwrap();
        context.home = home.to_path_buf();
        Self {
            machine: Workflow.state_machine(),
            context,
        }
    }
    async fn send(&mut self, event: &PromptOutcome) {
        self.machine.handle_with_context(event, &mut self.context);
        if self.machine.state().is_asking() {
            self.context.prepare_question().await.unwrap();
        }
    }
    async fn answer(&mut self, value: &PromptResponse) {
        self.send(&PromptOutcome::answer(value)).await;
    }
    async fn back(&mut self) {
        self.send(&PromptOutcome::back()).await;
    }
    async fn complete_docker(&mut self) {
        self.answer(&PromptResponse::confirm(true)).await;
        self.answer(&PromptResponse::text(self.context.home.to_str().unwrap()))
            .await;
        self.answer(&PromptResponse::confirm(true)).await;
        self.answer(&PromptResponse::selection(&[0, 1])).await;
        self.answer(&PromptResponse::selection(&[0])).await;
        self.answer(&PromptResponse::selection(&[0, 1])).await;
        assert!(self.context.step.is_review());
    }
}

#[tokio::test]
async fn round_trip_retains_all_answers_without_authorization() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let mut conversation = Conversation::new(&home, &[]);
    conversation.complete_docker().await;
    let expected = conversation.context.answers.clone();
    for _ in 0..12 {
        conversation.back().await;
    }
    assert!(conversation.context.step.is_initial());
    assert_eq!(conversation.context.answers, expected);
    while !conversation.context.step.is_review() {
        let answer = expected.get(&conversation.context.step).unwrap();
        conversation.answer(answer).await;
    }
    assert_eq!(conversation.context.answers, expected);
    for _ in 0..3 {
        conversation.answer(&PromptResponse::selection(&[])).await;
    }
    assert!(conversation.machine.state().is_asking());
    assert!(!home.exists());
    conversation.answer(&PromptResponse::selection(&[1])).await;
    assert_eq!(
        conversation.context.finish().unwrap(),
        std::process::ExitCode::SUCCESS
    );
    assert!(!home.exists());
}

#[tokio::test]
async fn source_change_invalidates_only_after_an_actual_change() {
    let root = tempfile::tempdir().unwrap();
    let mut conversation = Conversation::new(&root.path().join("home"), &[]);
    conversation.complete_docker().await;
    conversation.back().await;
    conversation.back().await;
    assert_eq!(conversation.context.step, Step::source());
    assert!(conversation.context.answers.contains_key(&Step::versions()));
    conversation.answer(&PromptResponse::selection(&[1])).await;
    assert_eq!(conversation.context.step, Step::connection());
    assert!(!conversation.context.answers.contains_key(&Step::versions()));
    assert!(conversation.context.detected.is_none());
}

#[rstest::rstest]
#[case::cancel(false, 0)]
#[case::interrupt(true, 130)]
#[tokio::test]
async fn cancellation_does_not_create_home(#[case] interrupted: bool, #[case] status: u8) {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let mut conversation = Conversation::new(&home, &[]);
    if interrupted {
        conversation.send(&PromptOutcome::cancel()).await;
    } else {
        conversation.answer(&PromptResponse::confirm(false)).await;
    }
    assert_eq!(
        conversation.context.finish().unwrap(),
        std::process::ExitCode::from(status)
    );
    assert!(!home.exists());
}

#[tokio::test]
async fn only_fresh_ok_can_create_the_home() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let mut conversation = Conversation::new(&home, &[]);
    conversation.complete_docker().await;
    conversation
        .answer(&PromptResponse::selection(&[0, 1]))
        .await;
    assert!(!home.exists());
    conversation.answer(&PromptResponse::selection(&[0])).await;
    assert!(conversation.machine.state().is_authorized());
    assert!(home.is_dir());
    assert!(
        !conversation
            .context
            .answers
            .contains_key(&conversation.context.step)
    );
}

#[rstest::rstest]
#[case::relative(".")]
#[case::tilde("~/sql-fun-navigation-validation")]
fn home_editor_resolves_explicit_paths(#[case] path: &str) {
    let resolved = Step::validate_home(path).unwrap();
    assert!(resolved.is_absolute());
}

#[test]
fn home_editor_rejects_files_and_missing_parents_without_mutations() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("file");
    std::fs::write(&file, "preserve").unwrap();
    assert!(Step::validate_home(file.to_str().unwrap()).is_err());
    assert!(Step::validate_home(file.join("home").to_str().unwrap()).is_err());
    assert!(Step::validate_home(root.path().join("missing/home").to_str().unwrap()).is_err());
    assert_eq!(std::fs::read_to_string(file).unwrap(), "preserve");
    assert!(!root.path().join("missing").exists());
}

#[tokio::test]
async fn noninteractive_choices_skip_prompts_but_not_validation() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("parent/home");
    let path = home.to_str().unwrap();
    let mut conversation = Conversation::new(
        &home,
        &["--yes", "--sql-fun-home", path, "--engines", "sqlite"],
    );
    assert!(!conversation.context.prepare_question().await.unwrap());
    conversation.answer(&PromptResponse::selection(&[0])).await;
    assert!(conversation.machine.state().is_authorized());
    assert!(home.is_dir());
}

#[tokio::test]
async fn failed_creation_returns_to_editing_and_requires_fresh_ok() {
    let root = tempfile::tempdir().unwrap();
    let parent = root.path().join("parent");
    std::fs::create_dir(&parent).unwrap();
    let home = parent.join("home");
    let mut conversation = Conversation::new(&home, &[]);
    conversation.complete_docker().await;
    // Simulate a path becoming unavailable after the read-only validation.
    std::fs::remove_dir(&parent).unwrap();
    std::fs::write(&parent, "preserve").unwrap();
    conversation.answer(&PromptResponse::selection(&[0])).await;
    assert!(conversation.context.step.is_home_editor());
    assert!(conversation.machine.state().is_asking());
    let corrected = root.path().join("corrected");
    conversation
        .answer(&PromptResponse::text(corrected.to_str().unwrap()))
        .await;
    conversation.answer(&PromptResponse::confirm(true)).await;
    while !conversation.context.step.is_review() {
        let answer = conversation
            .context
            .answers
            .get(&conversation.context.step)
            .unwrap()
            .clone();
        conversation.answer(&answer).await;
    }
    conversation.answer(&PromptResponse::selection(&[])).await;
    assert!(!corrected.exists());
    conversation.answer(&PromptResponse::selection(&[0])).await;
    assert!(corrected.is_dir());
    assert_eq!(std::fs::read_to_string(parent).unwrap(), "preserve");
}
