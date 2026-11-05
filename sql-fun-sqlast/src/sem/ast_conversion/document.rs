use crate::sem::AliasSpec;
use crate::{
    ast::XmlTable,
    context::ParseContext,
    sem::{AnalysisError, DataSource, ScalarAnalysisContext},
};

impl DataSource {
    pub(crate) fn from_ast_xml_table(
        ast: &XmlTable,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        Self::from_ast_table_call(
            &ast.rowexpr,
            context,
            scope,
            ast.alias.as_ref().map(AliasSpec::from_ast).as_ref(),
        )
    }
}
