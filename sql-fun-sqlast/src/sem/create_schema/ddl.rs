//! Analysis owned by the semantic definitions in this module.

use crate::{
    AstAndContextPair, CacheContext,
    ast::ddl::Schema as AstSchema,
    context::ParseContext,
    sem::{AnalysisError, CreateSchema, OwnerSpecification, SemAst},
};

impl CreateSchema {
    pub(crate) fn from_ast_and_apply<TContext: ParseContext>(
        ast: &AstSchema,
        context: TContext,
        cache: &mut CacheContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        context.apply_creation_group(|context| Self::from_ast_group(ast, context, cache))
    }

    pub(crate) fn from_ast_group<TContext: ParseContext>(
        ast: &AstSchema,
        mut context: TContext,
        cache: &mut CacheContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        if context.schema_definition(&ast.name).is_some() {
            let schema = Self::new(&ast.name, &[]).with_if_not_exists(ast.if_not_exists);
            let context = context.apply_create_schema(&schema)?;
            return Ok(AstAndContextPair::new(
                SemAst::create_schema(&schema),
                context,
            ));
        }
        let owner = if let Some(authorization) = &ast.authorization {
            Some(OwnerSpecification::from_ast(authorization, &mut context)?)
        } else {
            None
        };
        let previous_path = context.get_search_path().to_vec();
        let mut schema_path = vec![crate::names::SchemaName::from(ast.name.as_str())];
        for schema in &previous_path {
            if !schema_path.contains(schema) {
                schema_path.push(schema.clone());
            }
        }
        context.set_search_path(&schema_path);
        let mut elements = Vec::new();
        for element in &ast.elements {
            let AstAndContextPair(element, next) = SemAst::apply_ast_node(element, context, cache)?;
            context = next;
            elements.push(element);
        }
        context.set_search_path(&previous_path);
        let mut schema = Self::new(&ast.name, &elements).with_if_not_exists(ast.if_not_exists);
        if let Some(owner) = owner {
            schema.change_owner(&owner);
        }
        let context = context.apply_create_schema(&schema)?;
        Ok(AstAndContextPair::new(
            SemAst::create_schema(&schema),
            context,
        ))
    }
}
