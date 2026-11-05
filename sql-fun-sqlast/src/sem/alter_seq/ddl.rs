//! Analysis owned by the semantic definitions in this module.

use crate::{
    AstAndContextPair,
    ast::AlterSequence as AstAlterSequence,
    context::ParseContext,
    names::FullName,
    sem::{AlterSequence, AnalysisError, AnalysisProblem, CreateSequence, SemAst},
};

impl AlterSequence {
    pub(crate) fn from_ast_and_apply<TContext>(
        ast: &AstAlterSequence,
        context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let mut context = context;
        let mut alter_sequence = Self::new(&FullName::from_ast_name(&ast.name)?, &[]);
        for option in &ast.options {
            if option.name == "owned_by" {
                let relation =
                    if let Some(requested) = CreateSequence::owned_table_from_ast_option(option)? {
                        if let Some(relation) = context.resolve_table_name(&requested) {
                            Some(relation)
                        } else {
                            context.report_analysis_problem(AnalysisProblem::table_not_found(
                                &requested,
                            ))?;
                            continue;
                        }
                    } else {
                        None
                    };
                alter_sequence = alter_sequence.with_owned_by(relation.as_ref());
            }
        }
        let context = context.apply_alter_sequence(&alter_sequence)?;
        Ok(AstAndContextPair::new(
            SemAst::alter_sequence(&alter_sequence),
            context,
        ))
    }
}
