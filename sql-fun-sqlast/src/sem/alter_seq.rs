use crate::{
    ast::AlterSequence as AstAlterSequence,
    names::{FullName, TableName},
    sem::{
        AnalysisError, AnalysisProblem, AstAndContextPair, CreateSequence, ParseContext, SemAst,
        alter_table::AlterObjSubCommand,
    },
};

/// Analyzed ALTER SEQUENCE statement.
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct AlterSequence {
    seq_name: FullName,
    commands: Vec<AlterObjSubCommand>,
    /// Explicit dependency change; absent means preserve the current dependency.
    owned_by: Option<SequenceDependency>,
}

impl AlterSequence {
    pub(crate) fn with_owned_by(mut self, relation: Option<&TableName>) -> Self {
        self.owned_by = Some(SequenceDependency::from(relation));
        self
    }

    /// create `alter sequence` statement
    #[must_use]
    pub fn new(seq_name: &FullName, commands: &[AlterObjSubCommand]) -> Self {
        Self {
            seq_name: seq_name.clone(),
            commands: commands.to_vec(),
            owned_by: None,
        }
    }
}

/// An explicit sequence dependency, distinguishing OWNED BY NONE from an omitted edit.
#[derive(Clone, Debug)]
pub struct SequenceDependency(Option<TableName>);
impl From<Option<&TableName>> for SequenceDependency {
    fn from(relation: Option<&TableName>) -> Self {
        Self(relation.cloned())
    }
}
impl SequenceDependency {
    /// Bound owning relation; None is the explicit OWNED BY NONE choice.
    pub const fn relation(&self) -> Option<&TableName> {
        self.0.as_ref()
    }
}

// Analysis owned by the semantic definitions in this module.

impl AlterSequence {
    pub(crate) fn from_ast_and_apply<TContext>(
        ast: &AstAlterSequence,
        context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let mut context = context;
        let mut alter_sequence = Self::new(&FullName::from_ast_name(ast.name())?, &[]);
        for option in ast.options() {
            if option.name() == "owned_by" {
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
