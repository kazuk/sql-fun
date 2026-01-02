use crate::{
    sem::{AstAndContextPair, alter_table::AlterObjSubCommand, create_table::ColumnDefinition},
    syn::{ObjectType, Opt},
};

use super::{AlterView, AnalysisError, CreateTable, FullName, ParseContext, SemAst, TableName};

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
    /// object name
    #[must_use]
    pub fn name(&self) -> &FullName {
        &self.name
    }

    /// object type
    #[must_use]
    pub fn obj_type(&self) -> ObjectType {
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
}

/// analyze [`crate::syn::CreateTableAsStmt`]
#[tracing::instrument(skip_all)]
pub fn analyze_create_table_as<TParseContext>(
    context: TParseContext,
    parent_schema: &Option<String>,
    syn: crate::syn::CreateTableAsStmt,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    let Some(obj_type) = syn.get_objtype().as_inner() else {
        AnalysisError::raise_unexpected_none("create_table_as.obj_type")?
    };

    let Some(into) = syn.get_into().as_inner() else {
        AnalysisError::raise_unexpected_none("create_table_as.into")?
    };
    let Some(into_rel) = into.get_rel().as_inner() else {
        AnalysisError::raise_unexpected_none("into_clause.rel")?
    };
    let name = FullName::try_from(into_rel.clone())?;
    let Some(q) = syn.get_query().as_inner() else {
        AnalysisError::raise_unexpected_none("create_table_as.query")?
    };
    let AstAndContextPair(sem, context) = super::analyze_node(context, parent_schema, q)?;
    let context = match &sem {
        SemAst::SelectStatement(s) => {
            let columns = s.result_columns().column_definitions()?;
            let create_table = CreateTable::new(TableName::try_from(into_rel)?, columns);
            context.apply_create_table(&create_table)?
        }
        _ => AnalysisError::raise_unexpected_none("SemAst As Select statement")?,
    };
    let create_table_as = CreateTableAs {
        name,
        query: Box::new(sem),
        obj_type,
        owner: None,
        alter_statements: Vec::new(),
    };
    let result_context = context.apply_create_table_as(&create_table_as)?;
    Ok(AstAndContextPair::new(
        SemAst::CreateTableAs(create_table_as),
        result_context,
    ))
}
