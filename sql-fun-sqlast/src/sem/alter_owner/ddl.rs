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

#[cfg(test)]
mod test_operator_identity_includes_none_operands {
    use crate::{
        ast::ObjectKind,
        context::{OwnershipInfoRead, ParseContext},
        names::FullName,
        sem::{OwnedObjectDefinition, OwnershipIdentity, TypeReference},
        test_helpers::ownership::{apply_ownership_sql, ownership_context},
    };

    #[rstest::rstest]
    #[case::binary("OPERATOR public.+ (integer, integer)", &[Some(crate::sem::PgBuiltInType::int4()), Some(crate::sem::PgBuiltInType::int4())])]
    #[case::unary("OPERATOR public.! (NONE, integer)", &[None, Some(crate::sem::PgBuiltInType::int4())])]
    fn operator_identity_includes_none_operands(
        #[case] target: &str,
        #[case] arguments: &[Option<TypeReference>],
    ) -> testresult::TestResult {
        let mut context = ownership_context();
        let symbol = if arguments[0].is_none() { "!" } else { "+" };
        let identity = OwnershipIdentity::with_arguments(
            ObjectKind::from(26),
            &FullName::with_schema("public", symbol),
            arguments,
        );
        context.insert_owned_object(&OwnedObjectDefinition::new(&identity, None));
        let result = apply_ownership_sql(context, &format!("ALTER {target} OWNER TO bob"))?;
        assert_eq!(
            result.ast().ownership_change().unwrap().target().identity(),
            Some(&identity)
        );
        assert_eq!(
            result
                .context()
                .ownership_definitions(ObjectKind::from(26), identity.name())[0]
                .owner(),
            Some(result.ast().ownership_change().unwrap().owner())
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::out_argument(
        "CREATE FUNCTION public.f(IN a integer, OUT b integer) LANGUAGE sql AS 'SELECT a'",
        "FUNCTION public.f(integer)"
    )]
    #[case::named_signature(
        "CREATE FUNCTION public.f(a integer) RETURNS integer LANGUAGE sql AS 'SELECT a'",
        "FUNCTION public.f(IN a integer)"
    )]
    #[case::variadic_signature(
        "CREATE FUNCTION public.f(VARIADIC a text[]) RETURNS integer LANGUAGE sql AS 'SELECT 1'",
        "FUNCTION public.f(VARIADIC text[])"
    )]
    #[case::empty_signature(
        "CREATE FUNCTION public.f() RETURNS integer LANGUAGE sql AS 'SELECT 1'",
        "FUNCTION public.f()"
    )]
    #[case::unspecified_signature(
        "CREATE FUNCTION public.f(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1'",
        "FUNCTION public.f"
    )]
    fn routine_signatures_use_input_identity(
        #[case] create: &str,
        #[case] target: &str,
    ) -> testresult::TestResult {
        let context = apply_ownership_sql(ownership_context(), create)?.into_context();
        let result = apply_ownership_sql(context, &format!("ALTER {target} OWNER TO alice"))?;
        assert!(
            result.context().problems().is_empty(),
            "{:?}",
            result.context().problems()
        );
        assert!(
            result
                .ast()
                .ownership_change()
                .unwrap()
                .target()
                .identity()
                .is_some()
        );
        Ok(())
    }

    #[test]
    fn all_missing_argument_type_names_are_retained() -> testresult::TestResult {
        let result = apply_ownership_sql(
            ownership_context(),
            "ALTER FUNCTION public.f(missing_one, missing_two) OWNER TO alice",
        )?;
        let outcome = result.ast().ownership_change().unwrap().target();
        assert_eq!(
            outcome.unresolved_argument_types(),
            &[
                FullName::from_local_name("missing_one"),
                FullName::from_local_name("missing_two")
            ]
        );
        assert_eq!(
            result.context().problems()[0].code(),
            "ownership_argument_type_not_found"
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::zero_argument("AGGREGATE public.total(*)", &[])]
    #[case::ordered_set("AGGREGATE public.total(integer ORDER BY integer)", &[Some(crate::sem::PgBuiltInType::int4()), Some(crate::sem::PgBuiltInType::int4())])]
    fn aggregate_signatures_identify_the_exact_definition(
        #[case] target: &str,
        #[case] arguments: &[Option<TypeReference>],
    ) -> testresult::TestResult {
        let mut context = ownership_context();
        let identity = OwnershipIdentity::with_arguments(
            ObjectKind::from(2),
            &FullName::with_schema("public", "total"),
            arguments,
        );
        context.insert_owned_object(&OwnedObjectDefinition::new(&identity, None));
        let result = apply_ownership_sql(context, &format!("ALTER {target} OWNER TO alice"))?;
        assert_eq!(
            result.ast().ownership_change().unwrap().target().identity(),
            Some(&identity)
        );
        Ok(())
    }
}
