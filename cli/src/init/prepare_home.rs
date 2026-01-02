use std::path::{Path, PathBuf};

use inquire::{
    CustomUserError,
    validator::{ErrorMessage, Validation},
};

pub fn prepare_home(
    assume_yes: bool,
    home_arg: Option<&PathBuf>,
) -> Result<PathBuf, anyhow::Error> {
    if !assume_yes {
        let create_home_dir_description = include_str!("create_home_dir.txt");
        println!("{create_home_dir_description}");
    }

    if let Some(home) = home_arg {
        if home.exists() {
            // home directory exits
            eprintln!("{} directory exists.", home.display());
            Ok(home.clone())
        } else {
            // home directory not exists
            let confirmed = create_confirm(home, assume_yes)?;
            if confirmed {
                std::fs::create_dir_all(home)?;
                Ok(home.clone())
            } else {
                // No confirm, then edit to home
                let home_path = edit_home_path(home.to_string_lossy().as_ref())?;
                Ok(home_path)
            }
        }
    } else {
        let default_home = shellexpand::tilde("~/.sqlfun");
        let home_path = edit_home_path(&default_home)?;
        Ok(home_path)
    }
}

/// Validate input path is absolute and, non exists, parent path exists
#[allow(clippy::unnecessary_wraps)]
fn new_absolute_path_validator(input: &str) -> Result<Validation, CustomUserError> {
    let path = PathBuf::from(input);
    if !path.is_absolute() {
        return Ok(Validation::Invalid(ErrorMessage::from(
            "absolute path required",
        )));
    }
    if path.exists() {
        return Ok(Validation::Invalid(ErrorMessage::from("ready exists")));
    }
    let Some(parent) = path.parent() else {
        return Ok(Validation::Invalid(ErrorMessage::from(
            "can not to get parent",
        )));
    };
    if !parent.exists() {
        return Ok(Validation::Invalid(ErrorMessage::from(format!(
            "{} not exist",
            parent.display()
        ))));
    }
    Ok(Validation::Valid)
}

/// user edit and create directory for sql-fun home
fn edit_home_path(default_home: &str) -> Result<PathBuf, anyhow::Error> {
    loop {
        let input = inquire::Text::new("`sql-fun` home directory path:")
            .with_default(default_home)
            .with_validator(new_absolute_path_validator)
            .prompt()?;

        let path = PathBuf::from(input);

        match std::fs::create_dir_all(&path) {
            Ok(()) => {
                eprintln!("{} create successfully", path.display());
                return Ok(path);
            }
            Err(e) => {
                eprintln!("{} create failed {e} ", path.display());
                eprintln!("please specify other path.");
            }
        }
    }
}

fn create_confirm(home: &Path, assume_yes: bool) -> Result<bool, anyhow::Error> {
    if assume_yes {
        return Ok(true);
    }
    let confirmed = inquire::Confirm::new(&format!("Create the {}?", home.display())).prompt()?;

    Ok(confirmed)
}
