use crate::{
    CacheContext,
    ast::{Schema, TryFromWithTokens},
    sem::{AnalysisError, AstAndContextPair, OwnerSpecification, ParseContext, SemAst},
    syn::{CreateSchemaStmt, ScanToken},
};
use sql_fun_core::IVec;

/// analyzed [`crate::syn::CreateSchemaStmt`]
#[derive(Debug, Clone)]
pub struct CreateSchema {
    if_not_exists: bool,

    owner: Option<OwnerSpecification>,
    name: String,
    #[expect(dead_code)]
    elements: Vec<SemAst>,
}

impl CreateSchema {
    pub(crate) const fn if_not_exists(&self) -> bool {
        self.if_not_exists
    }
    pub(crate) const fn with_if_not_exists(mut self, value: bool) -> Self {
        self.if_not_exists = value;
        self
    }

    /// Retained ownership; None means no ownership information was supplied.
    pub const fn owner(&self) -> Option<&OwnerSpecification> {
        self.owner.as_ref()
    }
    /// Commit an ownership change, leaving the rest of the definition intact.
    pub(crate) fn change_owner(&mut self, owner: &OwnerSpecification) {
        self.owner = Some(owner.clone());
    }

    pub(crate) fn new(name: &str, elements: &[SemAst]) -> Self {
        Self {
            if_not_exists: false,
            owner: None,
            name: name.to_owned(),
            elements: elements.to_vec(),
        }
    }
    /// new instance
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// analyze [`crate::syn::CreateSchemaStmt`]
    pub fn analyze_create_schema<TParseContext>(
        context: TParseContext,
        syn: CreateSchemaStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Self::from_ast_and_apply(
            &Schema::try_from_with_tokens(&syn, tokens)?,
            context,
            &mut CacheContext::new(),
        )
    }
}

// Analysis owned by the semantic definitions in this module.

impl CreateSchema {
    pub(crate) fn from_ast_and_apply<TContext: ParseContext>(
        ast: &Schema,
        context: TContext,
        cache: &mut CacheContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        context.apply_creation_group(|context| Self::from_ast_group(ast, context, cache))
    }

    pub(crate) fn from_ast_group<TContext: ParseContext>(
        ast: &Schema,
        mut context: TContext,
        cache: &mut CacheContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        if context.schema_definition(ast.name()).is_some() {
            let schema = Self::new(ast.name(), &[]).with_if_not_exists(ast.if_not_exists());
            let context = context.apply_create_schema(&schema)?;
            return Ok(AstAndContextPair::new(
                SemAst::create_schema(&schema),
                context,
            ));
        }
        let owner = if let Some(authorization) = &ast.authorization() {
            Some(OwnerSpecification::from_ast(authorization, &mut context)?)
        } else {
            None
        };
        let previous_path = context.get_search_path().to_vec();
        let mut schema_path = vec![crate::names::SchemaName::from(ast.name().as_str())];
        for schema in &previous_path {
            if !schema_path.contains(schema) {
                schema_path.push(schema.clone());
            }
        }
        context.set_search_path(&schema_path);
        let mut elements = Vec::new();
        for element in ast.elements() {
            let AstAndContextPair(element, next) = SemAst::apply_ast_node(element, context, cache)?;
            context = next;
            elements.push(element);
        }
        context.set_search_path(&previous_path);
        let mut schema = Self::new(ast.name(), &elements).with_if_not_exists(ast.if_not_exists());
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
