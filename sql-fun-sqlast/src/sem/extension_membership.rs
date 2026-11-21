use crate::{
    ast::{Name, ObjectKind, extension_membership::ExtensionMembership as AstMembership},
    context::ParseContext,
    names::FullName,
    sem::{
        AnalysisError, AnalysisProblem, CatalogNameLookup, Kind, OwnershipIdentity,
        OwnershipLookup, SourcePathWithSpan, ownership::RequestedIdentity,
        search_path::expand_relation_search_path,
    },
};
use ::sql_fun_core::analysis_problem::QueryAnalysisStage;

/// A validated membership change and its declaration site.
#[derive(Clone, Debug)]
pub struct ExtensionMembershipChange {
    extension: String,
    member: OwnershipIdentity,
    add: bool,
    source: Option<SourcePathWithSpan>,
}

impl ExtensionMembershipChange {
    /// Extension named by the declaration.
    pub fn extension(&self) -> &str {
        &self.extension
    }

    /// Exact identity of the existing object.
    pub const fn member(&self) -> &OwnershipIdentity {
        &self.member
    }

    /// Whether this operation establishes membership.
    pub const fn is_add(&self) -> bool {
        self.add
    }

    /// Source location, if the input has one.
    pub const fn source(&self) -> Option<&SourcePathWithSpan> {
        self.source.as_ref()
    }

    pub(crate) fn analyze(
        ast: &AstMembership,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, AnalysisError> {
        let extension = ast.extension();
        if !context.extension_exists(extension) {
            return Self::reject(context, &format!("extension {extension:?} does not exist"));
        }
        let lookup = Self::resolve(ast, context)?;
        let Some(member) = lookup.identity() else {
            context.report_analysis_problem(AnalysisProblem::ownership_target(&lookup))?;
            return Ok(None);
        };
        let current = context.extension_membership(member);
        let add = ast.is_add();
        if add {
            if let Some(existing) = current {
                return Self::reject(
                    context,
                    &format!("{member} is already a member of extension {existing:?}"),
                );
            }
            if member.kind() == ObjectKind::SCHEMA
                && context.extension_schema(extension) == Some(member.name().name().as_str())
            {
                return Self::reject(
                    context,
                    &format!("cannot add the extension's own schema to extension {extension:?}"),
                );
            }
        } else if current != Some(extension) {
            return Self::reject(
                context,
                &format!("{member} is not a member of extension {extension:?}"),
            );
        }
        Ok(Some(Self {
            extension: extension.to_owned(),
            member: member.clone(),
            add,
            source: context.get_source_path_with_span().cloned(),
        }))
    }

    fn resolve(
        ast: &AstMembership,
        context: &mut impl ParseContext,
    ) -> Result<OwnershipLookup, AnalysisError> {
        let kind = ast.kind();
        match kind {
            ObjectKind::CAST => return Self::resolve_cast(ast, context),
            ObjectKind::DOMAIN | ObjectKind::TYPE => return Self::resolve_type(ast, context),
            ObjectKind::TABLE => return Self::resolve_relation(ast, context, Kind::Table),
            ObjectKind::VIEW => return Self::resolve_relation(ast, context, Kind::View),
            ObjectKind::MATERIALIZED_VIEW => {
                return Self::resolve_relation(ast, context, Kind::MaterializedView);
            }
            ObjectKind::SEQUENCE => return Self::resolve_relation(ast, context, Kind::Sequence),
            ObjectKind::OPERATOR_CLASS => {
                let (name, method) = ast.access_method_target().expect("operator class target");
                let name = FullName::from_ast_name(name)?;
                let lookup = context.lookup_operator_class(&name, method);
                return Ok(Self::from_name_lookup(&lookup, |name| {
                    OwnershipIdentity::with_access_method(kind, name, method)
                }));
            }
            ObjectKind::COLLATION => {
                let target = ast.object_target().expect("collation target");
                let name = FullName::from_ast_name(&Name::from_parts(target.name()))?;
                let lookup = context.lookup_collation(&name);
                return Ok(Self::from_name_lookup(&lookup, |name| {
                    OwnershipIdentity::named(kind, name)
                }));
            }
            ObjectKind::AGGREGATE
            | ObjectKind::FOREIGN_DATA_WRAPPER
            | ObjectKind::FUNCTION
            | ObjectKind::OPERATOR
            | ObjectKind::PROCEDURE
            | ObjectKind::ROUTINE
            | ObjectKind::SCHEMA => {}
            _ => todo!("extension membership object resolution (#1189)"),
        }
        let target = ast
            .object_target()
            .expect("named or signature-based membership target");
        let requested = RequestedIdentity::from_ast_object_target(target, kind, context)?;
        Ok(match kind {
            ObjectKind::OPERATOR => requested.resolve_extension_operator(context),
            ObjectKind::AGGREGATE
            | ObjectKind::FUNCTION
            | ObjectKind::PROCEDURE
            | ObjectKind::ROUTINE => requested.resolve_extension_routine(context),
            _ => requested.resolve(context),
        })
    }

    fn from_name_lookup(
        lookup: &CatalogNameLookup,
        identity: impl Fn(&FullName) -> OwnershipIdentity,
    ) -> OwnershipLookup {
        if let Some(name) = lookup.resolved_name() {
            OwnershipLookup::resolved(&identity(name))
        } else if let Some(candidates) = lookup.ambiguous_candidates() {
            OwnershipLookup::ambiguous(&candidates.iter().map(identity).collect::<Vec<_>>())
        } else {
            OwnershipLookup::not_found(&identity(lookup.requested()))
        }
    }

    fn resolve_relation(
        ast: &AstMembership,
        context: &impl ParseContext,
        expected: Kind,
    ) -> Result<OwnershipLookup, AnalysisError> {
        let target = ast.object_target().expect("relation target");
        let name = FullName::from_ast_name(&Name::from_parts(target.name()))?;
        let class = ast.kind();
        let resolved = expand_relation_search_path(context, &name, |key| {
            context.object_kind(key).map(|kind| (key.clone(), kind))
        });
        Ok(match resolved {
            Some((key, kind)) if kind == expected => {
                OwnershipLookup::resolved(&OwnershipIdentity::named(class, &key))
            }
            Some((key, _)) => OwnershipLookup::invalid_kind(&OwnershipIdentity::named(class, &key)),
            None => OwnershipLookup::not_found(&OwnershipIdentity::named(class, &name)),
        })
    }

    fn resolve_type(
        ast: &AstMembership,
        context: &impl ParseContext,
    ) -> Result<OwnershipLookup, AnalysisError> {
        let target = ast.object_target().expect("type target");
        let ty = target.type_name().as_ref().expect("type name");
        if !ty.array_bounds().is_empty() {
            todo!("extension membership for array types by element spelling (#1189)");
        }
        let name = FullName::from_ast_name(ty.name())?;
        let requested = OwnershipIdentity::named(ast.kind(), &name);
        let Some(resolved) = context.resolve_type_name(&name) else {
            return Ok(OwnershipLookup::not_found(&requested));
        };
        let Some(definition) = context.get_type_impl(&resolved) else {
            return Ok(OwnershipLookup::not_found(&requested));
        };
        let kind = if definition.is_domain() {
            ObjectKind::DOMAIN
        } else if ast.kind() == ObjectKind::DOMAIN {
            return Ok(OwnershipLookup::invalid_kind(&requested));
        } else {
            ObjectKind::TYPE
        };
        Ok(OwnershipLookup::resolved(&OwnershipIdentity::named(
            kind, &resolved,
        )))
    }

    fn resolve_cast(
        ast: &AstMembership,
        context: &mut impl ParseContext,
    ) -> Result<OwnershipLookup, AnalysisError> {
        let (source, target) = ast.cast_types().expect("cast target");
        let mut missing = Vec::new();
        let mut types = Vec::new();
        for ty in [source, target] {
            let resolved = OwnershipIdentity::argument_type_from_ast(ty, context)?;
            if resolved.is_none() {
                missing.push(FullName::from_ast_name(ty.name())?);
            }
            types.push(resolved);
        }
        let identity = OwnershipIdentity::with_arguments(
            ObjectKind::CAST,
            &FullName::from_local_name("cast"),
            &types,
        );
        Ok(match types.as_slice() {
            [Some(source), Some(target)] if context.has_declared_cast(source, target) => {
                OwnershipLookup::resolved(&identity)
            }
            _ if !missing.is_empty() => {
                OwnershipLookup::argument_type_not_found(&identity, &missing)
            }
            _ => OwnershipLookup::not_found(&identity),
        })
    }

    fn reject(
        context: &mut impl ParseContext,
        message: &str,
    ) -> Result<Option<Self>, AnalysisError> {
        context.report_analysis_problem(AnalysisProblem::query_analysis(
            QueryAnalysisStage::Analysis,
            message,
        ))?;
        Ok(None)
    }

    pub(crate) fn created(
        extension: &str,
        member: &OwnershipIdentity,
        source: Option<&SourcePathWithSpan>,
    ) -> Self {
        Self {
            extension: extension.to_owned(),
            member: member.clone(),
            add: true,
            source: source.cloned(),
        }
    }
}

#[cfg(test)]
mod test_membership_validation {
    use crate::{
        context::ExtensionInfoRead,
        sem::SemAst,
        test_helpers::ownership::{apply_ownership_sql, ownership_context},
    };

    #[rstest::rstest]
    #[case::duplicate("ALTER EXTENSION one ADD FUNCTION f()", "query_analysis")]
    #[case::cross_extension("ALTER EXTENSION two ADD FUNCTION f()", "query_analysis")]
    #[case::wrong_extension("ALTER EXTENSION two DROP FUNCTION f()", "query_analysis")]
    #[case::unknown_extension("ALTER EXTENSION missing DROP FUNCTION f()", "query_analysis")]
    #[case::missing_object(
        "ALTER EXTENSION one ADD FUNCTION missing()",
        "ownership_target_not_found"
    )]
    #[case::ambiguous("ALTER EXTENSION one DROP FUNCTION f", "ownership_target_ambiguous")]
    #[case::wrong_relation_kind("ALTER EXTENSION one ADD TABLE v", "ownership_invalid_target_kind")]
    fn rejected_change_keeps_existing_membership(
        #[case] statement: &str,
        #[case] diagnostic: &str,
    ) -> testresult::TestResult {
        let mut context = ownership_context();
        for sql in [
            "CREATE EXTENSION one",
            "CREATE EXTENSION two",
            "CREATE VIEW public.v AS SELECT 1 AS value",
            "CREATE FUNCTION f() RETURNS integer LANGUAGE sql AS 'SELECT 1'",
            "CREATE FUNCTION f(text) RETURNS text LANGUAGE sql AS 'SELECT $1'",
        ] {
            context = apply_ownership_sql(context, sql)?.into_context();
        }
        let added = apply_ownership_sql(context, "ALTER EXTENSION one ADD FUNCTION f()")?;
        let SemAst::ExtensionMembership(change) = added.ast() else {
            panic!("membership ADD must succeed");
        };
        let member = change.member().clone();
        let rejected = apply_ownership_sql(added.into_context(), statement)?;
        assert!(matches!(rejected.ast(), SemAst::UnresolvedStatement));
        assert_eq!(rejected.context().problems().len(), 1);
        assert_eq!(rejected.context().problems()[0].code(), diagnostic);
        assert_eq!(
            rejected.context().extension_membership(&member),
            Some("one")
        );
        assert_eq!(
            rejected
                .context()
                .extension_membership_history(&member)
                .len(),
            1
        );
        Ok(())
    }
}
