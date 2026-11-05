//! Analysis owned by the semantic definitions in this module.

use crate::{
    AstAndContextPair, CacheContext,
    ast::{Statement as AstStatement, StatementNode as AstStatementNode},
    context::ParseContext,
    sem::{
        AlterOwner, AlterSequence, AnalysisError, Comment, CreateCompositeType, CreateDomain,
        CreateEnum, CreateExtension, CreateFunction, CreateIndex, CreateRule, CreateSchema,
        CreateSequence, CreateTable, CreateTableAs, CreateTrigger, CreateView, DeleteStatement,
        DropRule, DropTable, DropTrigger, EngineFeatureValidation, InsertStatement, MergeStatement,
        ParameterConstraints, RelationAlteration, SelectStatement, SemAst, UpdateStatement,
        VariableSet,
    },
};

impl SemAst {
    pub(crate) fn from_ast_node(
        ast: &AstStatementNode,
        context: &mut impl ParseContext,
        cache: &mut CacheContext,
    ) -> Result<Self, AnalysisError> {
        if !EngineFeatureValidation::statement(ast, context)? {
            return Ok(Self::unresolved_statement());
        }
        match ast {
            AstStatementNode::DefaultPrivileges(_) => {
                todo!("ALTER DEFAULT PRIVILEGES interpretation (#955)")
            }
            AstStatementNode::ForeignTable(_) => {
                todo!("CREATE FOREIGN TABLE interpretation (#956)")
            }
            AstStatementNode::Query(query) => Ok(Self::select(&SelectStatement::from_ast(
                query, context, cache,
            )?)),
            AstStatementNode::Insert(insert) => Ok(Self::from_insert_resolution(
                InsertStatement::from_ast(insert, context, cache)?.as_ref(),
            )),
            AstStatementNode::Update(update) => Ok(Self::from_update_resolution(
                UpdateStatement::from_ast(update, context, cache)?.as_ref(),
            )),
            AstStatementNode::Delete(delete) => Ok(Self::delete(&DeleteStatement::from_ast(
                delete, context, cache,
            )?)),
            AstStatementNode::Merge(merge) => Ok(Self::merge(&MergeStatement::from_ast(
                merge, context, cache,
            )?)),
            AstStatementNode::CreateRule(rule) => Ok(Self::from_create_rule_resolution(
                CreateRule::from_ast(rule, context)?.as_ref(),
            )),
            AstStatementNode::DropTrigger(trigger) => Ok(Self::from_drop_trigger_resolution(
                DropTrigger::from_ast(trigger, context)?.as_ref(),
            )),
            AstStatementNode::CreateTrigger(trigger) => Ok(Self::from_create_trigger_resolution(
                CreateTrigger::from_ast(trigger, context)?.as_ref(),
            )),
            AstStatementNode::DropRule(rule) => Ok(Self::from_drop_rule_resolution(
                DropRule::from_ast(rule, context)?.as_ref(),
            )),
            AstStatementNode::View(_) => {
                AnalysisError::raise_unexpected_input("view requires catalog application")
            }
            AstStatementNode::Extension(_) => {
                AnalysisError::raise_unexpected_input("extension requires catalog application")
            }
            AstStatementNode::VariableSet(_) => {
                AnalysisError::raise_unexpected_input("variable set requires catalog application")
            }
            AstStatementNode::EnumType(_) => {
                AnalysisError::raise_unexpected_input("enum type requires catalog application")
            }
            AstStatementNode::Sequence(_) => {
                AnalysisError::raise_unexpected_input("sequence requires catalog application")
            }
            AstStatementNode::DropTable(_) => {
                AnalysisError::raise_unexpected_input("drop table requires catalog application")
            }
            AstStatementNode::AlterSequence(_) => {
                AnalysisError::raise_unexpected_input("alter sequence requires catalog application")
            }
            AstStatementNode::Comment(_) => {
                AnalysisError::raise_unexpected_input("comment requires catalog application")
            }
            AstStatementNode::AlterOwner(alteration) => Ok(Self::alter_owner(
                &AlterOwner::from_ast(alteration, context)?,
            )),
            AstStatementNode::CompositeType(composite_type) => Ok(Self::create_composite_type(
                &CreateCompositeType::from_ast(composite_type, context)?,
            )),
            AstStatementNode::AlterRelation(_) => {
                AnalysisError::raise_unexpected_input("statement requires catalog application")
            }
            AstStatementNode::Function(function) => Ok(Self::create_function(
                &CreateFunction::from_ast(function, context)?,
            )),
            AstStatementNode::Index(index) => Ok(Self::from_index_resolution(
                &CreateIndex::from_ast(index, context)?,
            )),
            AstStatementNode::TableAs(_) => {
                AnalysisError::raise_unexpected_input("table as requires catalog application")
            }
            AstStatementNode::Table(_) => {
                AnalysisError::raise_unexpected_input("table requires source metadata")
            }
            AstStatementNode::Schema(_) => {
                AnalysisError::raise_unexpected_input("schema requires catalog application")
            }
            AstStatementNode::Domain(domain) => Ok(Self::create_domain(&CreateDomain::from_ast(
                domain, context,
            )?)),
        }
    }
    pub(crate) fn apply_ast_node<TContext>(
        ast: &AstStatementNode,
        context: TContext,
        cache: &mut CacheContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let mut context = context;
        context.set_parameter_constraints(&ParameterConstraints::default());
        if !EngineFeatureValidation::statement(ast, &mut context)? {
            return Ok(AstAndContextPair::new(
                Self::unresolved_statement(),
                context,
            ));
        }
        match ast {
            AstStatementNode::CreateTrigger(trigger) => CreateTrigger::apply_ast(trigger, context),
            AstStatementNode::DropTrigger(trigger) => DropTrigger::apply_ast(trigger, context),
            AstStatementNode::CreateRule(rule) => CreateRule::apply_ast(rule, context),
            AstStatementNode::DropRule(rule) => DropRule::apply_ast(rule, context),
            AstStatementNode::View(view) => CreateView::from_ast_and_apply(view, context, cache),
            AstStatementNode::Extension(extension) => {
                CreateExtension::from_ast_and_apply(extension, context)
            }
            AstStatementNode::VariableSet(variable_set) => {
                VariableSet::from_ast_and_apply(variable_set, context)
            }
            AstStatementNode::EnumType(enum_type) => {
                CreateEnum::from_ast_and_apply(enum_type, context)
            }
            AstStatementNode::Sequence(sequence) => {
                CreateSequence::from_ast_and_apply(sequence, context)
            }
            AstStatementNode::DropTable(drop_table) => {
                DropTable::from_ast_and_apply(drop_table, context)
            }
            AstStatementNode::AlterSequence(alter_sequence) => {
                AlterSequence::from_ast_and_apply(alter_sequence, context)
            }
            AstStatementNode::Comment(comment) => Comment::from_ast_and_apply(comment, context),
            AstStatementNode::AlterOwner(alteration) => {
                AlterOwner::from_ast_and_apply(alteration, context)
            }
            AstStatementNode::CompositeType(composite_type) => {
                CreateCompositeType::from_ast_and_apply(composite_type, context)
            }
            AstStatementNode::AlterRelation(statement) => {
                RelationAlteration::from_ast_and_apply(statement, context)
            }
            AstStatementNode::Domain(domain) => CreateDomain::from_ast_and_apply(domain, context),
            AstStatementNode::Function(function) => {
                CreateFunction::from_ast_and_apply(function, context)
            }
            AstStatementNode::Schema(schema) => {
                CreateSchema::from_ast_and_apply(schema, context, cache)
            }
            AstStatementNode::Index(index) => CreateIndex::from_ast_and_apply(index, context),
            AstStatementNode::TableAs(table_as) => {
                CreateTableAs::from_ast_and_apply(table_as, context, cache)
            }
            AstStatementNode::Table(table) => CreateTable::from_ast_and_apply(table, context),
            _ => Ok(AstAndContextPair::new(
                Self::from_ast_node(ast, &mut context, cache)?,
                context,
            )),
        }
    }
}

impl SemAst {
    /// Analyze this common statement directly using the supplied context.
    ///
    /// No SQL reparsing or catalog-file discovery occurs. This uses the same
    /// resolver and diagnostic reporting as the normal semantic-analysis path.
    ///
    /// The caller retains and reuses `cache_context` across analyses against
    /// the same immutable schema base. It memoizes calculation facts without
    /// changing analysis results. The caller owns this association; the cache
    /// performs no environment validation.
    ///
    /// # Panics
    ///
    /// CREATE TRIGGER analysis is not implemented yet (#602 / #555).
    /// ALTER DEFAULT PRIVILEGES and CREATE FOREIGN TABLE interpretation remains
    /// unimplemented (#955 / #956), after syntax-version checks succeed.
    ///
    /// # Errors
    ///
    /// Returns a dialect mismatch, resolver, or diagnostic reporting error.
    /// The statement dialect must match the supplied context metadata.
    pub fn from_ast<TContext: ParseContext>(
        statement: &AstStatement,
        context: TContext,
        cache_context: &mut CacheContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        AnalysisError::check_dialect(statement.dialect(), context.dialect())?;
        Self::apply_ast_node(statement.node(), context, cache_context)
    }
}

impl SemAst {
    fn from_insert_resolution(value: Option<&InsertStatement>) -> Self {
        value.map_or_else(Self::unresolved_statement, Self::insert)
    }
    fn from_update_resolution(value: Option<&UpdateStatement>) -> Self {
        value.map_or_else(Self::unresolved_statement, Self::update)
    }
    fn from_create_rule_resolution(value: Option<&CreateRule>) -> Self {
        value.map_or_else(Self::unresolved_statement, Self::create_rule)
    }
    fn from_drop_trigger_resolution(value: Option<&DropTrigger>) -> Self {
        value.map_or_else(Self::unresolved_statement, Self::drop_trigger)
    }
    fn from_create_trigger_resolution(value: Option<&CreateTrigger>) -> Self {
        value.map_or_else(Self::unresolved_statement, Self::create_trigger)
    }
    fn from_drop_rule_resolution(value: Option<&DropRule>) -> Self {
        value.map_or_else(Self::unresolved_statement, Self::drop_rule)
    }
    pub(crate) fn create_composite_type(value: &CreateCompositeType) -> Self {
        Self::CreateCompType(value.clone())
    }
}

impl SemAst {
    pub(crate) fn from_comment(value: &Comment) -> Self {
        Self::Comment(value.clone())
    }
    pub(crate) fn from_drop_table(value: &DropTable) -> Self {
        Self::DropTable(value.clone())
    }
    pub(crate) fn from_create_sequence(value: &CreateSequence) -> Self {
        Self::CreateSequence(value.clone())
    }
    pub(crate) fn from_create_enum(value: &CreateEnum) -> Self {
        Self::CreateEnum(value.clone())
    }
    pub(crate) fn from_variable_set(value: &VariableSet) -> Self {
        Self::VariableSet(value.clone())
    }
    pub(crate) fn from_create_extension(value: &CreateExtension) -> Self {
        Self::CreateExtension(value.clone())
    }
    pub(crate) fn from_create_view(value: &CreateView) -> Self {
        Self::CreateView(value.clone())
    }
}
