//! Analysis owned by the semantic definitions in this module.

use crate::{
    ast::{
        AlterOwner as AstAlterOwner, Name as AstName, ObjectKind, ObjectTarget as AstObjectTarget,
    },
    context::ParseContext,
    names::FullName,
    sem::{
        AlterOwner, AnalysisError, AnalysisProblem, AstAndContextPair, OwnerSpecification,
        OwnershipIdentity, OwnershipLookup, SemAst,
    },
};

impl AlterOwner {
    pub(crate) fn from_ast(
        ast: &AstAlterOwner,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let requested = RequestedIdentity::from_ast_owner_target(ast, context)?;
        let target = if !requested.missing_types.is_empty() {
            OwnershipLookup::argument_type_not_found(&requested.identity, &requested.missing_types)
        } else {
            requested.identity.resolve(context)
        };
        if target.identity().is_none() {
            context.report_analysis_problem(AnalysisProblem::ownership_target(&target))?;
        }
        let owner = OwnerSpecification::from_ast(ast.owner(), context)?;
        Ok(Self::new(&target, &owner))
    }

    pub(crate) fn from_ast_and_apply<C: ParseContext>(
        ast: &AstAlterOwner,
        mut context: C,
    ) -> Result<AstAndContextPair<C>, AnalysisError> {
        let alteration = Self::from_ast(ast, &mut context)?;
        let context = context.apply_alter_owner(&alteration)?;
        Ok(AstAndContextPair::new(
            SemAst::alter_owner(&alteration),
            context,
        ))
    }
}

impl RequestedIdentity {
    fn from_ast_owner_target(
        ast: &AstAlterOwner,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let Some(kind) = ast.object_kind() else {
            return AnalysisError::raise_unexpected_none("alter_owner.object_kind");
        };
        if let Some(relation) = &ast.relation() {
            return Ok(Self::new(
                &OwnershipIdentity::named(kind, &FullName::from_ast_name(relation)?),
                &[],
            ));
        }
        let Some(object) = &ast.object() else {
            return AnalysisError::raise_unexpected_none("alter_owner.object");
        };
        Self::from_ast_object_target(object, kind, context)
    }
}

struct RequestedIdentity {
    identity: OwnershipIdentity,
    missing_types: Vec<FullName>,
}

impl RequestedIdentity {
    fn new(identity: &OwnershipIdentity, missing_types: &[FullName]) -> Self {
        Self {
            identity: identity.clone(),
            missing_types: missing_types.to_vec(),
        }
    }

    fn from_ast_object_target(
        ast: &AstObjectTarget,
        kind: ObjectKind,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let (name, access_method) = if matches!(
            kind,
            ObjectKind::OPERATOR_CLASS | ObjectKind::OPERATOR_FAMILY
        ) {
            let Some((method, name)) = ast.name().split_first() else {
                return AnalysisError::raise_unexpected_input("operator class/family identity");
            };
            (
                FullName::from_ast_name(&AstName::from_parts(name))?,
                Some(method.as_str()),
            )
        } else if let Some(type_name) = &ast.type_name() {
            (FullName::from_ast_name(type_name.name())?, None)
        } else {
            (
                FullName::from_ast_name(&AstName::from_parts(ast.name()))?,
                None,
            )
        };
        let mut missing = Vec::new();
        let identity = if let Some(method) = access_method {
            OwnershipIdentity::with_access_method(kind, &name, method)
        } else if let Some(arguments) = &ast.argument_types() {
            let mut types = Vec::new();
            for argument in arguments {
                if argument.is_output_only() {
                    continue;
                }
                if let Some(ty) = &argument.type_name() {
                    let reference = OwnershipIdentity::argument_type_from_ast(ty, context)?;
                    if reference.is_none() {
                        missing.push(FullName::from_ast_name(ty.name())?);
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
        Ok(Self::new(&identity, &missing))
    }
}
