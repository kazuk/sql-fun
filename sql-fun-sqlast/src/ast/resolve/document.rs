use crate::ast::XmlTable;
use crate::sem::ParseContext;
use crate::sem::{AnalysisError, DataSource, ScalarAnalysisContext};

impl XmlTable {
    pub(in crate::ast) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSource, AnalysisError> {
        self.rowexpr
            .resolve_table_call(context, scope, self.alias.as_ref())
    }
}
