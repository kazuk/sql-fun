use sql_fun_core::IVec;

use crate::{
    sem::AstAndContextPair,
    syn::{ListOpt, ScanToken},
};

use super::{AnalysisError, ParseContext, SemAst};

/// analyzed [`crate::syn::CreateSchemaStmt`]
#[derive(Debug, Clone)]
pub struct CreateSchema {
    name: String,
    #[expect(dead_code)]
    elements: Vec<SemAst>,
}

impl CreateSchema {
    /// new instance
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// analyze [`crate::syn::CreateSchemaStmt`]
    pub fn analyze_create_schema<TParseContext>(
        mut context: TParseContext,
        syn: crate::syn::CreateSchemaStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let name = syn.get_schemaname();
        let schema_name = Some(name.clone());
        let Some(elts) = syn.get_schema_elts().as_inner() else {
            AnalysisError::raise_unexpected_none("create_schema_stmt.schema_elts")?
        };

        let mut sem_items = Vec::new();
        for elt in elts {
            let AstAndContextPair(sem, new_context) =
                SemAst::analyze_node(context, &schema_name, elt, tokens)?;
            context = new_context;
            sem_items.push(sem);
        }
        let schema = Self {
            name: name.clone(),
            elements: sem_items,
        };
        let context = context.apply_create_schema(&schema)?;
        Ok(AstAndContextPair::new(
            SemAst::CreateSchema(schema),
            context,
        ))
    }
}
