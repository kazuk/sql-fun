use sql_fun_core::IVec;

use crate::{
    sem::{
        AlterView, AstAndContextPair,
        alter_table::AlterObjSubCommand,
        create_table::{ColumnDefinition, ColumnName},
    },
    syn::{Opt, ScanToken},
};

use super::{AnalysisError, FullName, ParseContext, SelectStatement, SemAst};

/// analyzed [`crate::syn::ViewStmt`]
#[derive(Debug, Clone)]
pub struct CreateView {
    temporary: bool,
    if_not_exists: bool,
    replace: bool,

    name: FullName,
    query: SelectStatement,
    lineage: crate::sem::ViewColumnLineage,
    options: Vec<crate::sem::ViewOption>,
    check_option: crate::sem::ViewCheckOption,
    defaults: Vec<(ColumnName, crate::sem::SemScalarExpr)>,
    triggers: crate::sem::TriggerReferences,
    rules: crate::sem::RuleReferences,
    alter_statements: Vec<AlterView>,
    owner: Option<crate::sem::OwnerSpecification>,
    materialized: bool,
}

impl CreateView {
    /// View options from its defining declaration.
    pub fn options(&self) -> &[crate::sem::ViewOption] {
        &self.options
    }
    /// Declared CHECK OPTION.
    pub const fn check_option(&self) -> crate::sem::ViewCheckOption {
        self.check_option
    }
    pub(crate) fn with_options(
        mut self,
        options: &[crate::sem::ViewOption],
        check: crate::sem::ViewCheckOption,
    ) -> Self {
        self.options = options.to_vec();
        self.check_option = check;
        self
    }
    /// The resolved defining query.
    pub const fn query(&self) -> &SelectStatement {
        &self.query
    }
    /// Proven output-column lineage, independent of writability policy.
    pub const fn column_lineage(&self) -> &crate::sem::ViewColumnLineage {
        &self.lineage
    }
    pub(crate) fn with_column_lineage(mut self, lineage: &crate::sem::ViewColumnLineage) -> Self {
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
        value: Option<&crate::sem::SemScalarExpr>,
    ) -> Result<(), crate::sem::problem::ViewProblem> {
        if !self.query.has_column(name) {
            return Err(crate::sem::problem::ViewProblem::unknown_default_column(
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
    pub(crate) fn replace_rule_query(&mut self, query: &crate::sem::SelectStatement) {
        self.query = query.clone();
        self.lineage = crate::sem::ViewColumnLineage::from_query(query);
    }
    pub(crate) fn retain_relation_state(&mut self, previous: &Self) {
        self.triggers = previous.triggers.clone();
        self.rules = previous.rules.clone();
        self.defaults = previous.defaults.clone();
        self.owner = previous.owner.clone();
        self.alter_statements = previous.alter_statements.clone();
    }
    pub(crate) fn retain_rule_references(&mut self, references: &crate::sem::RuleReferences) {
        self.rules = references.clone();
    }
    /// Rule links owned by this relation; resolve against the effective context.
    pub const fn rules(&self) -> &crate::sem::RuleReferences {
        &self.rules
    }
    pub(crate) fn associate_rule(&mut self, identity: &crate::sem::RuleIdentity) {
        self.rules.associate(identity);
    }
    pub(crate) fn dissociate_rule(&mut self, identity: &crate::sem::RuleIdentity) {
        self.rules.dissociate(identity);
    }

    /// Retained ownership; None means no ownership information was supplied.
    pub const fn owner(&self) -> Option<&crate::sem::OwnerSpecification> {
        self.owner.as_ref()
    }
    /// Commit an ownership change, leaving the rest of the definition intact.
    pub(crate) fn change_owner(&mut self, owner: &crate::sem::OwnerSpecification) {
        self.owner = Some(owner.clone());
    }

    /// Fully qualified trigger references associated with this relation.
    pub const fn triggers(&self) -> &crate::sem::TriggerReferences {
        &self.triggers
    }
    pub(crate) fn associate_trigger(&mut self, name: &crate::sem::FullName) {
        self.triggers.associate(name);
    }
    pub(crate) fn dissociate_trigger(&mut self, name: &crate::sem::FullName) {
        self.triggers.dissociate(name);
    }

    pub(crate) fn result_columns(&self) -> &crate::sem::ResultColumnCollection {
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
            triggers: crate::sem::TriggerReferences::default(),
            rules: crate::sem::RuleReferences::default(),
            query: query.clone(),
            lineage: crate::sem::ViewColumnLineage::from_query(query),
            defaults: Vec::new(),
            options: Vec::new(),
            check_option: crate::sem::ViewCheckOption::none(),
            alter_statements: alter_statements.to_owned(),
            owner: owner.as_deref().map(crate::sem::OwnerSpecification::named),
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

    /// analyze [`crate::syn::ViewStmt`]
    #[tracing::instrument(skip(context))]
    pub fn analyze_create_view<TParseContext>(
        context: TParseContext,
        parent_schema: &Option<String>,
        syn: crate::syn::ViewStmt,
        tokens: &IVec<ScanToken>,
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

        let AstAndContextPair(select, context) =
            SelectStatement::analyze_select(context, query, tokens)?;
        let SemAst::SelectStatement(stmt) = select else {
            AnalysisError::raise_unexpected_none("select_statement")?
        };
        let name = FullName::try_from(view)?;
        let create_view = Self::new(&name, &stmt, &[], &Default::default(), false);
        let context = context.apply_create_view(&create_view)?;
        Ok(AstAndContextPair::new(
            SemAst::CreateView(create_view),
            context,
        ))
    }
}

impl AlterObjSubCommand {
    fn validate_for_view(
        &self,
        context: &mut impl ParseContext,
        view: &CreateView,
    ) -> Result<bool, AnalysisError> {
        let name = match self {
            Self::SetColumnDefault(name, _) | Self::DropColumnDefault(name) => name,
            _ => return Ok(true),
        };
        if view.has_column(name) {
            return Ok(true);
        }
        context.report_analysis_problem(crate::sem::AnalysisProblem::view(
            &crate::sem::problem::ViewProblem::unknown_default_column(&view.name, name.as_str()),
        ))?;
        Ok(false)
    }
    fn apply_to_view(&self, view: &mut CreateView) {
        match self {
            Self::ChangeOwner(owner) => {
                if owner.can_apply() {
                    view.change_owner(owner);
                }
            }
            Self::SetColumnDefault(name, value) => view
                .change_column_default(name, Some(value))
                .expect("validated default column"),
            Self::DropColumnDefault(name) => view
                .change_column_default(name, None)
                .expect("validated default column"),
            _ => todo!(),
        }
    }
}
