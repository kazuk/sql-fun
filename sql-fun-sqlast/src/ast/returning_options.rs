use crate::{
    ast::ReturningOption,
    sem::AnalysisError,
    syn::{self, ListOpt, Opt},
};

impl ReturningOption {
    pub(super) fn from_postgres(
        clause: &syn::ReturningClauseOpt,
    ) -> Result<Vec<Self>, AnalysisError> {
        clause
            .get_options()
            .as_inner()
            .unwrap_or_default()
            .iter()
            .map(|node| {
                let option = super::postgres::required(
                    node.as_returning_option().as_inner(),
                    "RETURNING option",
                )?;
                let old = match option.get_option().as_inner() {
                    Some(syn::ReturningOptionKind::ReturningOptionOld) => true,
                    Some(syn::ReturningOptionKind::ReturningOptionNew) => false,
                    _ => return AnalysisError::raise_unexpected_input("RETURNING option kind"),
                };
                Ok(Self {
                    old,
                    alias: option.get_value(),
                })
            })
            .collect()
    }
}
