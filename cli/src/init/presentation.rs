use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum HomePathError {
    Resolution(String),
    NotDirectory(PathBuf),
    ParentUnavailable,
    ParentNotFound(PathBuf),
}

impl HomePathError {
    pub fn resolution(error: &impl std::fmt::Display) -> Self {
        Self::Resolution(error.to_string())
    }

    pub fn not_directory_error(path: &Path) -> Self {
        Self::NotDirectory(path.to_path_buf())
    }

    #[track_caller]
    pub fn not_directory<T>(path: &Path) -> Result<T, Self> {
        Err(Self::not_directory_error(path))
    }

    #[track_caller]
    pub const fn parent_unavailable<T>() -> Result<T, Self> {
        Err(Self::ParentUnavailable)
    }

    #[track_caller]
    pub fn parent_not_found<T>(path: &Path) -> Result<T, Self> {
        Err(Self::ParentNotFound(path.to_path_buf()))
    }

    fn show(&self) {
        match self {
            Self::Resolution(message) => eprintln!("{message}"),
            Self::NotDirectory(path) => eprintln!("{} is not a directory", path.display()),
            Self::ParentUnavailable => Self::show_parent_unavailable(),
            Self::ParentNotFound(path) => Self::show_parent_not_found(path),
        }
    }

    fn show_parent_unavailable() {
        eprint!("{}", include_str!("parent_unavailable.txt"));
    }

    fn show_parent_not_found(path: &Path) {
        eprint!(include_str!("parent_not_found.txt"), path = path.display());
    }
}

#[derive(Debug)]
pub enum Notice {
    HomeDescription,
    HomeExists(PathBuf),
    HomeCreated(PathBuf),
    InvalidHome(HomePathError),
}

impl Notice {
    pub const fn home_description() -> Self {
        Self::HomeDescription
    }

    pub fn home_exists(path: &Path) -> Self {
        Self::HomeExists(path.to_path_buf())
    }

    pub fn home_created(path: &Path) -> Self {
        Self::HomeCreated(path.to_path_buf())
    }

    pub const fn invalid_home(error: HomePathError) -> Self {
        Self::InvalidHome(error)
    }

    pub fn show(&self) {
        match self {
            Self::HomeDescription => Self::show_home_description(),
            Self::HomeExists(path) => Self::show_home_exists(path),
            Self::HomeCreated(path) => Self::show_home_created(path),
            Self::InvalidHome(error) => Self::show_invalid_home(error),
        }
    }

    fn show_home_description() {
        println!("{}", include_str!("create_home_dir.txt"));
    }

    fn show_home_exists(path: &Path) {
        eprint!(include_str!("home_exists.txt"), path = path.display());
    }

    fn show_home_created(path: &Path) {
        eprint!(include_str!("home_created.txt"), path = path.display());
    }

    fn show_invalid_home(error: &HomePathError) {
        error.show();
    }
}
