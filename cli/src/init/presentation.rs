use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum HomePathError {
    NotAbsolute,
    ParentUnavailable,
    ParentNotFound(PathBuf),
}

impl HomePathError {
    fn show(&self) {
        match self {
            Self::NotAbsolute => {
                eprint!("{}", include_str!("absolute_path_required.txt"));
            }
            Self::ParentUnavailable => {
                eprint!("{}", include_str!("parent_unavailable.txt"));
            }
            Self::ParentNotFound(path) => {
                eprint!(include_str!("parent_not_found.txt"), path = path.display());
            }
        }
    }
}

#[derive(Debug)]
pub enum Notice {
    HomeDescription,
    HomeExists(PathBuf),
    HomeCreating(PathBuf),
    HomeAlreadyExists,
    HomeCreated(PathBuf),
    HomeCreateFailed(PathBuf, std::io::Error),
    InvalidHome(HomePathError),
    VersionsDescription,
    VersionRequired,
}

impl Notice {
    pub const fn home_description() -> Self {
        Self::HomeDescription
    }

    pub fn home_exists(path: &Path) -> Self {
        Self::HomeExists(path.to_path_buf())
    }

    pub fn home_creating(path: &Path) -> Self {
        Self::HomeCreating(path.to_path_buf())
    }

    pub const fn home_already_exists() -> Self {
        Self::HomeAlreadyExists
    }

    pub fn home_created(path: &Path) -> Self {
        Self::HomeCreated(path.to_path_buf())
    }

    pub fn home_create_failed(path: &Path, error: std::io::Error) -> Self {
        Self::HomeCreateFailed(path.to_path_buf(), error)
    }

    pub const fn invalid_home(error: HomePathError) -> Self {
        Self::InvalidHome(error)
    }

    pub const fn versions_description() -> Self {
        Self::VersionsDescription
    }

    pub const fn version_required() -> Self {
        Self::VersionRequired
    }

    pub fn show(&self) {
        match self {
            Self::HomeDescription => Self::show_home_description(),
            Self::HomeExists(path) => Self::show_home_exists(path),
            Self::HomeCreating(path) => Self::show_home_creating(path),
            Self::HomeAlreadyExists => Self::show_home_already_exists(),
            Self::HomeCreated(path) => Self::show_home_created(path),
            Self::HomeCreateFailed(path, error) => Self::show_home_create_failed(path, error),
            Self::InvalidHome(error) => Self::show_invalid_home(error),
            Self::VersionsDescription => Self::show_versions_description(),
            Self::VersionRequired => Self::show_version_required(),
        }
    }

    fn show_home_description() {
        println!("{}", include_str!("create_home_dir.txt"));
    }

    fn show_home_exists(path: &Path) {
        eprint!(include_str!("home_exists.txt"), path = path.display());
    }

    fn show_home_creating(path: &Path) {
        print!(include_str!("home_creating.txt"), path = path.display());
    }

    fn show_home_already_exists() {
        print!("{}", include_str!("home_already_exists.txt"));
    }

    fn show_home_created(path: &Path) {
        eprint!(include_str!("home_created.txt"), path = path.display());
    }

    fn show_home_create_failed(path: &Path, error: &std::io::Error) {
        eprint!(
            include_str!("home_create_failed.txt"),
            path = path.display(),
            error = error
        );
    }

    fn show_invalid_home(error: &HomePathError) {
        error.show();
    }

    fn show_versions_description() {
        println!("{}", include_str!("database_engine_versions.txt"));
    }

    fn show_version_required() {
        eprint!("{}", include_str!("version_required.txt"));
    }
}
