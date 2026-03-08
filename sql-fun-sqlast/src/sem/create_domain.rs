use sql_fun_core::IVec;

use crate::{
    sem::{AstAndContextPair, TypeReference},
    syn::{Opt, ScanToken},
};

use super::{AnalysisError, AnalysisProblem, FullName, ParseContext, SemAst};

/// analyzed [`crate::syn::CreateDomainStmt`]
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateDomain {
    type_ref: TypeReference,
    base_type: Option<FullName>,
}

impl CreateDomain {
    /// get name
    #[must_use]
    pub fn name(&self) -> &FullName {
        self.type_ref.full_name()
    }

    /// get base type reference
    #[must_use]
    pub fn type_ref(&self) -> &TypeReference {
        &self.type_ref
    }

    /// analyze [`crate::syn::CreateDomainStmt`]
    pub fn analyze_create_domain<TParseContext>(
        mut context: TParseContext,
        syn: crate::syn::CreateDomainStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let name = FullName::try_from(syn.get_domainname())?;
        let Some(base) = syn.get_type_name().as_inner() else {
            AnalysisError::raise_unexpected_none("create_domain_stmt.type_name")?
        };

        let base_span = base.get_name_span(tokens);

        let create_domain =
            if let Some(base_type) = context.get_type(&FullName::try_from(base.clone())?) {
                CreateDomain {
                    type_ref: TypeReference::from_full_name(name),
                    base_type: base_type.full_name().cloned(),
                }
            } else {
                context.report_problem(AnalysisProblem::domain_type_name_not_found(
                    &name, &base, &base_span,
                ))?;
                CreateDomain {
                    type_ref: TypeReference::from_full_name(name),
                    base_type: None,
                }
            };
        let result_context = context.apply_create_domain(&create_domain)?;
        Ok(AstAndContextPair::new(
            SemAst::CreateDomain(create_domain),
            result_context,
        ))
    }
}

/// analyze [`crate::syn::CreateDomainStmt`]
#[deprecated = "use CreateDomain::analyze_create_domain"]
pub fn analyze_create_domain<TParseContext>(
    context: TParseContext,
    syn: crate::syn::CreateDomainStmt,
    tokens: &IVec<ScanToken>,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    CreateDomain::analyze_create_domain(context, syn, tokens)
}
