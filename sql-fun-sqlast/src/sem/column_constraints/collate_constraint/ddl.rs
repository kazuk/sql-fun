//! Analysis owned by the semantic definitions in this module.

use crate::{
    context::ParseContext,
    names::{ColumnName, TableName},
    sem::{AnalysisError, AnalysisProblem, CollateColumnConstraint},
};

impl CollateColumnConstraint {
    pub(in crate::sem::column_constraints) fn from_ast_name(
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        name: &str,
    ) -> Result<Self, AnalysisError> {
        if !Self::is_known_name(name) {
            context.report_analysis_problem(AnalysisProblem::collation_name_not_found(name))?;
        }
        Ok(Self::new(table, column, name))
    }
}
