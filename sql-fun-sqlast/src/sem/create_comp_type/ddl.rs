//! Analysis owned by the semantic definitions in this module.

use crate::{
    AstAndContextPair,
    ast::ddl::{CompositeField as AstCompositeField, CompositeType as AstCompositeType},
    context::ParseContext,
    names::{ColumnName, FullName, TableName},
    sem::{
        AnalysisError, AnalysisProblem, CompositeFieldCollection, CompositeFieldDefinition,
        CreateCompositeType, SemAst, TypeReference, WithTypmod,
    },
};

impl CreateCompositeType {
    pub(crate) fn from_ast(
        ast: &AstCompositeType,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let name = FullName::from_ast_name(ast.name())?;
        let table = TableName::from(name.clone());
        let mut fields = CompositeFieldCollection::default();
        for field in ast.fields() {
            fields.push(CompositeFieldDefinition::from_ast(field, context, &table)?);
        }
        Ok(Self::new(&name, &fields))
    }

    pub(crate) fn from_ast_and_apply<TContext: ParseContext>(
        ast: &AstCompositeType,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let composite = Self::from_ast(ast, &mut context)?;
        let context = context.apply_create_comp_type(&composite)?;
        Ok(AstAndContextPair::new(
            SemAst::create_composite_type(&composite),
            context,
        ))
    }
}

impl CompositeFieldDefinition {
    pub(crate) fn from_ast(
        ast: &AstCompositeField,
        context: &mut impl ParseContext,
        composite: &TableName,
    ) -> Result<Self, AnalysisError> {
        let requested = TypeReference::from_ast(ast.type_name())?;
        let type_ref = context.canonicalize_type_ref(&requested);
        if type_ref.is_none() {
            context.report_analysis_problem(AnalysisProblem::column_type_not_found(
                composite,
                &ColumnName::from(ast.name().as_str()),
                requested
                    .full_name()
                    .expect("SQL type syntax constructs a concrete reference"),
            ))?;
        }
        let mut field = Self::new(ast.name(), &type_ref, &Some(ast.is_not_null()));
        field.retain_requested_type(&WithTypmod::<TypeReference>::from_ast_declaration(
            ast.type_name(),
            type_ref.as_ref(),
        )?);
        let lookup = ast
            .collation()
            .as_ref()
            .map(FullName::from_ast_name)
            .transpose()?
            .map(|name| context.lookup_collation(&name));
        field.validate_collation(context, lookup.as_ref())?;
        Ok(field)
    }
}

#[cfg(test)]
mod test_composite_field_resolution {
    use crate::{
        ast::{TryFromWithTokens, ddl::CompositeType},
        names::{FullName, TableName},
        sem::{CompositeFieldDefinition, PgBuiltInType, TypeReference},
        syn::Opt,
        test_helpers::{SynBuilder, TestParseContext},
    };
    use ::sql_fun_core::IVec;

    #[rstest::rstest]
    #[case::array("integer[]", true)]
    #[case::dimensions("integer[][]", true)]
    #[case::bound("integer[3]", true)]
    #[case::scalar("integer", false)]
    fn retains_declared_array_identity(#[case] declaration: &str, #[case] is_array: bool) {
        let syntax = SynBuilder::new().parse(&format!(
            "CREATE TYPE public.array_value AS (items {declaration})"
        ));
        let composite = CompositeType::try_from_with_tokens(
            &syntax.as_composite_type_stmt().as_inner().unwrap(),
            &IVec::default(),
        )
        .unwrap();
        let mut context = TestParseContext::default();
        context.setup_type(PgBuiltInType::int4());
        let field = CompositeFieldDefinition::from_ast(
            &composite.fields()[0],
            &mut context,
            &TableName::from(FullName::with_schema("public", "array_value")),
        )
        .unwrap();
        assert_eq!(field.name(), "items");
        assert_eq!(
            field.get_type(),
            Some(TypeReference::concrete_type_ref(
                PgBuiltInType::int4()
                    .full_name()
                    .expect("catalog declaration has a nominal type identity"),
                is_array,
            ))
        );
    }
}
