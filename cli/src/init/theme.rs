//! The mandatory local theme: explicit choices first, mutations after final Ok.
mod download;

use self::download::Downloader;
use crate::{
    highlighter_theme::HighlighterTheme,
    init::{choice, prompt::PromptRequest, step::PromptInput},
};
use ::anyhow::Context;
use ::sql_fun_core::InitializeArgs;
use std::{
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

pub(super) const THEME_PATH: &str = "highlighter/default.toml";
pub(super) const THEME_URL: &str = "https://kazuk.github.io/sql-fun/highlighter/default.toml";

/// Reject unsupported parents/files before reading, editing, or publishing.
fn read_theme(home: &Path) -> Result<Option<Vec<u8>>, anyhow::Error> {
    let parent = home.join("highlighter");
    match fs::symlink_metadata(&parent) {
        Ok(info) => anyhow::ensure!(
            info.is_dir() && !info.file_type().is_symlink(),
            "unsupported theme directory: {}",
            parent.display()
        ),
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let destination = home.join(THEME_PATH);
    match fs::symlink_metadata(&destination) {
        Ok(info) => anyhow::ensure!(
            info.is_file() && !info.file_type().is_symlink(),
            "unsupported theme destination: {}",
            destination.display()
        ),
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    Ok(Some(fs::read(destination)?))
}

fn validate_bytes(bytes: &[u8]) -> Result<(), anyhow::Error> {
    HighlighterTheme::from_toml_str(std::str::from_utf8(bytes)?)?;
    Ok(())
}

pub(super) fn validate_installed(home: &Path) -> Result<(), anyhow::Error> {
    let bytes = read_theme(home)?.context("required highlighter theme is missing")?;
    validate_bytes(&bytes)
        .with_context(|| format!("invalid theme at {}", home.join(THEME_PATH).display()))
}

#[cfg(test)]
mod test_installed_validation {
    use crate::{init::theme::validate_installed, test_helpers::theme::ThemeFixture};
    #[rstest::rstest]
    #[case::toml("[[broken")]
    #[case::foreground("keyword = { foreground = 'invalid-color' }")]
    #[case::background("comment = { background = 'invalid-color' }")]
    fn rejects_invalid_renderer_input(#[case] content: &str) {
        let fixture = ThemeFixture::new();
        fixture.write_theme(content);
        assert!(validate_installed(fixture.home()).is_err());
        fixture.assert_theme(content);
    }
}

#[derive(Clone)]
enum Operation {
    Retain,
    Download,
    Edit(String),
}
impl Operation {
    const fn retain() -> Self {
        Self::Retain
    }
    const fn download() -> Self {
        Self::Download
    }
    fn edit(editor: &str) -> Self {
        Self::Edit(editor.to_owned())
    }
    const fn title(&self) -> &str {
        match self {
            Self::Retain => "Retain validated existing theme",
            Self::Download => "Download default theme",
            Self::Edit(_) => "Create/Edit theme",
        }
    }
    fn editor(&self) -> Option<&str> {
        if let Self::Edit(editor) = self {
            Some(editor)
        } else {
            None
        }
    }
    const fn downloads(&self) -> bool {
        matches!(self, Self::Download)
    }
    async fn prepare(
        &self,
        stage: &mut tempfile::NamedTempFile,
        original: Option<&[u8]>,
    ) -> Result<(), anyhow::Error> {
        match self {
            Self::Download => Downloader::find()?.download(THEME_URL, stage.path()).await,
            Self::Edit(editor) => {
                stage.write_all(original.unwrap_or_default())?;
                let status = tokio::process::Command::new(editor)
                    .arg(stage.path())
                    .kill_on_drop(true)
                    .status()
                    .await
                    .context("could not run theme editor")?;
                anyhow::ensure!(status.success(), "theme editor failed: {status}");
                Ok(())
            }
            Self::Retain => anyhow::bail!("theme operation was not selected"),
        }
    }
}

pub(super) struct ThemePlan {
    home: PathBuf,
    original: Option<Vec<u8>>,
    operation: Operation,
}
impl ThemePlan {
    pub(super) fn inspect(home: &Path) -> Result<Self, anyhow::Error> {
        let original = read_theme(home)?;
        Ok(Self {
            home: home.to_owned(),
            original,
            operation: Operation::retain(),
        })
    }
    fn valid_original(&self) -> bool {
        self.original
            .as_deref()
            .is_some_and(|bytes| validate_bytes(bytes).is_ok())
    }
    pub(super) fn choose(
        &mut self,
        args: &InitializeArgs,
        retained: Option<&Self>,
        input: &mut dyn PromptInput,
    ) -> Result<bool, anyhow::Error> {
        if self.valid_original() {
            if !args.assume_yes() {
                println!(
                    "Diagnostic colors: {}. This existing theme is valid and will be kept unchanged, including customizations. No download or editor is needed.",
                    self.home.join(THEME_PATH).display()
                );
            }
            return Ok(true);
        }
        if args.install_default_theme() {
            anyhow::ensure!(
                self.original.is_none(),
                "existing theme is invalid at {}; correct it explicitly before downloading a default theme (--install-default-theme never authorizes replacement)",
                self.home.join(THEME_PATH).display()
            );
            self.operation = Operation::download();
            if !args.assume_yes() {
                println!(
                    "Download default theme was requested with --install-default-theme.\n{}",
                    self.choice_explanation()
                );
            }
            return Ok(true);
        }
        anyhow::ensure!(
            !args.assume_yes(),
            "a valid highlighter theme is required; use --install-default-theme for a missing theme, or explicitly correct the existing theme"
        );
        let mut options = Vec::new();
        if self.original.is_none() {
            options.push(Operation::download().title().to_owned());
        }
        options.push(Operation::edit("").title().to_owned());
        let retained = retained.filter(|plan| plan.home == self.home);
        let explanation = self.choice_explanation();
        let selected = choice::select_one_retained(
            "Choose highlighter theme operation",
            &options,
            &retained.map_or_else(Vec::new, |plan| vec![plan.operation.title().to_owned()]),
            |request| input.answer(&request.clone().with_explanation(&explanation)),
        )?;
        if selected == Operation::download().title() {
            self.operation = Operation::download();
        } else {
            let editor = if let Some(plan) = retained {
                plan.operation.editor().unwrap_or("")
            } else {
                ""
            };
            self.operation = Operation::edit(editor);
        }
        Ok(false)
    }
    pub(super) fn choose_editor(
        &mut self,
        input: &mut dyn PromptInput,
    ) -> Result<bool, anyhow::Error> {
        let Some(editor) = self.operation.editor() else {
            return Ok(true);
        };
        let request = PromptRequest::text("Theme editor executable", "")
            .with_initial_text(editor)
            .with_explanation(&format!("Theme destination: {}.\nAfter final Ok, this editor will open a temporary {}. Enter an executable name on PATH or an executable path, without shell arguments (for example vi or /usr/bin/vi). Save a valid theme and exit successfully to install it. The editor is not run now.\nTheme format: https://kazuk.github.io/sql-fun/cli/init.html#diagnostic-theme", self.home.join(THEME_PATH).display(), if self.original.is_some() { "copy of the existing file" } else { "new file" }));
        let answer = input.answer(&request)?;
        let editor = answer
            .text_input()
            .context("theme editor executable is required")?;
        anyhow::ensure!(
            !editor.trim().is_empty(),
            "A theme editor executable is required. Rerun and supply an executable name or path"
        );
        self.operation = Operation::edit(editor);
        Ok(false)
    }
    fn choice_explanation(&self) -> String {
        let mut lines = vec![format!(
            "Diagnostics need a valid color theme at {}.",
            self.home.join(THEME_PATH).display()
        )];
        if let Some(bytes) = &self.original {
            if let Err(error) = validate_bytes(bytes) {
                lines.push(format!(
                    "The existing theme is invalid: {error}. Downloading cannot overwrite it."
                ));
            }
        } else {
            lines.push(format!("No theme exists there. Download default theme: after final Ok, fetch {THEME_URL} using curl, or wget if curl is unavailable; requires HTTPS access and trusted certificates."));
        }
        lines.push("Create/Edit theme: after final Ok, run an editor you specify on a temporary file (a copy if a theme exists). Install it only if the editor succeeds and the result is valid. A valid theme is required; neither route skips this requirement.\nTheme format: https://kazuk.github.io/sql-fun/cli/init.html#diagnostic-theme".to_owned());
        lines.join("\n")
    }

    pub(super) fn review(&self) -> Result<String, anyhow::Error> {
        let mut lines = vec![
            format!("Diagnostic theme: {}", self.operation.title()),
            format!(
                "Theme destination: {}",
                self.home.join(THEME_PATH).display()
            ),
        ];
        if self.operation.downloads() {
            lines.push(format!("Theme source: {THEME_URL}"));
            lines.push("Download requires curl, or wget if curl is unavailable, HTTPS access and trusted certificates.".to_owned());
        }
        if let Some(editor) = self.operation.editor() {
            anyhow::ensure!(!editor.is_empty(), "theme editor executable is required");
            lines.push(format!("Theme editor: {editor}"));
        }
        Ok(lines.join("\n"))
    }
    pub(super) async fn install(&self) -> Result<(), anyhow::Error> {
        self.apply().await.with_context(|| {
            format!(
                "could not install highlighter theme at {}",
                self.home.join(THEME_PATH).display()
            )
        })
    }
    async fn apply(&self) -> Result<(), anyhow::Error> {
        if self.valid_original() {
            return validate_installed(&self.home);
        }
        let current = read_theme(&self.home)?;
        if current != self.original {
            // A concurrent initializer may have completed a valid installation.
            if self.original.is_none() && current.is_some() {
                return validate_installed(&self.home);
            }
            anyhow::bail!("theme changed since confirmation; rerun init");
        }
        fs::create_dir_all(self.home.join("highlighter"))?;
        let mut stage = tempfile::NamedTempFile::new_in(self.home.join("highlighter"))?;
        self.operation
            .prepare(&mut stage, self.original.as_deref())
            .await?;
        // Editors may save by replacing the staged inode. Read the actual path.
        let info = fs::symlink_metadata(stage.path())?;
        anyhow::ensure!(
            info.is_file() && !info.file_type().is_symlink(),
            "unsupported staged theme type"
        );
        validate_bytes(&fs::read(stage.path())?).context("downloaded/edited theme is invalid")?;
        self.publish(stage)?;
        validate_installed(&self.home)
    }
    fn publish(&self, stage: tempfile::NamedTempFile) -> Result<(), anyhow::Error> {
        let destination = self.home.join(THEME_PATH);
        let current = read_theme(&self.home)?;
        if current != self.original {
            if self.original.is_none() && current.is_some() {
                return validate_installed(&self.home);
            }
            anyhow::bail!("theme changed during installation; existing content preserved");
        }
        if self.original.is_some() {
            // Only an explicitly authorized edit may replace the selected invalid file.
            anyhow::ensure!(
                self.operation.editor().is_some(),
                "refusing to overwrite existing theme"
            );
            stage.persist(&destination).map_err(|error| error.error)?;
        } else if let Err(error) = stage.persist_noclobber(&destination) {
            if error.error.kind() == ErrorKind::AlreadyExists {
                return validate_installed(&self.home);
            }
            return Err(error.error.into());
        }
        println!("Installed theme: {}", destination.display());
        Ok(())
    }
}

#[cfg(test)]
mod test_theme_plan {
    use crate::init::step::TerminalInput as StepTerminalInput;

    use crate::{
        init::theme::{Operation, ThemePlan},
        test_helpers::theme::ThemeFixture,
    };
    use ::clap::Parser;
    use ::sql_fun_core::InitializeArgs;

    #[rstest::rstest]
    #[case::custom("keyword = { foreground = 'blue', bold = true }")]
    #[case::renderer_defaults("")]
    #[tokio::test]
    async fn retains_valid_themes_without_downloading(#[case] content: &str) {
        let fixture = ThemeFixture::new();
        fixture.write_theme(content);
        let plan = ThemePlan::inspect(fixture.home()).unwrap();
        plan.install().await.unwrap();
        fixture.assert_theme(content);
        fixture.assert_no_stage();
    }

    #[tokio::test]
    async fn edits_only_after_authorization_and_validates_before_publication() {
        let fixture = ThemeFixture::new();
        fixture.write_theme("invalid = [");
        let editor = fixture.program(
            "editor",
            r#"printf "keyword = { foreground = 'blue' }" > "$1""#,
        );
        let mut plan = ThemePlan::inspect(fixture.home()).unwrap();
        plan.operation = Operation::edit(editor.to_str().unwrap());
        plan.review().unwrap();
        fixture.assert_theme("invalid = [");
        plan.install().await.unwrap();
        fixture.assert_theme("keyword = { foreground = 'blue' }");
        fixture.assert_no_stage();
    }

    #[rstest::rstest]
    #[case::process("exit 1")]
    #[case::invalid(r#"printf "keyword = { foreground = 'invalid-color' }" > "$1""#)]
    #[tokio::test]
    async fn editor_failure_preserves_existing_data(#[case] body: &str) {
        let fixture = ThemeFixture::new();
        fixture.write_theme("invalid = [");
        let editor = fixture.program("editor", body);
        let mut plan = ThemePlan::inspect(fixture.home()).unwrap();
        plan.operation = Operation::edit(editor.to_str().unwrap());
        assert!(plan.install().await.is_err());
        fixture.assert_theme("invalid = [");
        fixture.assert_no_stage();
    }

    #[tokio::test]
    async fn changed_destination_is_not_overwritten() {
        let fixture = ThemeFixture::new();
        fixture.write_theme("invalid = [");
        let editor = fixture.program("editor", "exit 0");
        let mut plan = ThemePlan::inspect(fixture.home()).unwrap();
        plan.operation = Operation::edit(editor.to_str().unwrap());
        fixture.write_theme("keyword = { foreground = 'green' }");
        assert!(plan.install().await.is_err());
        fixture.assert_theme("keyword = { foreground = 'green' }");
    }

    #[test]
    fn noninteractive_missing_authorization_fails_without_creating_theme() {
        let fixture = ThemeFixture::new();
        let args = InitializeArgs::try_parse_from(["init", "--yes"]).unwrap();
        let mut plan = ThemePlan::inspect(fixture.home()).unwrap();
        let mut input = StepTerminalInput;
        assert!(
            plan.choose(&args, None, &mut input)
                .unwrap_err()
                .to_string()
                .contains("--install-default-theme")
        );
        assert!(!fixture.theme().exists());
    }

    #[test]
    fn explicit_download_does_not_authorize_overwriting_invalid_theme() {
        let fixture = ThemeFixture::new();
        fixture.write_theme("invalid = [");
        let args =
            InitializeArgs::try_parse_from(["init", "--yes", "--install-default-theme"]).unwrap();
        let mut plan = ThemePlan::inspect(fixture.home()).unwrap();
        assert!(
            plan.choose(&args, None, &mut crate::init::step::TerminalInput)
                .is_err()
        );
        fixture.assert_theme("invalid = [");
    }

    #[test]
    fn refuses_symlink_destinations() {
        let fixture = ThemeFixture::new();
        fixture.write_theme("");
        std::fs::remove_file(fixture.theme()).unwrap();
        std::os::unix::fs::symlink(fixture.home().join("user-data"), fixture.theme()).unwrap();
        assert!(ThemePlan::inspect(fixture.home()).is_err());
    }
}
