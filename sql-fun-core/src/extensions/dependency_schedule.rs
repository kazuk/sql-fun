use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use crate::{ExtensionConfigError, ExtensionNameVersionPair, ExtensionScript};

/// A script in the datasource's dependency-respecting execution order.
#[derive(Debug)]
pub struct ScheduledExtensionScript {
    script: ExtensionScript,
    installation: bool,
}

impl ScheduledExtensionScript {
    /// Script and its effective control settings.
    #[must_use]
    pub const fn script(&self) -> &ExtensionScript {
        &self.script
    }

    /// Whether the script establishes the extension's installation schema.
    #[must_use]
    pub const fn is_installation(&self) -> bool {
        self.installation
    }
}

pub(super) fn schedule(
    selections: &[ExtensionNameVersionPair],
    directory: &Path,
) -> Result<Vec<ScheduledExtensionScript>, ExtensionConfigError> {
    let explicit = selections
        .iter()
        .map(|selection| (selection.name(), selection))
        .collect();
    let mut paths = ExtensionPaths {
        explicit,
        directory,
        by_name: HashMap::new(),
        dependency_order: Vec::new(),
    };
    for selection in selections {
        paths.discover(selection.name(), &mut Vec::new())?;
    }

    let mut scheduler = Scheduler {
        paths: &paths.by_name,
        active: Vec::new(),
        completed: HashSet::new(),
        ordered: Vec::new(),
    };
    // Prefer finishing prerequisites first, while letting an individual step
    // use the catalog from a prerequisite that has only installed so far.
    for name in &paths.dependency_order {
        scheduler.visit((name, paths.by_name[name].len() - 1))?;
    }
    Ok(scheduler.ordered)
}

#[cfg(test)]
mod test_dependency_schedule {
    use crate::{PostgresExtensionsCollection, extensions::test_helpers::ExtensionFixture};

    #[test]
    fn schedules_shared_prerequisite_once_using_explicit_version() -> ::testresult::TestResult {
        let fixture = ExtensionFixture::new(
            "root",
            &["1"],
            Some("default_version = 1\nrequires = 'left, right'"),
        )?;
        for name in ["left", "right", "shared"] {
            std::fs::write(fixture.script(name, "1"), "")?;
            let control = if name == "shared" {
                "default_version = 1"
            } else {
                "default_version = 1\nrequires = 'shared'"
            };
            std::fs::write(fixture.path().join(format!("{name}.control")), control)?;
        }
        std::fs::write(fixture.script("shared", "2"), "")?;
        let selected: PostgresExtensionsCollection =
            "root,right,shared,shared@2,shared@2".parse()?;
        let steps = selected.scheduled_scripts(fixture.path())?;
        assert_eq!(
            steps
                .iter()
                .map(|step| step.script().path())
                .collect::<Vec<_>>(),
            [
                fixture.script("shared", "2"),
                fixture.script("left", "1"),
                fixture.script("right", "1"),
                fixture.script("root", "1"),
            ],
        );
        Ok(())
    }

    #[test]
    fn reports_missing_prerequisite_and_closed_installation_cycle() -> ::testresult::TestResult {
        let fixture = ExtensionFixture::new(
            "first",
            &["1"],
            Some("default_version = 1\nrequires = 'second'"),
        )?;
        let selected: PostgresExtensionsCollection = "first".parse()?;
        let error = selected
            .scheduled_scripts(fixture.path())
            .unwrap_err()
            .to_string();
        assert!(error.contains("first--1.sql -> second"), "{error}");
        assert!(error.contains("no usable versions found"), "{error}");

        std::fs::write(fixture.script("second", "1"), "")?;
        std::fs::write(
            fixture.path().join("second.control"),
            "default_version = 1\nrequires = 'first'",
        )?;
        let error = selected
            .scheduled_scripts(fixture.path())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("Cyclic extension script dependency"),
            "{error}"
        );
        assert_eq!(error.matches("second--1.sql").count(), 2, "{error}");
        assert_eq!(error.matches("first--1.sql").count(), 1, "{error}");
        Ok(())
    }
}

struct ExtensionPaths<'a> {
    explicit: HashMap<&'a str, &'a ExtensionNameVersionPair>,
    directory: &'a Path,
    by_name: HashMap<String, Vec<ExtensionScript>>,
    dependency_order: Vec<String>,
}

impl ExtensionPaths<'_> {
    fn discover(
        &mut self,
        name: &str,
        requiring_scripts: &mut Vec<String>,
    ) -> Result<(), ExtensionConfigError> {
        if self.by_name.contains_key(name) {
            return Ok(());
        }

        let resolve = || {
            let mut selection = match self.explicit.get(name) {
                Some(selection) => (*selection).clone(),
                None => ExtensionNameVersionPair::try_new(name, &None)?,
            };
            selection.resolve_version(self.directory)
        };
        let scripts = resolve().map_err(|source| {
            if requiring_scripts.is_empty() {
                source
            } else {
                ExtensionConfigError::required_extension(
                    requiring_scripts.join(" -> ") + " -> " + name,
                    source,
                )
            }
        })?;
        let requirements =
            scripts
                .iter()
                .flat_map(|script| {
                    script.settings().requires().iter().map(move |required| {
                        (script.path().display().to_string(), required.clone())
                    })
                })
                .collect::<Vec<_>>();
        self.by_name.insert(name.to_owned(), scripts);
        for (script, required) in requirements {
            requiring_scripts.push(script);
            self.discover(&required, requiring_scripts)?;
            requiring_scripts.pop();
        }
        self.dependency_order.push(name.to_owned());
        Ok(())
    }
}

type Step<'a> = (&'a str, usize);

struct Scheduler<'a> {
    paths: &'a HashMap<String, Vec<ExtensionScript>>,
    active: Vec<Step<'a>>,
    completed: HashSet<Step<'a>>,
    ordered: Vec<ScheduledExtensionScript>,
}

impl<'a> Scheduler<'a> {
    fn visit(&mut self, step: Step<'a>) -> Result<(), ExtensionConfigError> {
        if self.completed.contains(&step) {
            return Ok(());
        }
        if let Some(start) = self.active.iter().position(|active| *active == step) {
            let chain = self.active[start..]
                .iter()
                .copied()
                .chain([step])
                .map(|(name, index)| self.paths[name][index].path().display().to_string())
                .collect::<Vec<_>>()
                .join(" -> ");
            return Err(ExtensionConfigError::CyclicDependency(chain));
        }

        self.active.push(step);
        let (name, index) = step;
        if index > 0 {
            self.visit((name, index - 1))?;
        }
        let script = &self.paths[name][index];
        for required in script.settings().requires() {
            self.visit((required, 0))?;
        }
        self.active.pop();
        self.completed.insert(step);
        self.ordered.push(ScheduledExtensionScript {
            script: script.clone(),
            installation: index == 0,
        });
        Ok(())
    }
}
