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
            let number = parse_version(&text)?;
            let identity = sql_fun_core::PostgresVersionInventory::builtin()
                .detect(
                    u32::try_from(number)?,
                    text.lines().nth(1).context("missing PostgreSQL version")?,
                )
                .map_err(anyhow::Error::msg)?;
            if let Some(expected) = metadata["version_identity"].as_str() {
                anyhow::ensure!(
                    identity.to_string() == expected,
                    "extension bundle PostgreSQL identity must exactly match extracted server identity"
                );
            } else {
                anyhow::ensure!(
                    identity.is_stable(),
                    "prerelease catalog metadata must identify its beta/RC version"
                );
            }
            version = Some(number);
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
    #[ignore = "large-payload validation needs a dedicated execution path; see #716"]
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
    let number = if let Some((major, minor)) = lines[1].split_once('.') {
        let major: u32 = major.parse()?;
        let minor: u32 = minor.parse()?;
        anyhow::ensure!(minor < 10_000, "invalid PostgreSQL minor version");
        major
            .checked_mul(10_000)
            .context("PostgreSQL version number overflow")?
            .checked_add(minor)
            .context("PostgreSQL version number overflow")?
    } else {
        let identity: sql_fun_core::PostgresVersion =
            lines[1].parse().map_err(anyhow::Error::msg)?;
        anyhow::ensure!(
            !identity.is_stable(),
            "stable PostgreSQL extension version requires a minor version"
        );
        identity
            .major()
            .checked_mul(10_000)
            .context("PostgreSQL version number overflow")?
    };
    sql_fun_core::PostgresVersionInventory::builtin()
        .detect(number, lines[1])
        .map_err(anyhow::Error::msg)?;
    Ok(u64::from(number))
}

#[cfg(test)]
mod tests {
    use super::parse_version;

    #[rstest::rstest]
    #[case::seventeen("1\n17.9\n", 170_009)]
    #[case::eighteen("1\n18.3\n", 180_003)]
    fn validates_protocol_and_full_postgresql_version(#[case] text: &str, #[case] expected: u64) {
        assert_eq!(parse_version(text).unwrap(), expected);
    }

    #[test]
    fn diagnoses_unregistered_prereleases_as_unsupported() {
        assert!(
            parse_version("1\n19beta1\n")
                .unwrap_err()
                .to_string()
                .contains("not supported")
        );
    }

    #[rstest::rstest]
    #[case::protocol("2\n17.9\n")]
    #[case::missing_minor("1\n17\n")]
    #[case::extra_record("1\n18.1\nextra")]
    #[case::minor_overflow("1\n17.10000\n")]
    #[case::unsupported_older("1\n16.1\n")]
    #[case::unsupported_newer("1\n19.1\n")]
    fn rejects_invalid_or_unsupported_versions(#[case] text: &str) {
        assert!(parse_version(text).is_err());
    }
}
