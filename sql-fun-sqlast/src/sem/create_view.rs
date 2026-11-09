mod ddl;
use crate::{
    names::{ColumnName, FullName, TableName},
    sem::{
        AlterView, AnalysisError, ColumnRename, OwnerSpecification, ParseContext,
        ResultColumnCollection, RuleIdentity, RuleReferences, SelectStatement, SemScalarExpr,
        TriggerReferences, ViewCheckOption, ViewColumnLineage, ViewOption,
        create_table::ColumnDefinition,
        problem::ViewProblem,
        traversal::{ColumnRenamer, RelationRenamer, SemanticVisitorMut},
    },
};

/// Analyzed CREATE VIEW statement.
#[derive(Debug, Clone)]
pub struct CreateView {
    temporary: bool,
    if_not_exists: bool,
    replace: bool,

    name: FullName,
    query: SelectStatement,
    lineage: ViewColumnLineage,
    options: Vec<ViewOption>,
    check_option: ViewCheckOption,
    defaults: Vec<(ColumnName, SemScalarExpr)>,
    triggers: TriggerReferences,
    rules: RuleReferences,
    alter_statements: Vec<AlterView>,
    owner: Option<OwnerSpecification>,
    materialized: bool,
}

impl CreateView {
    pub(crate) fn rename_column(&mut self, rename: &ColumnRename) -> Result<(), AnalysisError> {
        self.rename_column_with(rename, &mut ColumnRenamer::new(rename))
    }
    pub(crate) fn rename_column_with(
        &mut self,
        rename: &ColumnRename,
        visitor: &mut impl SemanticVisitorMut,
    ) -> Result<(), AnalysisError> {
        self.lineage.rename_column(rename, &self.query)?;
        self.query.walk(visitor)?;
        for (_, value) in &mut self.defaults {
            value.walk(visitor)?;
        }
        Ok(())
    }

    pub(crate) fn rename_relation(
        &mut self,
        old: &TableName,
        new: &TableName,
    ) -> Result<(), AnalysisError> {
        let mut visitor = RelationRenamer::new(old, new);
        self.query.walk(&mut visitor)?;
        for (_, value) in &mut self.defaults {
            value.walk(&mut visitor)?;
        }
        Ok(())
    }

    /// View options from its defining declaration.
    pub fn options(&self) -> &[ViewOption] {
        &self.options
    }
    /// Declared CHECK OPTION.
    pub const fn check_option(&self) -> ViewCheckOption {
        self.check_option
    }
    pub(crate) fn with_options(mut self, options: &[ViewOption], check: ViewCheckOption) -> Self {
        self.options = options.to_vec();
        self.check_option = check;
        self
    }
    pub(crate) fn walk_column_dependencies(
        &mut self,
        visitor: &mut impl SemanticVisitorMut,
    ) -> Result<(), AnalysisError> {
        self.query.walk(visitor)
    }
    /// The resolved defining query.
    pub const fn query(&self) -> &SelectStatement {
        &self.query
    }
    /// Proven output-column lineage, independent of writability policy.
    pub const fn column_lineage(&self) -> &ViewColumnLineage {
        &self.lineage
    }
    pub(crate) fn with_column_lineage(mut self, lineage: &ViewColumnLineage) -> Self {
        self.lineage = lineage.clone();
        self
    }
    pub(crate) fn apply_column_default(&self, column: &mut ColumnDefinition) {
        if let Some((_, value)) = self
            .defaults
            .iter()
            .find(|(name, _)| column.name().as_ref() == Some(name))
        {
            column.replace_view_default(&self.name, value);
        }
    }
    pub(crate) fn change_column_default(
        &mut self,
        name: &ColumnName,
        value: Option<&SemScalarExpr>,
    ) -> Result<(), ViewProblem> {
        if !self.query.has_column(name) {
            return Err(ViewProblem::unknown_default_column(
                &self.name,
                name.as_str(),
            ));
        }
        self.defaults.retain(|(existing, _)| existing != name);
        if let Some(value) = value {
            self.defaults.push((name.clone(), value.clone()));
        }
        Ok(())
    }
    pub(crate) const fn temporary(&self) -> bool {
        self.temporary
    }
    pub(crate) const fn with_temporary(mut self, value: bool) -> Self {
        self.temporary = value;
        self
    }

    pub(crate) const fn if_not_exists(&self) -> bool {
        self.if_not_exists
    }
    pub(crate) const fn with_if_not_exists(mut self, value: bool) -> Self {
        self.if_not_exists = value;
        self
    }
    pub(crate) const fn replace(&self) -> bool {
        self.replace
    }
    pub(crate) const fn with_replace(mut self, value: bool) -> Self {
        self.replace = value;
        self
    }
    pub(crate) fn replace_rule_query(&mut self, query: &SelectStatement) {
        self.query = query.clone();
        self.lineage = ViewColumnLineage::from_query(query);
    }
    pub(crate) fn retain_relation_state(&mut self, previous: &Self) {
        self.triggers = previous.triggers.clone();
        self.rules = previous.rules.clone();
        self.defaults = previous.defaults.clone();
        self.owner = previous.owner.clone();
        self.alter_statements = previous.alter_statements.clone();
    }
    pub(crate) fn retain_rule_references(&mut self, references: &RuleReferences) {
        self.rules = references.clone();
    }
    /// Rule links owned by this relation; resolve against the effective context.
    pub const fn rules(&self) -> &RuleReferences {
        &self.rules
    }
    pub(crate) fn associate_rule(&mut self, identity: &RuleIdentity) {
        self.rules.associate(identity);
    }
    pub(crate) fn dissociate_rule(&mut self, identity: &RuleIdentity) {
        self.rules.dissociate(identity);
    }

    /// Retained ownership; None means no ownership information was supplied.
    pub const fn owner(&self) -> Option<&OwnerSpecification> {
        self.owner.as_ref()
    }
    /// Commit an ownership change, leaving the rest of the definition intact.
    pub(crate) fn change_owner(&mut self, owner: &OwnerSpecification) {
        self.owner = Some(owner.clone());
    }

    /// Fully qualified trigger references associated with this relation.
    pub const fn triggers(&self) -> &TriggerReferences {
        &self.triggers
    }
    pub(crate) fn associate_trigger(&mut self, name: &FullName) {
        self.triggers.associate(name);
    }
    pub(crate) fn dissociate_trigger(&mut self, name: &FullName) {
        self.triggers.dissociate(name);
    }

    pub(crate) fn result_columns(&self) -> &ResultColumnCollection {
        self.query.result_columns()
    }

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
            temporary: false,
            if_not_exists: false,
            replace: false,
            name: name.clone(),
            triggers: TriggerReferences::default(),
            rules: RuleReferences::default(),
            query: query.clone(),
            lineage: ViewColumnLineage::from_query(query),
            defaults: Vec::new(),
            options: Vec::new(),
            check_option: ViewCheckOption::none(),
            alter_statements: alter_statements.to_owned(),
            owner: owner.as_deref().map(OwnerSpecification::named),
            materialized,
        }
    }

    /// name of view
    #[must_use]
    pub const fn name(&self) -> &FullName {
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

    /// Apply `ALTER VIEW`, returning false after reporting an invalid column.
    /// Rejected changes leave the view unchanged.
    pub fn apply_alter(
        &mut self,
        context: &mut impl ParseContext,
        alter_view: &AlterView,
    ) -> Result<bool, AnalysisError> {
        for command in alter_view.commands() {
            if !command.validate_for_view(context, self)? {
                return Ok(false);
            }
        }
        self.alter_statements.push(alter_view.clone());

        for command in alter_view.commands() {
            command.apply_to_view(self);
        }

        Ok(true)
    }

    /// view is materialized
    #[must_use]
    pub const fn is_materialized(&self) -> bool {
        self.materialized
    }
}
