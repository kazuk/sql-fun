use sql_fun_core::IVec;

use crate::{
    sem::unresolved::TryFromWithTokens,
    sem::{
        AstAndContextPair, SourcePathWithSpan, alter_table::AlterObjSubCommand,
        create_table::ColumnDefinition,
    },
    syn::{ObjectType, ScanToken},
};

use super::{AlterView, AnalysisError, FullName, ParseContext, SemAst};

/// analyzed [`crate::syn::CreateTableAsStmt`]
#[derive(Debug, Clone)]
pub struct CreateTableAs {
    name: FullName,
    #[expect(dead_code)]
    query: Box<SemAst>,
    obj_type: ObjectType,
    owner: Option<String>,
    alter_statements: Vec<AlterView>,
}

impl CreateTableAs {
    pub(crate) fn new(name: &FullName, query: &SemAst, materialized: bool) -> Self {
        Self {
            name: name.clone(),
            query: Box::new(query.clone()),
            obj_type: if materialized {
                ObjectType::ObjectMatview
            } else {
                ObjectType::ObjectTable
            },
            owner: None,
            alter_statements: Vec::new(),
        }
    }
    /// object name
    #[must_use]
    pub const fn name(&self) -> &FullName {
        &self.name
    }

    /// object type
    #[must_use]
    pub const fn obj_type(&self) -> ObjectType {
        self.obj_type
    }

    pub(crate) fn apply_alter(&mut self, alter_view: &AlterView) -> Result<(), AnalysisError> {
        self.alter_statements.push(alter_view.clone());

        for cmd in alter_view.commands() {
            match cmd {
                AlterObjSubCommand::ChangeOwner(new_owner) => self.owner = Some(new_owner.clone()),
                _ => todo!(),
            }
        }

        Ok(())
    }

    pub(crate) fn get_column_def(
        &self,
        _column_name: &super::ColumnName,
    ) -> Option<ColumnDefinition> {
        todo!()
    }

    pub(crate) fn has_column(&self, _column_name: &super::ColumnName) -> bool {
        todo!()
    }

    /// analyze [`crate::syn::CreateTableAsStmt`]
    #[tracing::instrument(skip_all)]
    pub fn analyze_create_table_as<TParseContext>(
        context: TParseContext,
        _parent_schema: &Option<String>,
        syn: crate::syn::CreateTableAsStmt,
        tokens: &IVec<ScanToken>,
        source_span: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        crate::sem::unresolved::TableAs::try_from_with_tokens(&syn, tokens)?.resolve_and_apply_at(
            context,
            crate::sem::unresolved::Dialect::postgres(),
            source_span,
        )
    }
}
