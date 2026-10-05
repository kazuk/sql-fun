use crate::{
    ast::TryFromWithTokens,
    sem::{AstAndContextPair, FullName, alter_table::AlterObjSubCommand},
    syn::Opt,
};

use super::{AnalysisError, ParseContext, SemAst};

/// analyzed [`crate::syn::AlterSeqStmt`]
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct AlterSequence {
    seq_name: FullName,
    commands: Vec<AlterObjSubCommand>,
}

impl AlterSequence {
    /// create `alter sequence` statement
    #[must_use]
    pub fn new(seq_name: &FullName, commands: &[AlterObjSubCommand]) -> Self {
        Self {
            seq_name: seq_name.clone(),
            commands: commands.to_vec(),
        }
    }

    /// analyze [`crate::syn::AlterSeqStmt`]
    pub fn analyze_alter_seq<TParseContext>(
        mut context: TParseContext,
        _parent_schema: &Option<String>,
        syn: crate::syn::AlterSeqStmt,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(seq) = syn.get_sequence().as_inner() else {
            AnalysisError::raise_unexpected_none("AlterSeqStmt.sequence")?
        };
        let seq = FullName::try_from(seq)?;

        let alter_seq = Self {
            seq_name: seq,
            commands: Default::default(),
        };
        context = context.apply_alter_sequence(&alter_seq)?;
        Ok(AstAndContextPair::new(
            SemAst::AlterSequence(alter_seq),
            context,
        ))
    }

    /// analyze [`crate::syn::AlterTableStmt`]
    pub fn analyze_alter_seq_statement<TParseContext>(
        context: TParseContext,
        _parent_schema: &Option<String>,
        syn: crate::syn::AlterTableStmt,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        crate::ast::AlterRelation::try_from_with_tokens(&syn, &sql_fun_core::IVec::default())?
            .resolve_and_apply(context, crate::ast::Dialect::postgres())
    }
}
