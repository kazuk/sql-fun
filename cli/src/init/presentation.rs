use std::path::PathBuf;

#[derive(Debug)]
pub(super) enum HomePathError {
    NotAbsolute,
    ParentUnavailable,
    ParentNotFound(PathBuf),
}

#[derive(Debug)]
pub(super) enum Notice {
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
    pub(super) fn show(&self) {
        match self {
            Self::HomeDescription => println!("{}", include_str!("create_home_dir.txt")),
            Self::HomeExists(path) => {
                eprint!(include_str!("home_exists.txt"), path = path.display());
            }
            Self::HomeCreating(path) => {
                print!(include_str!("home_creating.txt"), path = path.display());
            }
            Self::HomeAlreadyExists => print!("{}", include_str!("home_already_exists.txt")),
            Self::HomeCreated(path) => {
                eprint!(include_str!("home_created.txt"), path = path.display());
            }
            Self::HomeCreateFailed(path, error) => eprint!(
                include_str!("home_create_failed.txt"),
                path = path.display(),
                error = error
            ),
            Self::InvalidHome(error) => match error {
                HomePathError::NotAbsolute => {
                    eprint!("{}", include_str!("absolute_path_required.txt"));
                }
                HomePathError::ParentUnavailable => {
                    eprint!("{}", include_str!("parent_unavailable.txt"));
                }
                HomePathError::ParentNotFound(path) => {
                    eprint!(include_str!("parent_not_found.txt"), path = path.display());
                }
            },
            Self::VersionsDescription => {
                println!("{}", include_str!("database_engine_versions.txt"));
            }
            Self::VersionRequired => eprint!("{}", include_str!("version_required.txt")),
        }
    }
}
