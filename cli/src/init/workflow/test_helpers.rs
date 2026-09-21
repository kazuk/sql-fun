use super::{Initialization, PostgresVersions};
use std::path::PathBuf;

pub(super) fn initialization(yes: bool) -> Initialization {
    Initialization {
        yes,
        supplied_home: true,
        home: PathBuf::from("chosen/home"),
        default_home: "original/home".to_owned(),
        home_input: String::new(),
        version_args: Vec::new(),
        versions: PostgresVersions::ALL.to_vec(),
        notices: Vec::new(),
        error: None,
    }
}
