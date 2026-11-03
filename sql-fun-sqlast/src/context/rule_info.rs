use crate::context::BaseParseContext;
use crate::{
    context::TableInfoWrite,
    names::{FullName, TableName, ViewName},
    sem::{AnalysisError, CreateRule, RuleIdentity, ViewLike},
};

/// Read named rules through context layers without falling through tombstones.
pub trait RuleInfoRead {
    /// Read the current layer: missing entry, cancellation, or analyzed definition.
    fn local_rule(&self, _identity: &RuleIdentity) -> Option<Option<&CreateRule>> {
        None
    }
    /// Backing parent layer, if one exists.
    fn rule_parent(&self) -> Option<&dyn RuleInfoRead> {
        None
    }
    /// Look up an exact identity, stopping at a cancellation in any layer.
    fn rule(&self, identity: &RuleIdentity) -> Option<&CreateRule> {
        if let Some(entry) = self.local_rule(identity) {
            entry
        } else {
            self.rule_parent()?.rule(identity)
        }
    }
}

/// Explicit definition and target-link operations selected by rule DDL.
pub trait RuleInfoWrite: BaseParseContext + TableInfoWrite + Sized {
    /// Store an analyzed definition or cancellation in this layer.
    fn store_rule(&mut self, identity: &RuleIdentity, definition: Option<&CreateRule>);
    /// Store a cloned view whose rule links were explicitly updated.
    fn store_rule_view(&mut self, target: &FullName, view: &ViewLike);

    /// Register the supplied analyzed rule, replacing a cancellation.
    fn register_rule(mut self, rule: &CreateRule) -> Result<Self, AnalysisError> {
        self.store_rule(rule.identity(), Some(rule));
        Ok(self)
    }
    /// Mask an identity. DDL must separately request target-link removal.
    fn cancel_rule(mut self, identity: &RuleIdentity) -> Result<Self, AnalysisError> {
        self.store_rule(identity, None);
        Ok(self)
    }
    /// Attach an explicitly supplied identity to a cloned target in this layer.
    fn associate_rule(
        self,
        target: &FullName,
        identity: &RuleIdentity,
    ) -> Result<Self, AnalysisError>
    where
        Self: crate::context::ParseContext,
    {
        self.update_rule_link(target, identity, true)
    }
    /// Remove an explicitly supplied target link, without inferring a DROP.
    fn dissociate_rule(
        self,
        target: &FullName,
        identity: &RuleIdentity,
    ) -> Result<Self, AnalysisError>
    where
        Self: crate::context::ParseContext,
    {
        self.update_rule_link(target, identity, false)
    }
    /// Update a target link using the caller's explicit association decision.
    fn update_rule_link(
        mut self,
        target: &FullName,
        identity: &RuleIdentity,
        associate: bool,
    ) -> Result<Self, AnalysisError>
    where
        Self: crate::context::ParseContext,
    {
        if let Some(mut table) = self.get_table(&TableName::from(target.clone())).cloned() {
            if associate {
                table.associate_rule(identity);
            } else {
                table.dissociate_rule(identity);
            }
            self.insert_create_table(target, &table);
        } else if let Some(mut view) = self.get_view(&ViewName::from_full_name(target)).cloned() {
            if associate {
                view.associate_rule(identity);
            } else {
                view.dissociate_rule(identity);
            }
            self.store_rule_view(target, &view);
        } else {
            self.report_analysis_problem(crate::sem::AnalysisProblem::rule(
                &crate::sem::problem::RuleProblem::target(target),
            ))?;
        }
        Ok(self)
    }
}
