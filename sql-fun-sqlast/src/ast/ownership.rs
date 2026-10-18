use super::{AlterOwner, Name, ObjectArgument, ObjectTarget, Role};
use crate::sem::{
    AnalysisError, AnalysisProblem, AstAndContextPair, ObjectKind, OwnerSpecification,
    OwnershipIdentity, OwnershipLookup, ParseContext, SemAst, TypeReference,
};

impl AlterOwner {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<crate::sem::AlterOwner, AnalysisError> {
        let requested = self.target_identity(context)?;
        let target = if !requested.missing_types.is_empty() {
            OwnershipLookup::argument_type_not_found(&requested.identity, &requested.missing_types)
        } else {
            requested.identity.resolve(context)
        };
        if target.identity().is_none() {
            context.report_analysis_problem(AnalysisProblem::ownership_target(&target))?;
        }
        let owner = self.owner.resolve_owner(context)?;
        Ok(crate::sem::AlterOwner::new(&target, &owner))
    }
    pub(crate) fn resolve_and_apply<C: ParseContext>(
        &self,
        mut context: C,
    ) -> Result<AstAndContextPair<C>, AnalysisError> {
        let alteration = self.resolve(&mut context)?;
        let context = context.apply_alter_owner(&alteration)?;
        Ok(AstAndContextPair::new(
            SemAst::alter_owner(&alteration),
            context,
        ))
    }
    fn target_identity(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<RequestedIdentity, AnalysisError> {
        let Some(kind) = self.object_kind else {
            return AnalysisError::raise_unexpected_none("alter_owner.object_kind");
        };
        if let Some(relation) = &self.relation {
            return Ok(RequestedIdentity::new(
                &OwnershipIdentity::named(kind, &relation.full_name()?),
                &[],
            ));
        }
        let Some(object) = &self.object else {
            return AnalysisError::raise_unexpected_none("alter_owner.object");
        };
        object.ownership_identity(kind, context)
    }
}

struct RequestedIdentity {
    identity: OwnershipIdentity,
    missing_types: Vec<crate::sem::FullName>,
}
impl RequestedIdentity {
    fn new(identity: &OwnershipIdentity, missing_types: &[crate::sem::FullName]) -> Self {
        Self {
            identity: identity.clone(),
            missing_types: missing_types.to_vec(),
        }
    }
}
impl ObjectTarget {
    fn ownership_identity(
        &self,
        kind: ObjectKind,
        context: &mut impl ParseContext,
    ) -> Result<RequestedIdentity, AnalysisError> {
        let (name, access_method) = if matches!(
            kind,
            ObjectKind::OPERATOR_CLASS | ObjectKind::OPERATOR_FAMILY
        ) {
            let Some((method, name)) = self.name.split_first() else {
                return AnalysisError::raise_unexpected_input("operator class/family identity");
            };
            (Name(name.to_vec()).full_name()?, Some(method.as_str()))
        } else if let Some(type_name) = &self.type_name {
            (type_name.name.full_name()?, None)
        } else {
            (Name(self.name.clone()).full_name()?, None)
        };
        let mut missing = Vec::new();
        let identity = if let Some(method) = access_method {
            OwnershipIdentity::with_access_method(kind, &name, method)
        } else if let Some(arguments) = &self.argument_types {
            let mut types = Vec::new();
            for argument in arguments {
                if argument.is_output_only() {
                    continue;
                }
                if let Some(ty) = &argument.type_name {
                    let reference = ty.owner_argument_type(context)?;
                    if reference.is_none() {
                        missing.push(ty.name.full_name()?);
                    }
                    types.push(reference);
                } else {
                    types.push(None);
                }
            }
            OwnershipIdentity::with_arguments(kind, &name, &types)
        } else {
            OwnershipIdentity::named(kind, &name)
        };
        Ok(RequestedIdentity::new(&identity, &missing))
    }
}
impl ObjectArgument {
    fn is_output_only(&self) -> bool {
        self.mode.as_deref() == Some("OUT") || self.mode.as_deref() == Some("TABLE")
    }
}
impl super::TypeName {
    fn owner_argument_type(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<TypeReference>, AnalysisError> {
        let requested = self.name.full_name()?;
        Ok(context
            .resolve_type_name(&requested)
            .map(|name| TypeReference::concrete_type_ref(&name, !self.array_bounds.is_empty())))
    }
}
impl Role {
    pub(in crate::ast) fn resolve_owner(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<OwnerSpecification, AnalysisError> {
        match self.kind {
            1 => {
                if let Some(role) = context.role_by_name(&self.name) {
                    Ok(OwnerSpecification::role(role))
                } else {
                    context.report_analysis_problem(AnalysisProblem::ownership_role_not_found(
                        &self.name,
                    ))?;
                    Ok(OwnerSpecification::named(&self.name))
                }
            }
            2 => Ok(OwnerSpecification::current_role()),
            3 => Ok(OwnerSpecification::current_user()),
            4 => Ok(OwnerSpecification::session_user()),
            _ => AnalysisError::raise_unexpected_input("invalid ALTER OWNER role specification"),
        }
    }
}
impl OwnershipIdentity {
    fn resolve(&self, context: &impl ParseContext) -> OwnershipLookup {
        if !self.kind().valid_owner_kind() {
            return OwnershipLookup::invalid_kind(self);
        }
        let names = self.qualified_candidate_names(context);
        if self.needs_visible_routine_resolution() {
            return self.resolve_visible_routines(context, &names);
        }
        for name in names {
            if let Some(outcome) = self.with_name(&name).resolve_qualified_target(context) {
                return outcome;
            }
        }
        OwnershipLookup::not_found(self)
    }

    fn qualified_candidate_names(&self, context: &impl ParseContext) -> Vec<crate::sem::FullName> {
        if !self.kind().owner_schema_scoped() || self.name().schema().is_some() {
            return vec![self.name().clone()];
        }
        let mut names = Vec::new();
        let catalog = crate::sem::SchemaName::from("pg_catalog");
        // PostgreSQL implicitly searches pg_catalog unless it was explicitly placed.
        if !context.get_search_path().contains(&catalog) {
            names.push(self.name().replace_schema(&catalog));
        }
        for schema in context.get_search_path() {
            names.push(self.name().replace_schema(schema));
        }
        names
    }

    fn needs_visible_routine_resolution(&self) -> bool {
        self.arguments().is_none()
            && matches!(
                self.kind(),
                ObjectKind::FUNCTION | ObjectKind::PROCEDURE | ObjectKind::ROUTINE
            )
    }

    fn resolve_qualified_target(&self, context: &impl ParseContext) -> Option<OwnershipLookup> {
        if self.has_incompatible_type_kind(context) {
            return Some(OwnershipLookup::invalid_kind(self));
        }
        let mut identities = self.matching_catalog_identities(context);
        if identities.len() > 1 {
            Some(OwnershipLookup::ambiguous(&identities))
        } else if let Some(identity) = identities.pop() {
            Some(OwnershipLookup::resolved(&identity))
        } else if self.has_incompatible_routine_kind(context) {
            Some(OwnershipLookup::invalid_kind(self))
        } else {
            None
        }
    }

    fn has_incompatible_type_kind(&self, context: &impl ParseContext) -> bool {
        if !matches!(self.kind(), ObjectKind::TYPE | ObjectKind::DOMAIN) {
            return false;
        }
        let Some(definition) = context.get_type_impl(self.name()) else {
            return false;
        };
        match self.kind() {
            ObjectKind::TYPE => {
                definition.is_relation_type() || definition.is_array() == Some(true)
            }
            ObjectKind::DOMAIN => !definition.is_domain(),
            _ => false,
        }
    }

    fn matching_catalog_identities(&self, context: &impl ParseContext) -> Vec<Self> {
        context
            .ownership_definitions(self.kind(), self.name())
            .into_iter()
            .filter(|definition| self.matches(definition.identity()))
            .map(|definition| definition.identity().clone())
            .collect()
    }

    fn has_incompatible_routine_kind(&self, context: &impl ParseContext) -> bool {
        if !matches!(self.kind(), ObjectKind::FUNCTION | ObjectKind::PROCEDURE) {
            return false;
        }
        let requested = if let Some(arguments) = self.arguments() {
            Self::with_arguments(ObjectKind::ROUTINE, self.name(), arguments)
        } else {
            Self::named(ObjectKind::ROUTINE, self.name())
        };
        !requested.matching_catalog_identities(context).is_empty()
    }

    fn resolve_visible_routines(
        &self,
        context: &impl ParseContext,
        names: &[crate::sem::FullName],
    ) -> OwnershipLookup {
        let mut visible = Vec::new();
        for name in names {
            for definition in context.ownership_definitions(ObjectKind::ROUTINE, name) {
                let identity = definition.identity();
                if !Self::contains_signature(&visible, identity) {
                    visible.push(identity.clone());
                }
            }
        }
        let found_other_kind = !visible.is_empty();
        let mut matching: Vec<_> = visible
            .into_iter()
            .filter(|identity| self.kind().accepts_owner_kind(identity.kind()))
            .collect();
        if matching.len() > 1 {
            OwnershipLookup::ambiguous(&matching)
        } else if let Some(identity) = matching.pop() {
            OwnershipLookup::resolved(&identity)
        } else if found_other_kind {
            OwnershipLookup::invalid_kind(self)
        } else {
            OwnershipLookup::not_found(self)
        }
    }

    fn contains_signature(identities: &[Self], candidate: &Self) -> bool {
        for identity in identities {
            if identity.arguments() == candidate.arguments() {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests;

impl super::SyntaxOption {
    pub(in crate::ast) fn owned_relation(
        &self,
    ) -> Result<Option<crate::sem::TableName>, AnalysisError> {
        if self.name != "owned_by" {
            return Ok(None);
        }
        let Some(super::SyntaxValue::Expression(expression)) = &self.value else {
            return AnalysisError::raise_unexpected_input("sequence OWNED BY target");
        };
        expression.owned_relation()
    }
}
impl super::Expr {
    fn owned_relation(&self) -> Result<Option<crate::sem::TableName>, AnalysisError> {
        match self {
            Self::List(parts) => Self::owned_relation_from_parts(parts),
            Self::Column(name) => name.owned_relation(),
            Self::Literal(literal) => literal.owned_relation(),
            _ => AnalysisError::raise_unexpected_input("sequence OWNED BY identity"),
        }
    }
    fn owned_relation_from_parts(
        parts: &[Self],
    ) -> Result<Option<crate::sem::TableName>, AnalysisError> {
        let mut names = Vec::new();
        for part in parts {
            names.push(part.owned_identifier()?);
        }
        Name(names).owned_relation()
    }
    fn owned_identifier(&self) -> Result<String, AnalysisError> {
        match self {
            Self::Literal(literal) => literal.owned_identifier(),
            _ => AnalysisError::raise_unexpected_input("sequence OWNED BY identifier"),
        }
    }
}
impl Name {
    fn owned_relation(&self) -> Result<Option<crate::sem::TableName>, AnalysisError> {
        if self.0.len() == 1 && self.0[0].eq_ignore_ascii_case("none") {
            return Ok(None);
        }
        let Some((_, table)) = self.0.split_last() else {
            return AnalysisError::raise_unexpected_input("sequence OWNED BY column");
        };
        Ok(Some(crate::sem::TableName::from(
            Self(table.to_vec()).full_name()?,
        )))
    }
}
impl super::Literal {
    fn owned_relation(&self) -> Result<Option<crate::sem::TableName>, AnalysisError> {
        if let Self::Keyword(keyword) = self
            && keyword.eq_ignore_ascii_case("none")
        {
            Ok(None)
        } else {
            AnalysisError::raise_unexpected_input("sequence OWNED BY identity")
        }
    }
    fn owned_identifier(&self) -> Result<String, AnalysisError> {
        match self {
            Self::String(name) => Ok(name.clone()),
            _ => AnalysisError::raise_unexpected_input("sequence OWNED BY identifier"),
        }
    }
}
