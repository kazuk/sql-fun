mod ddl;
use crate::{
    ast::Dialect,
    names::TableName,
    sem::{
        AnalysisError, AnalysisProblem, ColumnRename, MinMaxExpr, ParseContext, PgBuiltInType,
        SemScalarExpr, TruthTestExpr,
        problem::CheckProblem,
        traversal::{ColumnRenamer, SemanticVisitorMut},
    },
};

/// Analyzed table CHECK, including failed definitions and their applicability.
#[derive(Debug, Clone)]
pub struct CheckConstraint {
    name: String,
    table: Option<TableName>,
    expression: Box<SemScalarExpr>,
    no_inherit: bool,
    not_valid: bool,
    validated: bool,
    enforced: Option<bool>,
    applicable: bool,
}
impl CheckConstraint {
    pub(crate) fn analyze(
        context: &mut impl ParseContext,
        table: &TableName,
        name: &str,
        expression: &SemScalarExpr,
    ) -> Result<Self, AnalysisError> {
        let mut result = Self {
            name: name.to_owned(),
            table: Some(table.clone()),
            expression: Box::new(expression.clone()),
            no_inherit: false,
            not_valid: false,
            validated: true,
            enforced: None,
            applicable: true,
        };
        let mut validator = CheckExpression {
            context,
            valid: true,
            placement: false,
            parameter: false,
        };
        result.expression.walk(&mut validator)?;
        validator.boolean_input(&mut result.expression)?;
        result.applicable = validator.valid;
        let placement = validator.placement;
        let parameter = validator.parameter;
        if placement {
            result.reject(context, CheckProblem::ExpressionPlacement)?;
        }
        if parameter {
            result.reject(context, CheckProblem::Parameter)?;
        }
        Ok(result)
    }

    pub(crate) const fn retain_options(
        &mut self,
        no_inherit: bool,
        not_valid: bool,
        enforced: Option<bool>,
    ) {
        self.no_inherit = no_inherit;
        self.not_valid = not_valid;
        self.enforced = enforced;
    }
    pub(crate) fn reject(
        &mut self,
        context: &mut impl ParseContext,
        problem: CheckProblem,
    ) -> Result<(), AnalysisError> {
        self.applicable = false;
        context.report_analysis_problem(AnalysisProblem::Check(problem))
    }
    pub(crate) fn reject_duplicate_name(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            todo!("SQLite repeated constraint names (#992)");
        }
        self.reject(context, CheckProblem::DuplicateName(self.name.clone()))
    }
    pub(crate) const fn disallow(&mut self) {
        self.applicable = false;
    }
    pub(crate) const fn set_validated(&mut self, validated: bool) {
        self.validated = validated;
    }
    /// Catalog validation state; CREATE is validated even when NOT VALID was written.
    /// This metadata does not establish anything about live database rows.
    pub const fn validated(&self) -> bool {
        self.validated
    }
    /// Explicit SQL name, or the empty string for an unnamed declaration.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Completed target binding; None records a missing target.
    pub const fn table(&self) -> Option<&TableName> {
        self.table.as_ref()
    }
    pub(crate) fn missing_target(&mut self) {
        self.table = None;
        self.applicable = false;
    }
    /// Analyzed CHECK expression, preserving resolved and failed references.
    pub const fn expression(&self) -> &SemScalarExpr {
        &self.expression
    }
    /// Whether NO INHERIT was declared.
    pub const fn no_inherit(&self) -> bool {
        self.no_inherit
    }
    /// Whether NOT VALID was explicitly written.
    pub const fn not_valid(&self) -> bool {
        self.not_valid
    }
    /// Explicit enforcement choice; omission remains absent.
    pub const fn enforced(&self) -> Option<bool> {
        self.enforced
    }
    /// Whether this definition permits a catalog transition.
    pub const fn can_apply(&self) -> bool {
        self.applicable
    }
    pub(crate) fn rename_column(&mut self, rename: &ColumnRename) -> Result<(), AnalysisError> {
        self.expression.walk(&mut ColumnRenamer::new(rename))
    }
    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        if let Some(table) = &mut self.table {
            table.rename_reference(old, new);
        }
    }
    pub(crate) fn walk(
        &mut self,
        visitor: &mut impl SemanticVisitorMut,
    ) -> Result<(), AnalysisError> {
        self.expression.walk(visitor)
    }
}

struct CheckExpression<'a, C> {
    context: &'a mut C,
    valid: bool,
    placement: bool,
    parameter: bool,
}
impl<C: ParseContext> SemanticVisitorMut for CheckExpression<'_, C> {
    fn boolean_input(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        if self.context.dialect() != Dialect::postgres() {
            return Ok(());
        }
        let problem = if let Some(ty) = MinMaxExpr::input_type(expression) {
            if ty == MinMaxExpr::unknown() {
                if let Some(literal) = expression.as_const()
                    && let Some(text) = literal.as_string()
                    && !TruthTestExpr::valid_boolean_input(text)
                {
                    Some(CheckProblem::BooleanLiteral(text.to_owned()))
                } else {
                    None
                }
            } else if !TruthTestExpr::accepts_type(&*self.context, &ty) {
                Some(CheckProblem::BooleanType)
            } else {
                None
            }
        } else {
            self.valid = false;
            return Ok(());
        };
        if let Some(problem) = problem {
            self.valid = false;
            self.context
                .report_analysis_problem(AnalysisProblem::Check(problem))?;
        } else if expression.get_type() != Some(PgBuiltInType::bool()) {
            *expression = SemScalarExpr::wrap_implicit_cast(&PgBuiltInType::bool(), expression);
        }
        Ok(())
    }
    fn walk_source_column_values(&self) -> bool {
        false
    }
    fn enter_query(&mut self) {
        self.placement = true;
    }
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        self.valid &= expression.check_references_resolved();
        if !expression.preserves_view_rows(&*self.context) {
            self.placement = true;
        }
        if matches!(expression, SemScalarExpr::Param(_)) {
            self.parameter = true;
        }
        Ok(())
    }
}
