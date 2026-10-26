use super::{ConnectionStep, HomeStep, InitStep, PostgresSourceStep, SelectionStep, VersionsStep};
use crate::init::test_helpers::{Answer, HomeFixture, Script, common_args, state};

#[tokio::test]
async fn independent_steps_update_shared_answers() {
    let common = common_args();
    let mut state = state(&common, &[]);
    let script = Script::install(
        &mut state,
        vec![
            Answer::choose("Choose engines", &[]),
            Answer::choose("Choose engines", &[0, 1]),
            Answer::choose("builtin source", &[0, 1]),
            Answer::choose("builtin source", &[0]),
            Answer::choose("versions to initialize", &[0, 1]),
        ],
    );
    SelectionStep.prompt(&mut state).await.unwrap();
    assert_eq!(
        state.selection.as_ref().unwrap().engines,
        ["postgres", "sqlite"]
    );
    PostgresSourceStep.prompt(&mut state).await.unwrap();
    assert!(state.args.docker());
    VersionsStep.prompt(&mut state).await.unwrap();
    assert_eq!(state.versions.unwrap().len(), 2);
    script.assert_finished();
}

#[tokio::test]
async fn home_confirmation_precedes_creation_and_later_questions() {
    let common = common_args();
    let fixture = HomeFixture::new();
    let home = fixture.home();
    let mut state = state(&common, &["--sql-fun-home", home.to_str().unwrap()]);
    let script = Script::install(
        &mut state,
        vec![
            Answer::confirm("Initialize", true).with_home(&home, false),
            Answer::confirm("Use", true).with_home(&home, false),
            Answer::choose("Choose engines", &[1]).with_home(&home, true),
        ],
    );
    HomeStep.prompt(&mut state).await.unwrap();
    SelectionStep.prompt(&mut state).await.unwrap();
    assert!(home.is_dir());
    script.assert_finished();
}

#[tokio::test]
async fn explicit_inputs_do_not_open_terminal() {
    let common = common_args();
    let mut state = state(
        &common,
        &[
            "--yes",
            "--engines",
            "postgres",
            "--docker",
            "--engine-versions",
            "17",
        ],
    );
    let script = Script::install(&mut state, vec![]);
    SelectionStep.prompt(&mut state).await.unwrap();
    PostgresSourceStep.prompt(&mut state).await.unwrap();
    VersionsStep.prompt(&mut state).await.unwrap();
    assert!(state.args.docker());
    assert_eq!(state.versions.unwrap().len(), 1);
    script.assert_finished();
}

#[tokio::test]
async fn connection_step_accepts_only_nonempty_connection_input() {
    let common = common_args();
    let mut state = state(&common, &[]);
    let script = Script::install(
        &mut state,
        vec![
            Answer::confirm("libpq connection string", true),
            Answer::text("libpq connection string", "   "),
            Answer::text("libpq connection string", "host=explicit-server"),
        ],
    );
    ConnectionStep.prompt(&mut state).await.unwrap();
    assert_eq!(state.args.database_url(), Some("host=explicit-server"));
    script.assert_finished();
}
