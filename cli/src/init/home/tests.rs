use crate::init::{
    home::Initialization,
    prompt::PromptRequest,
    step::{Direction, HomeStep, InitStep},
    test_helpers::{Answer, HomeFixture, Script, common_args, state},
};

#[rstest::rstest]
#[case::relative(".")]
#[case::home_relative("~/sql-fun-home-test")]
fn edited_paths_resolve_before_confirmation(#[case] text: &str) {
    let path = Initialization::edited_path(text).unwrap();
    assert!(path.is_absolute());
    if text.starts_with('~') {
        assert_eq!(
            path,
            std::env::home_dir().unwrap().join("sql-fun-home-test")
        );
    } else {
        assert_eq!(path, std::env::current_dir().unwrap());
    }
}

#[tokio::test]
async fn invalid_text_is_retained_without_changing_the_original_suggestion() {
    let common = common_args();
    let fixture = HomeFixture::new();
    let home = fixture.home();
    let blocked = fixture.blocked_home();
    let mut state = state(&common, &["--sql-fun-home", home.to_str().unwrap()]);
    let request = PromptRequest::text(
        include_str!("../home_prompt.txt").trim_end(),
        home.to_str().unwrap(),
    )
    .with_explanation(&state.home.entry_explanation());
    let script = Script::install(
        &mut state,
        vec![
            Answer::confirm("Use", false),
            Answer::text("home directory", blocked.to_str().unwrap()).expecting(request.clone()),
            Answer::text("home directory", home.to_str().unwrap())
                .expecting(request.with_initial_text(blocked.to_str().unwrap())),
            Answer::confirm("Use", true),
        ],
    );
    HomeStep
        .prompt(&mut state, Direction::forward())
        .await
        .unwrap();
    fixture.assert_preserved();
    assert_eq!(state.home.home(), home);
    script.assert_finished();
}

#[tokio::test]
async fn invalid_cli_home_returns_to_editor_interactively() {
    let common = common_args();
    let fixture = HomeFixture::new();
    let home = fixture.home();
    let blocked = fixture.blocked_home();
    let mut state = state(&common, &["--sql-fun-home", blocked.to_str().unwrap()]);
    let script = Script::install(
        &mut state,
        vec![
            Answer::text("home directory", home.to_str().unwrap()),
            Answer::confirm("Use", true),
        ],
    );
    HomeStep
        .prompt(&mut state, Direction::forward())
        .await
        .unwrap();
    fixture.assert_preserved();
    assert!(!home.exists());
    script.assert_finished();
}

#[tokio::test]
async fn invalid_cli_home_is_an_error_under_yes() {
    let common = common_args();
    let fixture = HomeFixture::new();
    let blocked = fixture.blocked_home();
    let mut state = state(
        &common,
        &["--yes", "--sql-fun-home", blocked.to_str().unwrap()],
    );
    Script::install(&mut state, vec![]);
    assert!(
        HomeStep
            .prompt(&mut state, Direction::forward())
            .await
            .unwrap_err()
            .to_string()
            .contains("must be a directory")
    );
    fixture.assert_preserved();
}

#[test]
fn creation_failure_after_authorization_is_an_error() {
    let common = common_args();
    let fixture = HomeFixture::new();
    let home = fixture.blocked_home().join("home");
    let state = state(&common, &["--sql-fun-home", home.to_str().unwrap()]);
    assert!(state.home.create_directory().is_err());
    assert!(!home.exists());
    fixture.assert_preserved();
}

#[tokio::test]
async fn an_environment_candidate_does_not_satisfy_yes() {
    let common = common_args();
    let fixture = HomeFixture::new();
    let home = fixture.home();
    let mut state = state(&common, &["--yes"]);
    state.home.set_home(&home);
    Script::install(&mut state, vec![]);
    assert!(
        HomeStep
            .prompt(&mut state, Direction::forward())
            .await
            .unwrap_err()
            .to_string()
            .contains("--sql-fun-home")
    );
    assert!(!home.exists());
}
