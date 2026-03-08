use crate::{
    sem::{AlterSequence, AstAndContextPair},
    syn::Opt,
};

use super::{AnalysisError, FullName, ParseContext, SemAst, alter_table::AlterObjSubCommand};

/// analyzed [`crate::syn::CreateSeqStmt`]
#[derive(Debug, Clone)]
pub struct CreateSequence {
    name: FullName,
    alter_statements: Vec<AlterSequence>,
    owner: Option<String>,
}

impl CreateSequence {
    /// create instance
    #[must_use]
    pub fn new(name: &FullName) -> Self {
        Self {
            name: name.clone(),
            alter_statements: Default::default(),
            owner: Default::default(),
        }
    }

    /// get sequence name
    #[must_use]
    pub fn name(&self) -> &FullName {
        &self.name
    }

    /// apply `alter sequence`
    pub fn apply_alter(&mut self, alter_seq: &AlterSequence) -> Result<(), AnalysisError> {
        self.alter_statements.push(alter_seq.clone());

        for cmd in alter_seq.commands() {
            match cmd {
                AlterObjSubCommand::ChangeOwner(s) => self.owner = Some(s.clone()),
                AlterObjSubCommand::SetColumnDefault(_column_name, _sem_scalar_expr) => todo!(),
                AlterObjSubCommand::AddConstraint(_constraint) => todo!(),
                AlterObjSubCommand::ClusterOn(_) => todo!(),
            }
        }
        Ok(())
    }

    /// analyze [`crate::syn::CreateSeqStmt`]
    pub fn analyze_create_seq<TParseContext>(
        context: TParseContext,
        _parent_schema: &Option<String>,
        syn: crate::syn::CreateSeqStmt,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(name) = syn.get_sequence().as_inner() else {
            AnalysisError::raise_unexpected_none("create_seq.sequence")?
        };

        let create_sequence = CreateSequence::new(&FullName::try_from(name)?);
        let context = context.apply_create_sequence(&create_sequence)?;
        Ok(AstAndContextPair::new(
            SemAst::CreateSequence(create_sequence),
            context,
        ))
    }
}

/// analyze [`crate::syn::CreateSeqStmt`]
#[deprecated = "use CreateSequence::analyze_create_seq"]
pub fn analyze_create_seq<TParseContext>(
    context: TParseContext,
    parent_schema: &Option<String>,
    syn: crate::syn::CreateSeqStmt,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    CreateSequence::analyze_create_seq(context, parent_schema, syn)
}
