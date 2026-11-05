use crate::{
    CacheContext,
    ast::TableAs,
    names::{ColumnName, TableName},
    sem::{
        ColumnRename, OwnerSpecification, ResultColumnCollection,
        traversal::{ColumnRenamer, RelationRenamer, SemanticVisitorMut},
    },
    syn::CreateTableAsStmt,
};
use sql_fun_core::IVec;

use crate::{
    ast::TryFromWithTokens,
    sem::{
        AstAndContextPair, SourcePathWithSpan, alter_table::AlterObjSubCommand,
        create_table::ColumnDefinition,
    },
    syn::{ObjectType, ScanToken},
};

use crate::{
    names::FullName,
    sem::{AlterView, AnalysisError, ParseContext, SemAst},
};

/// analyzed [`crate::syn::CreateTableAsStmt`]
#[derive(Debug, Clone)]
pub struct CreateTableAs {
    if_not_exists: bool,

    name: FullName,
    query: Box<SemAst>,
    obj_type: ObjectType,
    owner: Option<OwnerSpecification>,
    alter_statements: Vec<AlterView>,
}

impl CreateTableAs {
    pub(crate) fn set_operations_valid(&self) -> Result<bool, AnalysisError> {
        self.query
            .select_statement()
            .expect("CREATE AS query is a SELECT")
            .set_operations_valid()
    }

    pub(crate) fn walk_column_dependencies(
        &mut self,
        visitor: &mut impl SemanticVisitorMut,
    ) -> Result<(), AnalysisError> {
        self.query.walk_nested_command(visitor)
    }

    pub(crate) fn rename_column(&mut self, rename: &ColumnRename) -> Result<(), AnalysisError> {
        self.query
            .walk_nested_command(&mut ColumnRenamer::new(rename))
    }

    pub(crate) fn rename_relation(
        &mut self,
        old: &TableName,
        new: &TableName,
    ) -> Result<(), AnalysisError> {
        self.query
            .walk_nested_command(&mut RelationRenamer::new(old, new))
    }

    pub(crate) fn result_columns(&self) -> &ResultColumnCollection {
        self.query
            .select_statement()
            .expect("CREATE AS query is a SELECT")
            .result_columns()
    }

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

    pub(crate) fn new(name: &FullName, query: &SemAst, materialized: bool) -> Self {
        Self {
            if_not_exists: false,
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
                AlterObjSubCommand::ChangeOwner(new_owner) => {
                    if new_owner.can_apply() {
                        self.change_owner(new_owner);
                    }
                }
                _ => todo!(),
            }
        }

        Ok(())
    }

    pub(crate) fn get_column_def(&self, column: &ColumnName) -> Option<ColumnDefinition> {
        self.result_columns().get_result_column_def(column)
    }
    pub(crate) fn has_column(&self, column: &ColumnName) -> bool {
        self.result_columns().has_column(column)
    }

    /// analyze [`crate::syn::CreateTableAsStmt`]
    #[tracing::instrument(skip_all)]
    pub fn analyze_create_table_as<TParseContext>(
        context: TParseContext,
        _parent_schema: &Option<String>,
        syn: CreateTableAsStmt,
        tokens: &IVec<ScanToken>,
        source_span: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Self::from_ast_and_apply_at(
            &TableAs::try_from_with_tokens(&syn, tokens)?,
            context,
            &mut CacheContext::new(),
            source_span,
        )
    }
}
