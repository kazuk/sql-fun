use crate::{
    sem::{AnalysisProblem, AstAndContextPair, FullName, alter_table::AlterObjSubCommand},
    syn::{AlterTableType, ListOpt, Opt},
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
            AnalysisError::raise_unexpected_none("AlterSeqStmt.seuqence")?
        };
        let seq = FullName::try_from(seq)?;

        let alter_seq = AlterSequence {
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
        mut context: TParseContext,
        _parent_schema: &Option<String>,
        syn: crate::syn::AlterTableStmt,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(rel) = syn.get_relation().as_inner() else {
            AnalysisError::raise_unexpected_none("alter_table_stmt.relation")?
        };
        let seq_name = FullName::try_from(rel)?;
        if context.get_sequence(&seq_name).is_none() {
            context.report_problem(AnalysisProblem::sequence_not_found(&seq_name))?;
        }
        let Some(cmds) = syn.get_cmds().map(|c| c.as_alter_table_cmd()) else {
            AnalysisError::raise_unexpected_none("alter_table_stmt.cmds")?
        };
        let mut sub_cmds = Vec::new();

        for cmd in cmds {
            let Some(subtyp) = cmd.get_subtype().as_inner() else {
                AnalysisError::raise_unexpected_none("alter_table_cmd.subtype")?
            };
            let sub_cmd = match subtyp {
                AlterTableType::AtChangeOwner => {
                    crate::sem::alter_table::analyze_alter_obj_change_owner(&context, cmd)?
                }

                _ => todo!("{subtyp:?}"),
            };
            sub_cmds.push(sub_cmd);
        }

        let alter_seq = Self::new(&seq_name, &sub_cmds);

        context = context.apply_alter_sequence(&alter_seq)?;
        Ok(AstAndContextPair::new(
            SemAst::AlterSequence(alter_seq),
            context,
        ))
    }
}

/// analyze [`crate::syn::AlterSeqStmt`]
#[deprecated = "use AlterSequence::analyze_alter_seq"]
#[expect(dead_code)]
pub fn analyze_alter_seq<TParseContext>(
    context: TParseContext,
    parent_schema: &Option<String>,
    syn: crate::syn::AlterSeqStmt,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    AlterSequence::analyze_alter_seq(context, parent_schema, syn)
}
