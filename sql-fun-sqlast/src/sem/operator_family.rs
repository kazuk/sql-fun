//! PostgreSQL operator families and their effective membership.

use std::collections::HashMap;

use crate::{
    ast::Name,
    context::{BaseParseContext, ParseContext},
    names::FullName,
    sem::{
        AccessMethodKind, AnalysisError, AnalysisProblem, OperatorIdentity, TypeReference,
        conflict_masters::lookup_postgres,
    },
};

/// Schema-qualified family name scoped to one index method.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct OperatorFamilyKey {
    pub(crate) name: FullName,
    pub(crate) method: String,
}

impl OperatorFamilyKey {
    pub(crate) fn new(name: FullName, method: &str) -> Self {
        Self { name, method: method.to_owned() }
    }

    pub(crate) fn resolve(
        name: &Name,
        method: &str,
        context: &impl BaseParseContext,
    ) -> Result<Option<Self>, AnalysisError> {
        let name = FullName::from_ast_name(name)?;
        let lookup = lookup_postgres(context, &name, |name| {
            context.operator_family(&Self::new(name.clone(), method)).is_some()
        });
        Ok(lookup.resolved_name().map(|name| Self::new(name.clone(), method)))
    }
}

/// Complete family state published by a schema layer.
#[derive(Clone, Debug)]
pub struct OperatorFamily {
    pub(crate) key: OperatorFamilyKey,
    pub(crate) members: HashMap<FamilySlot, FamilyMember>,
}

impl OperatorFamily {
    pub(crate) fn new(key: OperatorFamilyKey) -> Self {
        Self { key, members: HashMap::new() }
    }

    pub(crate) fn insert(&mut self, slot: FamilySlot, member: FamilyMember) -> Result<(), String> {
        if self.members.contains_key(&slot) {
            return Err(format!("duplicate {:?} slot {} for ({}, {})", slot.kind, slot.number, slot.left, slot.right));
        }
        if let MemberObject::Operator(operator) = &member.object
            && self.members.values().any(|existing| {
                matches!(&existing.object, MemberObject::Operator(other) if operator == other)
                    && member.order_family.is_some() == existing.order_family.is_some()
            })
        {
            return Err("operator already occupies a slot of the same purpose".to_owned());
        }
        self.members.insert(slot, member);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum MemberKind {
    Operator,
    Function,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct FamilySlot {
    pub(crate) kind: MemberKind,
    pub(crate) number: i32,
    pub(crate) left: TypeReference,
    pub(crate) right: TypeReference,
}

#[derive(Clone, Debug)]
pub(crate) struct FamilyMember {
    pub(crate) object: MemberObject,
    pub(crate) order_family: Option<OperatorFamilyKey>,
    pub(crate) owner: MemberOwner,
}

#[derive(Clone, Debug)]
pub(crate) enum MemberObject {
    Operator(OperatorIdentity),
    Function {
        name: FullName,
        arguments: Vec<TypeReference>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MemberOwner {
    Builtin,
    Loose,
    Class(FullName),
}

pub(crate) fn report_family_problem(
    context: &mut impl ParseContext,
    message: impl Into<String>,
) -> Result<(), AnalysisError> {
    context.report_analysis_problem(AnalysisProblem::InvalidOperatorFamilyDeclaration(message.into()))
}

pub(crate) fn require_index_method(
    method: &str,
    context: &mut impl ParseContext,
) -> Result<bool, AnalysisError> {
    match context.access_method(method) {
        Some(definition) if definition.kind() == AccessMethodKind::Index => Ok(true),
        Some(_) => {
            report_family_problem(context, format!("access method {method:?} is not of type INDEX"))?;
            Ok(false)
        }
        None => {
            report_family_problem(context, format!("access method {method:?} does not exist"))?;
            Ok(false)
        }
    }
}
