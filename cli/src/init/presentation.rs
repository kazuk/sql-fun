use std::path::PathBuf;

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
    pub fn show(&self) {
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
            Self::InvalidHome(error) => error.show(),
            Self::VersionsDescription => {
                println!("{}", include_str!("database_engine_versions.txt"));
            }
            Self::VersionRequired => eprint!("{}", include_str!("version_required.txt")),
        }
    }
}
