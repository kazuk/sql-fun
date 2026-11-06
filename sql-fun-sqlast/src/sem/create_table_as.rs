use crate::{
    CacheContext,
    ast::{CreateTableMode, TableAs},
    names::{ColumnName, FullName, TableName},
    sem::{
        AlterView, AnalysisError, AnalysisProblem, AstAndContextPair, ColumnRename, CreateTable,
        Kind, OwnerSpecification, ParseContext, ResultColumnCollection, SelectStatement, SemAst,
        SourcePathWithSpan,
        alter_table::AlterObjSubCommand,
        create_table::ColumnDefinition,
        problem::CreationProblem,
        traversal::{ColumnRenamer, RelationRenamer, SemanticVisitorMut},
    },
};

/// Analyzed CREATE TABLE AS statement.
#[derive(Debug, Clone)]
pub struct CreateTableAs {
    if_not_exists: bool,

    name: FullName,
    query: Box<SemAst>,
    obj_type: Kind,
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
                Kind::MaterializedView
            } else {
                Kind::Table
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
    pub const fn obj_type(&self) -> Kind {
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
}

// Analysis owned by the semantic definitions in this module.

impl CreateTableAs {
    pub(crate) fn from_ast_and_apply<TContext: ParseContext>(
        ast: &TableAs,
        context: TContext,
        cache: &mut CacheContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let Some(source) = context.get_source_path_with_span().cloned() else {
            return AnalysisError::raise_unexpected_none("source span");
        };
        Self::from_ast_and_apply_at(ast, context, cache, &source)
    }

    pub(crate) fn from_ast_and_apply_at<TContext: ParseContext>(
        ast: &TableAs,
        mut context: TContext,
        cache: &mut CacheContext,
        source: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let mut query = SelectStatement::from_ast(ast.query(), &mut context, cache)?;
        let name = FullName::from_ast_name(ast.into().relation().name())?;
        if ast.into().columns().len() > query.result_columns().len() {
            context.report_problem(AnalysisProblem::creation(CreationProblem::named(
                &name,
                "too many output column names",
                source.span(),
            )))?;
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        query.rename_columns(ast.into().columns());
        let columns = query.result_columns().column_definitions()?;
        let mut names = Vec::new();
        for column in &columns {
            if names.contains(&column.name()) {
                context.report_problem(AnalysisProblem::creation(CreationProblem::named(
                    &name,
                    "duplicate output column name",
                    source.span(),
                )))?;
                return Ok(AstAndContextPair::new(
                    SemAst::unresolved_statement(),
                    context,
                ));
            }
            names.push(column.name());
        }
        let table = CreateTable::new(
            &TableName::from(name.clone()),
            columns,
            CreateTableMode::from_if_not_exists(ast.if_not_exists()),
            ast.into().relation().persistence() == "t",
            source,
        );
        let table_as = Self::new(&name, &SemAst::select(&query), ast.materialized())
            .with_if_not_exists(ast.if_not_exists());
        let context = if ast.materialized() {
            context.apply_create_table_as(&table_as)?
        } else {
            context.apply_create_table(&table)?
        };
        Ok(AstAndContextPair::new(
            SemAst::create_table_as(&table_as),
            context,
        ))
    }
}
