use super::{
    flow::collect,
    test_helpers::{Answer, HomeFixture, MetadataProject, Script, common_args, state},
};

#[tokio::test]
async fn complete_interactive_flow_collects_mixed_engine_settings_without_creating_home() {
    let common = common_args();
    let fixture = HomeFixture::new();
    let home = fixture.home();
    let mut state = state(&common, &[]);
    let script = Script::install(
        &mut state,
        vec![
            Answer::text("home directory", home.to_str().unwrap()).with_home(&home, false),
            Answer::confirm("Use", true).with_home(&home, false),
            Answer::choose("Choose engines", &[0, 1]).with_home(&home, false),
            Answer::choose("builtin source", &[0]).with_home(&home, false),
            Answer::choose("versions to initialize", &[0, 1]).with_home(&home, false),
            Answer::choose("Initialize", &[0]).with_home(&home, false),
        ],
    );
    assert!(!collect(&mut state).await.unwrap().is_cancelled());
    assert_eq!(state.home.home(), home);
    assert!(state.effective_args().docker());
    assert_eq!(state.versions.as_ref().unwrap().len(), 2);
    assert_eq!(state.plans.len(), 2);
    assert!(
        !home.join("postgres").exists(),
        "collection must not install catalogs"
    );
    script.assert_finished();
}

#[tokio::test]
async fn cancellation_at_versions_does_not_create_home() {
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
            Answer::confirm("Use", true).with_home(&home, false),
            Answer::choose("builtin source", &[0]).with_home(&home, false),
            Answer::interrupted("versions to initialize").with_home(&home, false),
        ],
    );
    assert!(collect(&mut state).await.unwrap().is_cancelled());
    assert!(!home.exists());
    assert!(state.plans.is_empty());
    assert!(!home.join("postgres").exists());
    script.assert_finished();
}

#[tokio::test]
async fn final_cancel_preserves_filesystem() {
    let common = common_args();
    let fixture = HomeFixture::new();
    let home = fixture.home();
    let mut state = state(&common, &["--sql-fun-home", home.to_str().unwrap()]);
    let script = Script::install(
        &mut state,
        vec![
            Answer::confirm("Use", true),
            Answer::choose("Choose engines", &[1]),
            Answer::choose("Initialize", &[1]),
        ],
    );
    assert!(collect(&mut state).await.unwrap().is_cancelled());
    assert!(!home.exists());
    assert!(state.selection.is_some());
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
    assert!(!home.exists());
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
            Answer::confirm("Use", false),
            Answer::text("home directory", marker.to_str().unwrap()),
            Answer::text("home directory", blocked.to_str().unwrap()),
            Answer::text("home directory", home.to_str().unwrap()),
            Answer::confirm("Use", true).with_home(&home, false),
            Answer::choose("Initialize", &[0]),
        ],
    );
    collect(&mut state).await.unwrap();
    fixture.assert_preserved();
    assert_eq!(state.home.home(), home);
    assert!(!home.exists());
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
            Answer::text("home directory", home.to_str().unwrap()),
            Answer::confirm("Use", true),
            Answer::choose(title, &[selected]).with_home(&home, false),
            Answer::choose("Initialize", &[0]),
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

#[tokio::test]
async fn unchanged_answers_survive_nested_backtracking_and_repeated_first_escape() {
    let common = common_args();
    let fixture = HomeFixture::new();
    let home = fixture.home();
    let mut state = state(&common, &[]);
    let script = Script::install(
        &mut state,
        vec![
            Answer::text("home directory", home.to_str().unwrap()),
            Answer::confirm("Use", true),
            Answer::choose("Choose engines", &[0, 1]),
            Answer::choose("builtin source", &[0]),
            Answer::choose("versions to initialize", &[0, 1]),
            Answer::cancelled("Initialize"),
            Answer::cancelled("versions to initialize"),
            Answer::cancelled("builtin source"),
            Answer::cancelled("Choose engines"),
            Answer::cancelled("Use"),
            Answer::cancelled("home directory"),
            Answer::cancelled("home directory"),
            Answer::text("home directory", home.to_str().unwrap()),
            Answer::confirm("Use", true),
            Answer::choose("Choose engines", &[0, 1]),
            Answer::choose("builtin source", &[0]),
            Answer::choose("versions to initialize", &[0, 1]),
            Answer::choose("Initialize", &[]),
            Answer::choose("Initialize", &[0, 1]),
            Answer::choose("Initialize", &[0]),
        ],
    );
    assert!(!collect(&mut state).await.unwrap().is_cancelled());
    assert_eq!(state.home.home(), home);
    assert_eq!(state.plans.len(), 2);
    assert_eq!(state.versions.as_ref().unwrap().len(), 2);
    assert!(state.effective_args().docker());
    assert!(
        !state.args.docker(),
        "interactive choices must not become supplied CLI choices"
    );
    assert!(!home.exists());
    script.assert_finished();
}

#[tokio::test]
async fn removing_postgres_discards_its_answers_and_stale_plans() {
    let common = common_args();
    let fixture = HomeFixture::new();
    let home = fixture.home();
    let mut state = state(&common, &["--sql-fun-home", home.to_str().unwrap()]);
    let script = Script::install(
        &mut state,
        vec![
            Answer::confirm("Use", true),
            Answer::choose("Choose engines", &[0, 1]),
            Answer::choose("builtin source", &[0]),
            Answer::choose("versions to initialize", &[0]),
            Answer::cancelled("Initialize"),
            Answer::cancelled("versions to initialize"),
            Answer::cancelled("builtin source"),
            Answer::choose("Choose engines", &[1]),
            Answer::choose("Initialize", &[0]),
        ],
    );
    collect(&mut state).await.unwrap();
    assert_eq!(state.plans.len(), 1);
    assert!(state.versions.is_none());
    assert!(state.source.is_none());
    assert!(!home.exists());
    script.assert_finished();
}

#[tokio::test]
async fn changing_source_and_connection_invalidates_versions_only_after_acceptance() {
    let common = common_args();
    let mut state = state(&common, &[]);
    let script = Script::install(
        &mut state,
        vec![
            Answer::choose("builtin source", &[0]),
            Answer::choose("versions to initialize", &[0]),
            Answer::cancelled("builtin source"),
            Answer::choose("builtin source", &[1]),
            Answer::text("libpq connection string", "host=first"),
            Answer::text("libpq connection string", "host=first"),
            Answer::text("libpq connection string", "host=second"),
        ],
    );
    state.choose_postgres_source().unwrap();
    state.choose_versions().await.unwrap();
    assert!(state.choose_postgres_source().is_err());
    assert!(state.versions.is_some());
    state.choose_postgres_source().unwrap();
    assert!(state.versions.is_none());
    assert!(!state.effective_args().docker());
    state.choose_connection().unwrap();
    state.versions = Some(vec![]);
    state.choose_connection().unwrap();
    assert!(state.versions.is_some());
    state.choose_connection().unwrap();
    assert!(state.versions.is_none());
    assert_eq!(state.effective_args().database_url(), Some("host=second"));
    script.assert_finished();
}

#[tokio::test]
async fn changing_only_sqlite_datasources_preserves_confirmed_postgres_answers() {
    let project = MetadataProject::new(false);
    let mut state = project.state();
    let script = Script::install(
        &mut state,
        vec![
            Answer::choose("Choose datasources", &[1]),
            Answer::choose("Choose datasources", &[0, 1]),
            Answer::choose("Choose datasources", &[0]),
        ],
    );
    state.load_selection().unwrap();
    state.choose_datasources().unwrap();
    state.source = Some(false);
    state.versions = Some(vec!["17".parse().unwrap()]);
    state.choose_datasources().unwrap();
    assert_eq!(state.versions.as_ref().unwrap(), &["17".parse().unwrap()]);
    assert_eq!(state.source, Some(false));
    state.choose_datasources().unwrap();
    assert!(state.versions.is_none());
    assert!(state.source.is_none());
    script.assert_finished();
}
