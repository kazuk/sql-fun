use super::{
    error::InitError,
    presentation::{HomePathError, Notice},
    prompt::{PromptRequest, PromptResponse},
};

use std::path::PathBuf;

use ::sql_fun_core::{InitializeArgs, resolve_sql_fun_home_path};

pub(super) struct Initialization {
    yes: bool,
    supplied_home: bool,
    home: PathBuf,
    default_home: String,
    home_input: String,
    confirmed: Option<bool>,
}

impl From<&InitializeArgs> for Initialization {
    fn from(args: &InitializeArgs) -> Self {
        let home = args
            .sql_fun_home()
            .clone()
            .unwrap_or_else(|| PathBuf::from(shellexpand::tilde("~/.sql-fun").as_ref()));
        Self {
            yes: args.assume_yes(),
            supplied_home: args.sql_fun_home().is_some(),
            default_home: home.to_string_lossy().into_owned(),
            home_input: String::new(),
            confirmed: None,
            home,
        }
    }
}

impl Initialization {
    pub(super) fn begin(&mut self) -> Result<bool, InitError> {
        if !self.yes {
            Notice::home_description().show();
        }
        self.needs_editing()
    }

    pub(super) fn confirmed(&self) -> bool {
        self.confirmed == Some(true)
    }

    /// Returns false only for an edited path whose creation should be retried.
    pub(super) fn create_directory(&self) -> Result<bool, InitError> {
        let existed = self.home.exists();
        match std::fs::create_dir_all(&self.home) {
            Ok(()) => {
                if existed {
                    Notice::home_exists(&self.home).show();
                } else {
                    Notice::home_created(&self.home).show();
                }
                Ok(true)
            }
            Err(error) => {
                if self.supplied_home {
                    return InitError::io(error);
                }
                Notice::home_create_failed(&self.home, error).show();
                Ok(false)
            }
        }
    }

    fn needs_editing(&mut self) -> Result<bool, InitError> {
        if self.supplied_home {
            match resolve_sql_fun_home_path(&self.home) {
                Ok(path) => {
                    self.home = path;
                    if self.home.exists() && !self.home.is_dir() {
                        if self.yes {
                            return Err(anyhow::anyhow!("sql-fun home must be a directory").into());
                        }
                        Notice::invalid_home(HomePathError::not_directory_error(&self.home)).show();
                        return Ok(true);
                    }
                }
                Err(error) => {
                    if self.yes {
                        return Err(anyhow::Error::from(error).into());
                    }
                    Notice::invalid_home(HomePathError::resolution(&error)).show();
                    return Ok(true);
                }
            }
        } else if self.yes {
            return Err(anyhow::anyhow!("--yes requires --sql-fun-home").into());
        }
        Ok(!self.supplied_home)
    }

    pub(super) fn edit_directory(
        &mut self,
        input: &mut dyn super::step::PromptInput,
    ) -> Result<(), InitError> {
        loop {
            let request = PromptRequest::text(
                include_str!("home_prompt.txt").trim_end(),
                &self.default_home,
            )
            .with_initial_text(&self.home_input);
            let answer = input.answer(&request)?;
            let Some(text) = answer.text_input() else {
                continue;
            };
            match Self::edited_path(text) {
                Ok(path) => {
                    self.home = path;
                    self.home_input.clear();
                    self.supplied_home = false;
                    self.confirmed = None;
                    return Ok(());
                }
                Err(error) => {
                    text.clone_into(&mut self.home_input);
                    Notice::invalid_home(error).show();
                }
            }
        }
    }

    fn edited_path(text: &str) -> Result<PathBuf, HomePathError> {
        let path = resolve_sql_fun_home_path(std::path::Path::new(text))
            .map_err(|error| HomePathError::resolution(&error))?;
        if path.exists() {
            if !path.is_dir() {
                return HomePathError::not_directory(&path);
            }
        } else if let Some(parent) = path.parent() {
            if !parent.is_dir() {
                if parent.exists() {
                    return HomePathError::not_directory(parent);
                }
                return HomePathError::parent_not_found(parent);
            }
        } else {
            return HomePathError::parent_unavailable();
        }
        Ok(path)
    }

    pub(super) fn confirm_directory(
        &mut self,
        input: &mut dyn super::step::PromptInput,
    ) -> Result<(), InitError> {
        loop {
            let automatic = if self.yes {
                Some(PromptResponse::confirm(true))
            } else {
                None
            };
            let request = PromptRequest::confirm(
                format!(include_str!("confirm_home.txt"), path = self.home.display()).trim_end(),
            )
            .with_automatic(automatic);
            if let Some(confirmed) = input.answer(&request)?.confirmation() {
                self.confirmed = Some(confirmed);
                return Ok(());
            }
        }
    }

    pub(super) fn set_home(&mut self, home: &std::path::Path) {
        self.home = home.to_path_buf();
        self.default_home = home.to_string_lossy().into_owned();
    }
    pub(super) fn home(&self) -> &std::path::Path {
        &self.home
    }
}

#[cfg(test)]
mod tests;
