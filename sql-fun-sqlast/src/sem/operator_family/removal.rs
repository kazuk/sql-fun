use std::collections::HashMap;

use crate::{
    context::{OperatorFamilyInfoRead, SchemaFileContext},
    names::FullName,
    sem::{
        AnalysisError, CascadeRemoval, CascadeTarget, OperatorIdentity,
        operator_family::{
            FamilySlot, MemberObject, MemberOwner, OperatorFamily, OperatorFamilyKey,
        },
    },
};

#[derive(Clone, Debug)]
pub(crate) struct FamilyRemoval {
    family: OperatorFamilyKey,
    operator: OperatorIdentity,
    slots: Vec<FamilySlot>,
    classes: Vec<FullName>,
}

impl CascadeRemoval for FamilyRemoval {
    fn target(&self) -> CascadeTarget {
        CascadeTarget::OperatorFamily(self.family.clone(), Box::new(self.operator.clone()))
    }

    fn restrict_problem(&self) -> Option<String> {
        self.classes
            .first()
            .map(|class| format!("operator class {class} depends on the operator"))
    }

    fn clone_box(&self) -> Box<dyn CascadeRemoval> {
        Box::new(self.clone())
    }

    fn remove(&self, context: &mut SchemaFileContext) -> Result<(), AnalysisError> {
        let Some(mut family) = context.operator_family(&self.family).cloned() else {
            return AnalysisError::raise_unexpected_input(
                "dependent operator family missing from catalog",
            );
        };
        for slot in &self.slots {
            family.members.remove(slot);
        }
        context.apply_operator_family_removal(&family, &self.classes);
        Ok(())
    }
}

impl FamilyRemoval {
    pub(crate) const fn operator(&self) -> &OperatorIdentity {
        &self.operator
    }
}

impl OperatorFamily {
    pub(crate) fn removal_registrations(&self) -> Vec<FamilyRemoval> {
        let mut by_operator = HashMap::new();
        let mut by_class: HashMap<&FullName, Vec<&FamilySlot>> = HashMap::new();
        for (slot, member) in &self.members {
            if let MemberOwner::Class(class) = &member.owner {
                by_class.entry(class.as_ref()).or_default().push(slot);
            }
            let MemberObject::Operator(name) = &member.object else {
                continue;
            };
            let operator = OperatorIdentity::bound(name, Some(&slot.left), &slot.right);
            let registration =
                by_operator
                    .entry(operator.clone())
                    .or_insert_with(|| FamilyRemoval {
                        family: self.key.clone(),
                        operator,
                        slots: Vec::new(),
                        classes: Vec::new(),
                    });
            if let MemberOwner::Class(class) = &member.owner {
                registration.classes.push(class.as_ref().clone());
            } else {
                registration.slots.push(slot.clone());
            }
        }
        for removal in by_operator.values_mut() {
            removal.classes.sort_by_cached_key(FullName::to_string);
            removal.classes.dedup();
            for class in &removal.classes {
                if let Some(slots) = by_class.get(class) {
                    removal
                        .slots
                        .extend(slots.iter().map(|slot| (*slot).clone()));
                }
            }
        }
        by_operator.into_values().collect()
    }
}
