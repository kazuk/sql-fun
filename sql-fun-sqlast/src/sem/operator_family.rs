//! PostgreSQL operator families and their effective membership.

mod alter;
pub(crate) mod member;
pub(crate) mod seed;

pub use alter::AlterOperatorFamily;

use std::{collections::HashMap, sync::Arc};

use crate::{
    ast::{Name, operator_family::CreateOperatorFamily as Syntax},
    context::{BaseParseContext, ParseContext},
    names::{FullName, OperatorName, SchemaName},
    sem::{
        AccessMethodKind, AnalysisError, AnalysisProblem, TypeReference,
        search_path::search_schemas,
    },
};

/// Schema-qualified family name scoped to one index method.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct OperatorFamilyKey {
    pub(crate) name: FullName,
    pub(crate) method: String,
}

impl OperatorFamilyKey {
    /// Create an exact key from a schema-qualified name and method.
    pub fn new(name: FullName, method: &str) -> Self {
        Self {
            name,
            method: method.to_owned(),
        }
    }

    pub(crate) fn resolve(
        name: &Name,
        method: &str,
        context: &impl BaseParseContext,
    ) -> Result<Option<Self>, AnalysisError> {
        let name = FullName::from_ast_name(name)?;
        if name.catalog().is_some() {
            return Ok(None);
        }
        let exists = |name| {
            let key = Self::new(name, method);
            context.operator_family(&key).map(|_| key)
        };
        if name.schema().is_some() {
            return Ok(exists(name));
        }
        Ok(search_schemas(context, false)
            .iter()
            .filter(|schema| !schema.as_str().starts_with("pg_temp_"))
            .find_map(|schema| exists(name.replace_schema(schema))))
    }
}

/// Complete family state published by a schema layer.
#[derive(Clone, Debug)]
pub struct OperatorFamily {
    pub(crate) key: OperatorFamilyKey,
    pub(crate) members: HashMap<FamilySlot, FamilyMember>,
}

impl OperatorFamily {
    /// Exact identity of this family.
    pub const fn key(&self) -> &OperatorFamilyKey {
        &self.key
    }

    pub(crate) fn new(key: OperatorFamilyKey) -> Self {
        Self {
            key,
            members: HashMap::new(),
        }
    }

    pub(crate) fn insert(&mut self, slot: FamilySlot, member: FamilyMember) -> Result<(), String> {
        if let Some(existing) = self.members.get(&slot) {
            let occupied = match &existing.object {
                MemberObject::Operator(operator) => operator.to_string(),
                MemberObject::Function { name, arguments } => {
                    let arguments = arguments
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>();
                    format!("{name}({})", arguments.join(", "))
                }
            };
            return Err(format!(
                "{:?} slot {} for ({}, {}) is already occupied by {occupied}",
                slot.kind, slot.number, slot.left, slot.right
            ));
        }
        if let MemberObject::Operator(operator) = &member.object
            && self.members.iter().any(|(existing_slot, existing)| {
                matches!(&existing.object, MemberObject::Operator(other) if operator == other)
                    && slot.left == existing_slot.left
                    && slot.right == existing_slot.right
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
    pub(crate) left: Arc<TypeReference>,
    pub(crate) right: Arc<TypeReference>,
}

#[derive(Clone, Debug)]
pub(crate) struct FamilyMember {
    pub(crate) object: MemberObject,
    pub(crate) order_family: Option<Box<OperatorFamilyKey>>,
    pub(crate) owner: MemberOwner,
}

#[derive(Clone, Debug)]
pub(crate) enum MemberObject {
    Operator(OperatorName),
    Function {
        name: FullName,
        arguments: Box<[Arc<TypeReference>]>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MemberOwner {
    Builtin,
    Family,
    Loose,
    Class(Box<FullName>),
}

pub(crate) fn report_family_problem(
    context: &mut impl ParseContext,
    message: impl Into<String>,
) -> Result<(), AnalysisError> {
    context.report_analysis_problem(AnalysisProblem::InvalidOperatorFamilyDeclaration(
        message.into(),
    ))
}

pub(crate) fn require_index_method(
    method: &str,
    context: &mut impl ParseContext,
) -> Result<bool, AnalysisError> {
    match context.access_method(method) {
        Some(definition) if definition.kind() == AccessMethodKind::Index => Ok(true),
        Some(_) => {
            report_family_problem(
                context,
                format!("access method {method:?} is not of type INDEX"),
            )?;
            Ok(false)
        }
        None => {
            report_family_problem(context, format!("access method {method:?} does not exist"))?;
            Ok(false)
        }
    }
}

pub(crate) fn creation_name(
    name: &Name,
    context: &mut impl ParseContext,
) -> Result<Option<FullName>, AnalysisError> {
    let name = FullName::from_ast_name(name)?;
    if name.catalog().is_some() {
        report_family_problem(context, "cross-database operator family or class reference")?;
        return Ok(None);
    }
    let exists = |schema: &SchemaName| {
        matches!(
            schema.as_str(),
            "public" | "pg_catalog" | "information_schema"
        ) || context.schema_definition(schema.as_str()).is_some()
    };
    if let Some(schema) = name.schema() {
        if exists(schema) {
            return Ok(Some(name));
        }
        report_family_problem(context, format!("schema {schema} does not exist"))?;
    } else if let Some(schema) = context
        .get_search_path()
        .iter()
        .find(|schema| exists(schema))
    {
        return Ok(Some(name.replace_schema(schema)));
    } else {
        report_family_problem(
            context,
            "no schema selected for operator family or class creation",
        )?;
    }
    Ok(None)
}

/// An analyzed standalone operator family declaration.
#[derive(Clone, Debug)]
pub struct CreateOperatorFamily {
    definition: Option<OperatorFamily>,
}

impl CreateOperatorFamily {
    /// Definition supplied by an applicable declaration.
    pub const fn definition(&self) -> Option<&OperatorFamily> {
        self.definition.as_ref()
    }

    pub(crate) fn analyze(
        syntax: &Syntax,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let method = syntax.access_method();
        if !require_index_method(method, context)? {
            return Ok(Self { definition: None });
        }
        let Some(name) = creation_name(syntax.name(), context)? else {
            return Ok(Self { definition: None });
        };
        let key = OperatorFamilyKey::new(name, method);
        if context.operator_family(&key).is_some() {
            report_family_problem(
                context,
                format!(
                    "operator family {} already exists for access method {method:?}",
                    key.name
                ),
            )?;
            return Ok(Self { definition: None });
        }
        Ok(Self {
            definition: Some(OperatorFamily::new(key)),
        })
    }

    pub(crate) fn apply<C: ParseContext>(&self, context: C) -> C {
        if let Some(definition) = self.definition() {
            context.publish_operator_family(definition)
        } else {
            context
        }
    }
}
