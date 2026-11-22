//! Read the compact operator-family catalog extracted from PostgreSQL.

use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::BufReader,
    path::Path,
    sync::Arc,
};

use crate::{
    names::{FullName, OperatorName},
    sem::{
        AnalysisError, OperatorFamilyKey, TypeReference,
        operator_family::{
            FamilyMember, FamilySlot, MemberKind, MemberObject, MemberOwner, OperatorFamily,
        },
    },
};

#[derive(serde::Deserialize)]
struct QualifiedName(String, String);

impl QualifiedName {
    fn full_name(&self) -> FullName {
        FullName::with_schema(&self.0, &self.1)
    }
}

#[derive(serde::Deserialize)]
struct SeedType(String, String, bool);

impl SeedType {
    fn intern(&self, types: &mut HashSet<Arc<TypeReference>>) -> Arc<TypeReference> {
        let reference =
            TypeReference::concrete_type_ref(&FullName::with_schema(&self.0, &self.1), self.2);
        if let Some(shared) = types.get(&reference) {
            return shared.clone();
        }
        let shared = Arc::new(reference);
        types.insert(shared.clone());
        shared
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum SeedOwner {
    Family,
    Loose,
    Class(QualifiedName),
}

fn owner(seed: Option<SeedOwner>) -> MemberOwner {
    match seed {
        None => MemberOwner::Builtin,
        Some(SeedOwner::Family) => MemberOwner::Family,
        Some(SeedOwner::Loose) => MemberOwner::Loose,
        Some(SeedOwner::Class(name)) => MemberOwner::Class(Box::new(name.full_name())),
    }
}

#[derive(serde::Deserialize)]
struct SeedOperator(
    i32,
    SeedType,
    SeedType,
    QualifiedName,
    Option<QualifiedName>,
    Option<SeedOwner>,
);

#[derive(serde::Deserialize)]
struct SeedFunction(i32, SeedType, SeedType, QualifiedName, Option<SeedOwner>);

#[derive(serde::Deserialize)]
struct SeedFamily {
    schema: String,
    name: String,
    access_method: String,
    operators: Vec<SeedOperator>,
    functions: Vec<SeedFunction>,
}

impl SeedFamily {
    fn into_family(
        self,
        types: &mut HashSet<Arc<TypeReference>>,
    ) -> Result<OperatorFamily, String> {
        let mut family = OperatorFamily::new(OperatorFamilyKey::new(
            FullName::with_schema(&self.schema, &self.name),
            &self.access_method,
        ));
        for SeedOperator(number, left, right, operator, order_family, dependency) in self.operators
        {
            family.insert(
                FamilySlot {
                    kind: MemberKind::Operator,
                    number,
                    left: left.intern(types),
                    right: right.intern(types),
                },
                FamilyMember {
                    object: MemberObject::Operator(OperatorName::from_full_name(
                        &operator.full_name(),
                    )),
                    order_family: order_family
                        .map(|name| Box::new(OperatorFamilyKey::new(name.full_name(), "btree"))),
                    owner: owner(dependency),
                },
            )?;
        }
        for SeedFunction(number, left, right, function, dependency) in self.functions {
            family.insert(
                FamilySlot {
                    kind: MemberKind::Function,
                    number,
                    left: left.intern(types),
                    right: right.intern(types),
                },
                FamilyMember {
                    object: MemberObject::Function(function.full_name()),
                    order_family: None,
                    owner: owner(dependency),
                },
            )?;
        }
        Ok(family)
    }
}

pub(crate) fn load(
    path: &Path,
) -> Result<HashMap<OperatorFamilyKey, OperatorFamily>, AnalysisError> {
    let invalid_seed = |error: &dyn std::fmt::Display| {
        AnalysisError::unexpected_input_error(&format!("{}: {error}", path.display()))
    };
    let reader = BufReader::new(File::open(path).map_err(|error| invalid_seed(&error))?);
    let rows: Vec<SeedFamily> =
        serde_json::from_reader(reader).map_err(|error| invalid_seed(&error))?;
    let mut families = HashMap::new();
    let mut types = HashSet::new();
    for row in rows {
        let family = row
            .into_family(&mut types)
            .map_err(|error| invalid_seed(&error))?;
        if families.insert(family.key.clone(), family).is_some() {
            return Err(invalid_seed(&"duplicate operator family"));
        }
    }
    Ok(families)
}
