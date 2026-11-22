//! Read the operator-family catalog extracted from PostgreSQL.

use std::{collections::HashMap, fs::File, io::BufReader, path::Path};

use crate::{
    names::{FullName, OperatorName},
    sem::{AnalysisError, OperatorFamilyIdentity, OperatorIdentity, TypeReference},
};

use crate::sem::operator_family::{
    FamilyMember, FamilySlot, MemberKind, MemberObject, MemberOwner, OperatorFamily,
    OperatorFamilyKey,
};

#[derive(serde::Deserialize)]
struct QualifiedName {
    schema: String,
    name: String,
}

impl QualifiedName {
    fn full_name(&self) -> FullName {
        FullName::with_schema(&self.schema, &self.name)
    }
}

#[derive(serde::Deserialize)]
struct SeedType {
    #[serde(flatten)]
    name: QualifiedName,
    array_element: Option<QualifiedName>,
}

impl SeedType {
    fn reference(&self) -> TypeReference {
        if let Some(element) = &self.array_element {
            TypeReference::concrete_type_ref(&element.full_name(), true)
        } else {
            TypeReference::concrete_type_ref(&self.name.full_name(), false)
        }
    }
}

#[derive(serde::Deserialize)]
struct SeedSlot {
    number: i32,
    left_type: SeedType,
    right_type: SeedType,
    owner: Option<OperatorFamilyIdentity>,
    builtin: bool,
    #[serde(default)]
    family_owned: bool,
}

impl SeedSlot {
    fn slot(&self, kind: MemberKind) -> FamilySlot {
        FamilySlot {
            kind,
            number: self.number,
            left: self.left_type.reference(),
            right: self.right_type.reference(),
        }
    }

    fn owner(&self) -> MemberOwner {
        if self.builtin {
            MemberOwner::Builtin
        } else if let Some(owner) = &self.owner {
            MemberOwner::Class(owner.qualified_name())
        } else if self.family_owned {
            MemberOwner::Family
        } else {
            MemberOwner::Loose
        }
    }
}

#[derive(serde::Deserialize)]
struct SeedOperator {
    #[serde(flatten)]
    slot: SeedSlot,
    operator: QualifiedName,
    order_family: Option<OperatorFamilyIdentity>,
}

#[derive(serde::Deserialize)]
struct SeedFunction {
    #[serde(flatten)]
    slot: SeedSlot,
    function: QualifiedName,
    arguments: Vec<SeedType>,
}

#[derive(serde::Deserialize)]
struct SeedFamily {
    #[serde(flatten)]
    identity: OperatorFamilyIdentity,
    operators: Vec<SeedOperator>,
    functions: Vec<SeedFunction>,
}

impl SeedFamily {
    fn into_family(self) -> Result<OperatorFamily, String> {
        let mut family = OperatorFamily::new(key(&self.identity));
        for operator in self.operators {
            let slot = operator.slot.slot(MemberKind::Operator);
            let identity = OperatorIdentity::binary_signature(
                OperatorName::from_full_name(&operator.operator.full_name()),
                slot.left.clone(),
                slot.right.clone(),
            );
            family.insert(
                slot,
                FamilyMember {
                    object: MemberObject::Operator(identity),
                    order_family: operator.order_family.as_ref().map(key),
                    owner: operator.slot.owner(),
                },
            )?;
        }
        for function in self.functions {
            family.insert(
                function.slot.slot(MemberKind::Function),
                FamilyMember {
                    object: MemberObject::Function {
                        name: function.function.full_name(),
                        arguments: function.arguments.iter().map(SeedType::reference).collect(),
                    },
                    order_family: None,
                    owner: function.slot.owner(),
                },
            )?;
        }
        Ok(family)
    }
}

fn key(identity: &OperatorFamilyIdentity) -> OperatorFamilyKey {
    OperatorFamilyKey::new(identity.qualified_name(), identity.access_method())
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
    for row in rows {
        let family = row.into_family().map_err(|error| invalid_seed(&error))?;
        if families.insert(family.key.clone(), family).is_some() {
            return Err(invalid_seed(&"duplicate operator family"));
        }
    }
    Ok(families)
}
