use crate::{
    sem::{AlterSequence, AstAndContextPair},
    syn::Opt,
};

use {
    super::{AnalysisError, ParseContext, SemAst, alter_table::AlterObjSubCommand},
    crate::names::FullName,
};

/// analyzed [`crate::syn::CreateSeqStmt`]
#[derive(Debug, Clone)]
pub struct CreateSequence {
    temporary: bool,
    if_not_exists: bool,

    name: FullName,
    alter_statements: Vec<AlterSequence>,
    owner: Option<crate::sem::OwnerSpecification>,
    owned_by: Option<crate::names::TableName>,
}

impl CreateSequence {
    pub(crate) const fn temporary(&self) -> bool {
        self.temporary
    }
    pub(crate) const fn with_temporary(mut self, value: bool) -> Self {
        self.temporary = value;
        self
    }

    pub(crate) const fn if_not_exists(&self) -> bool {
        self.if_not_exists
    }
    pub(crate) const fn with_if_not_exists(mut self, value: bool) -> Self {
        self.if_not_exists = value;
        self
    }

    pub(crate) fn with_catalog_name(&self, name: &FullName) -> Self {
        let mut definition = self.clone();
        definition.name = name.clone();
        definition
    }

    /// Dependency on an explicitly specified owning relation (not the role owner).
    pub const fn owned_by(&self) -> Option<&crate::names::TableName> {
        self.owned_by.as_ref()
    }
    /// Retain a known OWNED BY relation dependency.
    pub fn set_owned_by(&mut self, table: &crate::names::TableName) {
        self.owned_by = Some(table.clone());
    }

    /// Retained ownership; None means no ownership information was supplied.
    pub const fn owner(&self) -> Option<&crate::sem::OwnerSpecification> {
        self.owner.as_ref()
    }
    /// Commit an ownership change, leaving the rest of the definition intact.
    pub(crate) fn change_owner(&mut self, owner: &crate::sem::OwnerSpecification) {
        self.owner = Some(owner.clone());
    }

    /// create instance
    #[must_use]
    pub fn new(name: &FullName) -> Self {
        Self {
            temporary: false,
            if_not_exists: false,
            name: name.clone(),
            alter_statements: Default::default(),
            owner: Default::default(),
            owned_by: None,
        }
    }

    /// get sequence name
    #[must_use]
    pub const fn name(&self) -> &FullName {
        &self.name
    }

    /// apply `alter sequence`
    pub fn apply_alter(&mut self, alter_seq: &AlterSequence) -> Result<(), AnalysisError> {
        let mut updated = self.clone();
        updated.apply_alter_in(alter_seq)?;
        *self = updated;
        Ok(())
    }
    fn apply_alter_in(&mut self, alter_seq: &AlterSequence) -> Result<(), AnalysisError> {
        self.alter_statements.push(alter_seq.clone());
        if let Some(dependency) = alter_seq.owned_by() {
            self.owned_by = dependency.relation().cloned();
        }

        for cmd in alter_seq.commands() {
            match cmd {
                AlterObjSubCommand::ChangeOwner(s) => {
                    if s.can_apply() {
                        self.change_owner(s);
                    }
                }
                AlterObjSubCommand::ColumnAttributes(_)
                | AlterObjSubCommand::AddConstraint(_)
                | AlterObjSubCommand::ClusterOn(_)
                | AlterObjSubCommand::RenameTo(_)
                | AlterObjSubCommand::AddColumn(_)
                | AlterObjSubCommand::AlterColumn(_, _)
                | AlterObjSubCommand::RenameColumn(_)
                | AlterObjSubCommand::DropColumn(_)
                | AlterObjSubCommand::DropColumnDefault(_)
                | AlterObjSubCommand::SetColumnDefault(_, _) => {
                    AnalysisError::raise_unexpected_input("unexpected alter sequence alter cmd")?;
                    AnalysisError::never()
                }
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

        let create_sequence = Self::new(&FullName::try_from(name)?);
        let context = context.apply_create_sequence(&create_sequence)?;
        Ok(AstAndContextPair::new(
            SemAst::CreateSequence(create_sequence),
            context,
        ))
    }
}
