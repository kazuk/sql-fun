use crate::sem::{AstConversion, convert};
use crate::{
    ast::XmlTable,
    context::ParseContext,
    sem::{AnalysisError, DataSource, ScalarAnalysisContext},
};

impl AstConversion<'_, XmlTable> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<DataSource, AnalysisError> {
        convert(&self.ast.rowexpr).resolve_table_call(
            context,
            scope,
            self.ast
                .alias
                .as_ref()
                .map(|alias| convert(alias).resolve())
                .as_ref(),
        )
    }
}
