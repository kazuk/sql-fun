//! Analysis owned by the semantic definitions in this module.

use crate::{
    AstAndContextPair,
    ast::{
        Expr as AstExpr, Literal as AstLiteral, Name as AstName, Sequence as AstSequence,
        SyntaxOption as AstSyntaxOption, SyntaxValue as AstSyntaxValue,
    },
    context::ParseContext,
    names::{FullName, TableName},
    sem::{AnalysisError, AnalysisProblem, CreateSequence, SemAst},
};

impl CreateSequence {
    pub(crate) fn owned_table_from_ast_option(
        ast: &AstSyntaxOption,
    ) -> Result<Option<TableName>, AnalysisError> {
        if ast.name() != "owned_by" {
            return Ok(None);
        }
        let Some(AstSyntaxValue::Expression(expression)) = &ast.value() else {
            return AnalysisError::raise_unexpected_input("sequence OWNED BY target");
        };
        Self::owned_table_from_ast_expression(expression)
    }

    pub(crate) fn owned_table_from_ast_expression(
        ast: &AstExpr,
    ) -> Result<Option<TableName>, AnalysisError> {
        match ast {
            AstExpr::List(parts) => Self::owned_table_from_ast_parts(parts),
            AstExpr::Column(name) => Self::owned_table_from_ast_name(name),
            AstExpr::Literal(literal) => Self::owned_table_from_ast_literal(literal),
            _ => AnalysisError::raise_unexpected_input("sequence OWNED BY identity"),
        }
    }

    pub(crate) fn owned_table_from_ast_parts(
        parts: &[AstExpr],
    ) -> Result<Option<TableName>, AnalysisError> {
        let mut names = Vec::new();
        for part in parts {
            names.push(Self::owned_identifier_from_ast_expression(part)?);
        }
        Self::owned_table_from_ast_name(&AstName::from_parts(&names))
    }

    pub(crate) fn owned_identifier_from_ast_expression(
        ast: &AstExpr,
    ) -> Result<String, AnalysisError> {
        match ast {
            AstExpr::Literal(literal) => Self::owned_identifier_from_ast_literal(literal),
            _ => AnalysisError::raise_unexpected_input("sequence OWNED BY identifier"),
        }
    }

    pub(crate) fn owned_table_from_ast_name(
        ast: &AstName,
    ) -> Result<Option<TableName>, AnalysisError> {
        if ast.parts().len() == 1 && ast.parts()[0].eq_ignore_ascii_case("none") {
            return Ok(None);
        }
        let Some((_, table)) = ast.parts().split_last() else {
            return AnalysisError::raise_unexpected_input("sequence OWNED BY column");
        };
        Ok(Some(TableName::from(FullName::from_ast_name(
            &AstName::from_parts(table),
        )?)))
    }

    pub(crate) fn owned_table_from_ast_literal(
        ast: &AstLiteral,
    ) -> Result<Option<TableName>, AnalysisError> {
        if let AstLiteral::Keyword(keyword) = ast
            && keyword.eq_ignore_ascii_case("none")
        {
            Ok(None)
        } else {
            AnalysisError::raise_unexpected_input("sequence OWNED BY identity")
        }
    }

    pub(crate) fn owned_identifier_from_ast_literal(
        ast: &AstLiteral,
    ) -> Result<String, AnalysisError> {
        match ast {
            AstLiteral::String(name) => Ok(name.clone()),
            _ => AnalysisError::raise_unexpected_input("sequence OWNED BY identifier"),
        }
    }
}

impl CreateSequence {
    pub(crate) fn from_ast_and_apply<TContext>(
        ast: &AstSequence,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let mut sequence = Self::new(&FullName::from_ast_name(ast.name())?)
            .with_if_not_exists(ast.if_not_exists())
            .with_temporary(ast.temporary());
        for option in ast.options() {
            if let Some(table) = Self::owned_table_from_ast_option(option)? {
                let Some(resolved) = context.resolve_table_name(&table) else {
                    context.report_analysis_problem(AnalysisProblem::table_not_found(&table))?;
                    continue;
                };
                let table = resolved;
                sequence.set_owned_by(&table);
            }
        }
        let context = context.apply_create_sequence(&sequence)?;
        Ok(AstAndContextPair::new(
            SemAst::from_create_sequence(&sequence),
            context,
        ))
    }
}
