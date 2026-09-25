use std::{
    collections::HashSet,
    fs::{self, File},
    io::Read,
    path::{Component, Path},
};

use ::anyhow::Context;

pub(super) const EXPORT_SCRIPT: &str = include_str!("export-extensions.sh");

pub(super) fn import(bundle: &Path, output: &Path) -> Result<(), anyhow::Error> {
    let metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("metadata.json"))?)?;
    anyhow::ensure!(
        metadata["format_version"] == 1,
        "unsupported catalog format version"
    );
    let server = metadata["server_version_num"]
        .as_u64()
        .context("missing server version")?;
    anyhow::ensure!(
        fs::metadata(bundle)?.is_file(),
        "extension bundle must be a regular file"
    );
    let file = File::open(bundle)?;
    let mut archive = tar::Archive::new(file);
    let mut seen = HashSet::new();
    let mut version = None;
    let extension = output.join("extension");
    fs::create_dir(&extension)?;
    let mut total = 0u64;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        anyhow::ensure!(
            path.components()
                .all(|part| matches!(part, Component::Normal(_))),
            "unsafe extension bundle path"
        );
        anyhow::ensure!(
            seen.insert(path.clone()),
            "duplicate extension bundle entry"
        );
        total = total
            .checked_add(entry.size())
            .context("bundle size overflow")?;
        if path == Path::new("extension") && entry.header().entry_type().is_dir() {
            continue;
        }
        anyhow::ensure!(
            entry.header().entry_type().is_file(),
            "bundle supports regular files only; export must dereference symlinks"
        );
        if path == Path::new("VERSION") {
            anyhow::ensure!(entry.size() <= 128, "invalid extension version record");
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            version = Some(parse_version(&text)?);
        } else {
            anyhow::ensure!(
                path.parent() == Some(Path::new("extension")),
                "unexpected bundle path"
            );
            let suffix = path.extension().context("extension file has no suffix")?;
            anyhow::ensure!(
                suffix == "sql" || suffix == "control",
                "unexpected extension file suffix"
            );
            let destination = output.join(path);
            let mut file = File::options()
                .write(true)
                .create_new(true)
                .open(destination)?;
            std::io::copy(&mut entry, &mut file)?;
        }
    }
    anyhow::ensure!(
        version == Some(server),
        "extension bundle PostgreSQL version must exactly match extracted server version"
    );
    Ok(())
}

#[cfg(test)]
mod import_tests {
    use super::super::test_helpers::BundleFixture;

    #[test]
    fn accepts_streamed_payload_larger_than_previous_fixed_ceiling() {
        let fixture = BundleFixture::new();
        let size = 257 * 1024 * 1024;
        fixture.import_large_extension(size).unwrap();
        assert_eq!(
            std::fs::metadata(fixture.path().join("extension/large.sql"))
                .unwrap()
                .len(),
            size
        );
    }

    #[test]
    fn preserves_extension_bytes() {
        let fixture = BundleFixture::new();
        fixture
            .import_entries(&[
                ("VERSION", b"1\n17.9\n", b'0'),
                ("extension/odd name.sql", b"SELECT '<&>';\n", b'0'),
            ])
            .unwrap();
        assert_eq!(
            std::fs::read(fixture.path().join("extension/odd name.sql")).unwrap(),
            b"SELECT '<&>';\n"
        );
    }

    #[test]
    fn rejects_unsafe_or_ambiguous_bundles() {
        for (path, kind) in [
            ("../escaped.sql", b'0'),
            ("/absolute.sql", b'0'),
            ("extension/../escaped.sql", b'0'),
            ("extension/link.sql", b'2'),
            ("extension/hard.sql", b'1'),
            ("extension/device.sql", b'3'),
            ("extension/unknown.txt", b'0'),
        ] {
            let fixture = BundleFixture::new();
            assert!(
                fixture
                    .import_entries(&[("VERSION", b"1\n17.9\n", b'0'), (path, b"", kind)])
                    .is_err(),
                "{path}"
            );
        }
        for entries in [
            vec![
                ("VERSION", b"1\n17.9\n".as_slice(), b'0'),
                ("VERSION", b"1\n17.9\n", b'0'),
            ],
            vec![("VERSION", b"1\n18.1\n".as_slice(), b'0')],
            vec![("extension/a.sql", b"".as_slice(), b'0')],
        ] {
            assert!(BundleFixture::new().import_entries(&entries).is_err());
        }
    }
}

fn parse_version(record: &str) -> Result<u64, anyhow::Error> {
    let lines: Vec<_> = record.lines().collect();
    anyhow::ensure!(
        lines.len() == 2 && lines[0] == "1",
        "unsupported extension bundle format"
    );
    let (major, minor) = lines[1]
        .split_once('.')
        .context("invalid PostgreSQL extension version")?;
    let major: u64 = major.parse()?;
    let minor: u64 = minor.parse()?;
    anyhow::ensure!(
        (17..=18).contains(&major) && minor < 10_000,
        "unsupported PostgreSQL extension version"
    );
    Ok(major * 10_000 + minor)
}

#[cfg(test)]
mod tests {
    use super::parse_version;

    #[test]
    fn validates_protocol_and_full_postgresql_version() {
        assert_eq!(parse_version("1\n17.9\n").unwrap(), 170_009);
        assert_eq!(parse_version("1\n18.3\n").unwrap(), 180_003);
        for text in ["2\n17.9\n", "1\n17\n", "1\n18.1\nextra", "1\n17.10000\n"] {
            assert!(parse_version(text).is_err());
        }
    }
}
