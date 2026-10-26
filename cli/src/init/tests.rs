use super::{
    flow::collect,
    test_helpers::{Answer, HomeFixture, MetadataProject, Script, common_args, state},
};

#[tokio::test]
async fn complete_interactive_flow_collects_mixed_engine_settings_after_creating_home() {
    let common = common_args();
    let fixture = HomeFixture::new();
    let home = fixture.home();
    let mut state = state(&common, &[]);
    let script = Script::install(
        &mut state,
        vec![
            Answer::confirm("Initialize", true).with_home(&home, false),
            Answer::text("home directory", home.to_str().unwrap()).with_home(&home, false),
            Answer::confirm("Use", true).with_home(&home, false),
            Answer::choose("Choose engines", &[0, 1]).with_home(&home, true),
            Answer::choose("builtin source", &[0]).with_home(&home, true),
            Answer::choose("versions to initialize", &[0, 1]).with_home(&home, true),
        ],
    );
    assert!(!collect(&mut state).await.unwrap().is_cancelled());
    assert_eq!(state.home.home(), home);
    assert!(state.args.docker());
    assert_eq!(state.versions.as_ref().unwrap().len(), 2);
    assert_eq!(state.plans.len(), 2);
    assert!(
        !home.join("postgres").exists(),
        "collection must not install catalogs"
    );
    script.assert_finished();
}

#[tokio::test]
async fn cancellation_at_versions_preserves_home_already_created() {
    let common = common_args();
    let fixture = HomeFixture::new();
    let home = fixture.home();
    let mut state = state(
        &common,
        &[
            "--sql-fun-home",
            home.to_str().unwrap(),
            "--engines",
            "postgres",
        ],
    );
    let script = Script::install(
        &mut state,
        vec![
            Answer::confirm("Initialize", true),
            Answer::confirm("Use", true).with_home(&home, false),
            Answer::choose("builtin source", &[0]).with_home(&home, true),
            Answer::cancelled("versions to initialize").with_home(&home, true),
        ],
    );
    assert!(collect(&mut state).await.is_err());
    assert!(home.is_dir());
    assert!(state.plans.is_empty());
    assert!(!home.join("postgres").exists());
    script.assert_finished();
}

#[tokio::test]
async fn decline_stops_before_home_and_selection() {
    let common = common_args();
    let fixture = HomeFixture::new();
    let home = fixture.home();
    let mut state = state(&common, &["--sql-fun-home", home.to_str().unwrap()]);
    let script = Script::install(&mut state, vec![Answer::confirm("Initialize", false)]);
    assert!(collect(&mut state).await.unwrap().is_cancelled());
    assert!(!home.exists());
    assert!(state.selection.is_none());
    script.assert_finished();
}

#[tokio::test]
async fn invalid_explicit_version_fails_before_noninteractive_home_creation() {
    let common = common_args();
    let fixture = HomeFixture::new();
    let home = fixture.home();
    let mut state = state(
        &common,
        &[
            "--yes",
            "--sql-fun-home",
            home.to_str().unwrap(),
            "--engines",
            "postgres",
            "--docker",
            "--engine-versions",
            "16",
        ],
    );
    Script::install(&mut state, vec![]);
    assert!(
        collect(&mut state)
            .await
            .unwrap_err()
            .to_string()
            .contains("not supported")
    );
    assert!(!home.exists());
}

#[tokio::test]
async fn complete_noninteractive_flow_never_prompts() {
    let common = common_args();
    let fixture = HomeFixture::new();
    let home = fixture.missing_parent_home();
    let mut state = state(
        &common,
        &[
            "--yes",
            "--sql-fun-home",
            home.to_str().unwrap(),
            "--engines",
            "postgres,sqlite",
            "--docker",
            "--engine-versions",
            "17,18",
        ],
    );
    let script = Script::install(&mut state, vec![]);
    assert!(!collect(&mut state).await.unwrap().is_cancelled());
    assert!(home.is_dir());
    assert_eq!(state.plans.len(), 2);
    assert_eq!(state.versions.as_ref().unwrap().len(), 2);
    script.assert_finished();
}

#[tokio::test]
async fn home_editor_retries_and_keeps_original_home_contents() {
    let common = common_args();
    let fixture = HomeFixture::new();
    let marker = fixture.blocked_home();
    let home = fixture.home();
    let blocked = fixture.missing_parent_home();
    let mut state = state(
        &common,
        &[
            "--sql-fun-home",
            fixture.existing_home().to_str().unwrap(),
            "--engines",
            "sqlite",
        ],
    );
    let script = Script::install(
        &mut state,
        vec![
            Answer::confirm("Initialize", true),
            Answer::confirm("Use", false),
            Answer::text("home directory", marker.to_str().unwrap()),
            Answer::text("home directory", blocked.to_str().unwrap()),
            Answer::text("home directory", home.to_str().unwrap()),
            Answer::confirm("Use", true).with_home(&home, false),
        ],
    );
    collect(&mut state).await.unwrap();
    fixture.assert_preserved();
    assert_eq!(state.home.home(), home);
    assert!(home.is_dir());
    assert!(!blocked.exists());
    script.assert_finished();
}

#[rstest::rstest]
#[case::named_datasource(false, "Choose datasources", 0)]
#[case::no_datasource_candidates(true, "Choose engines", 1)]
#[tokio::test]
async fn explicit_project_access_composes_the_appropriate_selection_question(
    #[case] empty: bool,
    #[case] title: &str,
    #[case] selected: usize,
) {
    let project = MetadataProject::new(empty);
    let mut state = project.state();
    let home = state.home.home().to_path_buf();
    let script = Script::install(
        &mut state,
        vec![
            Answer::confirm("Initialize", true),
            Answer::text("home directory", home.to_str().unwrap()),
            Answer::confirm("Use", true),
            Answer::choose(title, &[selected]).with_home(&home, true),
        ],
    );
    collect(&mut state).await.unwrap();
    let selection = state.selection.as_ref().unwrap();
    assert_eq!(selection.engines, ["sqlite"]);
    if empty {
        assert_eq!(selection.datasource_names, Vec::<String>::new());
    } else {
        assert_eq!(selection.datasource_names, ["local"]);
    }
    assert_eq!(state.plans.len(), 1);
    assert!(
        state.versions.is_none(),
        "unselected PostgreSQL must not add questions or settings"
    );
    script.assert_finished();
}
