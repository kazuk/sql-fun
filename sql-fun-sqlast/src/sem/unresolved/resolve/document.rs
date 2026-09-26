use crate::sem::resolution::Resolution;
use crate::sem::unresolved::{Dialect, XmlTable};
use crate::sem::{AnalysisError, DataSource, ScalarAnalysisContext};

impl XmlTable {
    pub(in crate::sem::unresolved) fn resolve(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSource, AnalysisError> {
        self.rowexpr
            .resolve_table_call(context, dialect, scope, self.alias.as_ref())
    }
}
