use crate::{
    sem::{
        AlterView, AstAndContextPair,
        alter_table::AlterObjSubCommand,
        create_table::{ColumnDefinition, ColumnName},
    },
    syn::Opt,
};

use super::{AnalysisError, FullName, ParseContext, SelectStatement, SemAst, analyze_select};

/// analyzed [`crate::syn::ViewStmt`]
#[derive(Debug, Clone)]
pub struct CreateView {
    name: FullName,
    query: SelectStatement,
    alter_statements: Vec<AlterView>,
    owner: Option<String>,
    materialized: bool,
}

impl CreateView {
    /// create instance
    #[must_use]
    pub fn new(
        name: &FullName,
        query: &SelectStatement,
        alter_statements: &[AlterView],
        owner: &Option<String>,
        materialized: bool,
    ) -> Self {
        Self {
            name: name.clone(),
            query: query.clone(),
            alter_statements: alter_statements.to_owned(),
            owner: owner.clone(),
            materialized,
        }
    }

    /// name of view
    #[must_use]
    pub fn name(&self) -> &FullName {
        &self.name
    }

    /// get result set column
    #[must_use]
    pub fn get_column_def(&self, column_name: &ColumnName) -> Option<ColumnDefinition> {
        self.query.get_result_column_def(column_name)
    }

    /// test column existing
    #[must_use]
    pub fn has_column(&self, column: &ColumnName) -> bool {
        self.query.has_column(column)
    }

    /// apply `ALTER VIEW`
    pub fn apply_alter(&mut self, alter_view: &AlterView) -> Result<(), AnalysisError> {
        self.alter_statements.push(alter_view.clone());

        for cmd in alter_view.commands() {
            match cmd {
                AlterObjSubCommand::ChangeOwner(new_owner) => self.owner = Some(new_owner.clone()),
                _ => todo!(),
            }
        }

        Ok(())
    }

    /// view is materialized
    #[must_use]
    pub fn is_materialized(&self) -> bool {
        self.materialized
    }
}

/// analyze [`crate::syn::ViewStmt`]
#[tracing::instrument(skip(context))]
pub fn analyze_create_view<TParseContext>(
    context: TParseContext,
    parent_schema: &Option<String>,
    syn: crate::syn::ViewStmt,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    let Some(view) = syn.get_view().as_inner() else {
        AnalysisError::raise_unexpected_none("view_stmt.view")?
    };
    let _materialized = syn.get_options();

    let Some(query) = syn.get_query().as_select_stmt().as_inner() else {
        AnalysisError::raise_unexpected_none("view_stmt.query")?
    };

    let AstAndContextPair(select, context) = analyze_select(context, query)?;
    let SemAst::SelectStatement(stmt) = select else {
        AnalysisError::raise_unexpected_none("select_statement")?
    };
    let name = FullName::try_from(view)?;
    let create_view = CreateView::new(&name, &stmt, &[], &Default::default(), false);
    let context = context.apply_create_view(&create_view)?;
    Ok(AstAndContextPair::new(
        SemAst::CreateView(create_view),
        context,
    ))
}
