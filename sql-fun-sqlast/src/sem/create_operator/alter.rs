//! PostgreSQL ALTER OPERATOR SET analysis.

use crate::{
    ast::{
        Literal, TypeName,
        create_operator::{AlterOperator as Syntax, OperatorOption, OperatorOptionValue},
    },
    context::ParseContext,
    names::{FullName, OperatorName},
    sem::{
        AnalysisError, TypeReference,
        create_operator::{
            Declaration, OperatorIdentity, OperatorLinks, builtin, estimator_identity,
        },
        search_path::expand_operator_search_path,
        traversal::SemanticVisitor,
    },
};
use ::sql_fun_core::PostgresVersion;

/// Resolved ALTER OPERATOR SET and its catalog changes.
#[derive(Clone, Debug)]
pub struct AlterOperator {
    identity: Option<OperatorIdentity>,
    estimators: Vec<FullName>,
    updates: Vec<(OperatorIdentity, OperatorLinks)>,
    applicable: bool,
}

impl AlterOperator {
    /// Whether the statement can update the catalog.
    pub const fn applicable(&self) -> bool {
        self.applicable
    }

    pub(crate) fn updates(&self) -> &[(OperatorIdentity, OperatorLinks)] {
        if self.applicable { &self.updates } else { &[] }
    }

    pub(crate) fn visit_references<V: SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        if let Some(identity) = &self.identity {
            for operand in identity.left.iter().chain([&identity.right]) {
                visitor.schema_type(operand)?;
            }
        }
        for estimator in &self.estimators {
            visitor.schema_function(estimator)?;
        }
        Ok(())
    }

    pub(crate) fn analyze(
        syntax: &Syntax,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let mut analysis = Declaration {
            context,
            applicable: true,
        };
        let identity = analysis.alter_target(syntax)?;
        let mut updates = Vec::new();
        let mut estimators = Vec::new();
        if let Some(identity) = &identity {
            analysis.alter_settings(syntax, identity, &mut estimators, &mut updates)?;
        }
        Ok(Self {
            identity,
            estimators,
            updates,
            applicable: analysis.applicable,
        })
    }
}

impl<C: ParseContext> Declaration<'_, C> {
    fn alter_target(&mut self, syntax: &Syntax) -> Result<Option<OperatorIdentity>, AnalysisError> {
        let name = match OperatorName::from_ast_name(syntax.name()) {
            Ok(name) => name,
            Err(problem) => {
                problem.report_problem_to(self.context)?;
                self.applicable = false;
                return Ok(None);
            }
        };
        let [left, right] = syntax
            .operands()
            .each_ref()
            .map(|operand| operand.type_name().as_ref());
        let resolve = |name: &TypeName| {
            Ok::<_, AnalysisError>(
                self.context
                    .canonicalize_type_ref(&TypeReference::from_ast(name)?),
            )
        };
        let left_type = left.map(resolve).transpose()?.flatten();
        let right_type = right.map(resolve).transpose()?.flatten();
        if let Some(right_type) = right_type
            && (left.is_none() || left_type.is_some())
            && let Some((identity, _)) =
                OperatorIdentity::resolve(&name, left_type.as_ref(), &right_type, self.context)
        {
            return Ok(Some(identity));
        }
        self.reject(
            "ALTER OPERATOR requires an existing exact operator signature",
            None,
        )?;
        Ok(None)
    }

    fn alter_settings(
        &mut self,
        syntax: &Syntax,
        identity: &OperatorIdentity,
        estimators: &mut Vec<FullName>,
        updates: &mut Vec<(OperatorIdentity, OperatorLinks)>,
    ) -> Result<(), AnalysisError> {
        let modern = if let Ok(version) = self
            .context
            .get_args()
            .postgres_version(self.context.metadata())
            && let Ok(version) = version.to_string().parse::<PostgresVersion>()
        {
            version.major() >= 17
        } else {
            false
        };
        let boolean = identity.result_type(self.context) == Some(&builtin("bool"));
        let mut settings = self
            .context
            .operator_links(identity)
            .cloned()
            .unwrap_or_default();
        let options = syntax.options();
        for (index, option) in options.iter().enumerate() {
            let key = option.key();
            self.validate_alter_option_value(option, modern)?;
            if options[index + 1..].iter().any(|later| later.key() == key) {
                continue;
            }
            match key {
                "restrict" | "join" => {
                    self.alter_estimator(identity, boolean, option, &mut settings, estimators)?;
                }
                "hashes" | "merges" if modern => {
                    if !settings.flags_and_links_known {
                        self.reject(
                            "operator settings are missing from the catalog",
                            option.location(),
                        )?;
                        continue;
                    }
                    self.alter_flag(identity, boolean, option, &mut settings)?;
                }
                "commutator" | "negator" if modern => {
                    if !settings.flags_and_links_known {
                        self.reject(
                            "operator settings are missing from the catalog",
                            option.location(),
                        )?;
                        continue;
                    }
                    self.alter_link(identity, boolean, option, &mut settings, updates)?;
                }
                "commutator" | "negator" | "hashes" | "merges" | "leftarg" | "rightarg"
                | "function" | "procedure" => {
                    self.reject(
                        &format!("operator attribute {key} cannot be changed"),
                        option.location(),
                    )?;
                }
                _ => self.reject(
                    &format!("operator attribute {key} is not recognized"),
                    option.location(),
                )?,
            }
        }
        updates.insert(0, (identity.clone(), settings));
        Ok(())
    }

    fn alter_estimator(
        &mut self,
        identity: &OperatorIdentity,
        boolean: bool,
        option: &OperatorOption,
        settings: &mut OperatorLinks,
        estimators: &mut Vec<FullName>,
    ) -> Result<(), AnalysisError> {
        let join = option.key() == "join";
        let value = if option.value().is_none() {
            None
        } else {
            self.estimator(Some(option), join)?
        };
        if let Some(value) = &value {
            if !boolean || join && !identity.binary() {
                self.reject(
                    "estimator is incompatible with the operator signature",
                    option.location(),
                )?;
            }
            estimators.push(value.name().clone());
        }
        let destination = if join {
            &mut settings.join
        } else {
            &mut settings.restriction
        };
        *destination = Some(value.as_ref().map(estimator_identity));
        Ok(())
    }

    fn alter_flag(
        &mut self,
        identity: &OperatorIdentity,
        boolean: bool,
        option: &OperatorOption,
        settings: &mut OperatorLinks,
    ) -> Result<(), AnalysisError> {
        let Some(value) = flag_value(option) else {
            return Ok(()); // Already diagnosed during option validation.
        };
        let flag = if option.key() == "hashes" {
            &mut settings.hashes
        } else {
            &mut settings.merges
        };
        if *flag && !value {
            self.reject(
                "an enabled operator flag cannot be disabled",
                option.location(),
            )?;
        } else if value && (!identity.binary() || !boolean) {
            self.reject(
                "operator flag requires a binary boolean operator",
                option.location(),
            )?;
        }
        *flag = value;
        Ok(())
    }

    fn validate_alter_option_value(
        &mut self,
        option: &OperatorOption,
        modern: bool,
    ) -> Result<(), AnalysisError> {
        if matches!(option.key(), "restrict" | "join" | "commutator" | "negator")
            && option.value().is_some()
            && option.name().is_none()
        {
            self.reject("operator attribute requires a name", option.location())?;
        }
        if modern && matches!(option.key(), "hashes" | "merges") && flag_value(option).is_none() {
            self.reject("operator flag requires a boolean value", option.location())?;
        }
        Ok(())
    }

    fn alter_link(
        &mut self,
        identity: &OperatorIdentity,
        boolean: bool,
        option: &OperatorOption,
        settings: &mut OperatorLinks,
        updates: &mut Vec<(OperatorIdentity, OperatorLinks)>,
    ) -> Result<(), AnalysisError> {
        let commutator = option.key() == "commutator";
        if commutator && !identity.binary() || !commutator && !boolean {
            return self.reject(
                "operator link is incompatible with the operator signature",
                option.location(),
            );
        }
        let related = if let Some(name) = option.name()
            && let Ok(name) = OperatorName::from_ast_name(&name)
        {
            expand_operator_search_path(self.context, &name, |name| {
                let related = identity.related(name.clone(), commutator);
                related.result_type(self.context).map(|_| related)
            })
        } else {
            None
        };
        let Some(related) = related else {
            return self.reject(
                "operator link requires an existing exact operator signature",
                option.location(),
            );
        };
        if !commutator && &related == identity {
            return self.reject("an operator cannot be its own negator", option.location());
        }
        if settings
            .target(commutator)
            .is_some_and(|current| current != &related)
        {
            return self.reject(
                "an existing operator link cannot be changed",
                option.location(),
            );
        }
        let result = identity.result_type(self.context).cloned();
        self.validate_relationship(
            identity,
            result.as_ref(),
            &related,
            commutator,
            option.location(),
        )?;
        if &related != identity {
            let reciprocal = if let Some((_, settings)) =
                updates.iter_mut().find(|(target, _)| target == &related)
            {
                settings
            } else {
                let settings = self
                    .context
                    .operator_links(&related)
                    .cloned()
                    .unwrap_or_default();
                updates.push((related.clone(), settings));
                &mut updates.last_mut().expect("inserted update").1
            };
            if !reciprocal.flags_and_links_known {
                return self.reject(
                    "referenced operator settings are missing from the catalog",
                    option.location(),
                );
            }
            *reciprocal = reciprocal.clone().with_reciprocal(commutator, identity);
        }
        *settings = settings.clone().with_reciprocal(commutator, &related);
        Ok(())
    }
}

fn flag_value(option: &OperatorOption) -> Option<bool> {
    let name = match option.value() {
        None => return Some(true),
        Some(OperatorOptionValue::Literal(Literal::Boolean(value))) => return Some(*value),
        Some(OperatorOptionValue::Literal(Literal::String(value) | Literal::Keyword(value))) => {
            Some(value.as_str())
        }
        Some(OperatorOptionValue::Type(name)) => match name.name().parts() {
            [name] => Some(name.as_str()),
            _ => None,
        },
        _ => None,
    };
    if let Some(name) = name {
        if name.eq_ignore_ascii_case("true") || name.eq_ignore_ascii_case("on") {
            return Some(true);
        }
        if name.eq_ignore_ascii_case("false") || name.eq_ignore_ascii_case("off") {
            return Some(false);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::{
        context::{BaseParseContext, OperatorInfoRead, ParseContext, SchemaFileContext},
        names::SchemaName,
        sem::{AnalysisProblem, SemAst, create_operator::builtin},
        test_helpers::{TestParseContext, ownership::apply_ownership_sql, schema_layer},
    };
    use std::path::Path;

    #[test]
    fn rejected_update_preserves_estimators_and_later_updates_apply() -> testresult::TestResult {
        let mut inventory = TestParseContext::default();
        inventory.set_get_search_path_result(&[SchemaName::from("public")]);
        for name in ["int4", "bool", "internal", "oid", "int2", "float8"] {
            inventory.setup_type(builtin(name));
        }
        let base = inventory.freeze();
        let mut context = schema_layer(Path::new("operator-settings.sql"), &base);
        for sql in [
            "CREATE FUNCTION op(integer, integer) RETURNS boolean LANGUAGE sql AS 'SELECT true'",
            "CREATE FUNCTION rs(internal, oid, internal, integer) RETURNS double precision LANGUAGE internal AS 'eqsel'",
            "CREATE FUNCTION js(internal, oid, internal, smallint, internal) RETURNS double precision LANGUAGE internal AS 'eqjoinsel'",
            "CREATE OPERATOR %% (LEFTARG=integer, RIGHTARG=integer, FUNCTION=op, RESTRICT=rs, JOIN=js)",
        ] {
            context = apply_ownership_sql(context, sql)?.into_context();
        }
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let base = context.freeze();
        let context = schema_layer(Path::new("alter-settings.sql"), &base);
        let rejected = apply_ownership_sql(
            context,
            "ALTER OPERATOR %% (integer,integer) SET (RESTRICT=NONE, JOIN=missing)",
        )?;
        let SemAst::AlterOperator(statement) = rejected.ast() else {
            panic!("expected ALTER OPERATOR");
        };
        assert!(!statement.applicable());
        let identity = statement.identity.clone().expect("resolved target");
        let context = rejected.into_context();
        assert!(matches!(
            context.problems(),
            [AnalysisProblem::InvalidOperatorDeclaration(..)]
        ));
        let settings = context
            .operator_links(&identity)
            .expect("original settings");
        assert!(matches!(settings.restriction, Some(Some(_))));
        let join = settings.join.clone().expect("original join estimator");
        let applied = apply_ownership_sql(
            context,
            "ALTER OPERATOR %% (integer,integer) SET (RESTRICT=NONE)",
        )?;
        assert!(
            matches!(applied.ast(), SemAst::AlterOperator(statement) if statement.applicable())
        );
        let context = applied.into_context();
        let settings = context.operator_links(&identity).expect("updated settings");
        assert_eq!(settings.restriction, Some(None));
        assert_eq!(settings.join.as_ref(), Some(&join));
        assert!(identity.result_type(&context).is_some());
        Ok(())
    }

    #[rstest::rstest]
    #[case::postgres16("16", false)]
    #[case::postgres17("17", true)]
    fn additional_attributes_use_selected_postgres_version(
        #[case] version: &str,
        #[case] supported: bool,
    ) -> testresult::TestResult {
        let mut inventory = TestParseContext::default().with_postgres_version(version);
        inventory.set_get_search_path_result(&[SchemaName::from("public")]);
        for name in ["int4", "bool"] {
            inventory.setup_type(builtin(name));
        }
        let base = inventory.freeze();
        let mut context = SchemaFileContext::new(
            Path::new("operator-settings.sql"),
            &base,
            base.get_args(),
            base.metadata(),
        );
        for sql in [
            "CREATE FUNCTION op(integer, integer) RETURNS boolean LANGUAGE sql AS 'SELECT true'",
            "CREATE OPERATOR %% (LEFTARG=integer, RIGHTARG=integer, FUNCTION=op)",
            "CREATE OPERATOR @@@ (LEFTARG=integer, RIGHTARG=integer, FUNCTION=op)",
        ] {
            context = apply_ownership_sql(context, sql)?.into_context();
        }
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let result = apply_ownership_sql(
            context,
            "ALTER OPERATOR %% (integer,integer) SET (HASHES=on, MERGES, COMMUTATOR = %%, NEGATOR = @@@)",
        )?;
        let SemAst::AlterOperator(statement) = result.ast() else {
            panic!("expected ALTER OPERATOR");
        };
        assert_eq!(statement.applicable(), supported);
        let identity = statement.identity.clone().expect("resolved target");
        let context = result.into_context();
        let settings = context
            .operator_links(&identity)
            .expect("operator settings");
        assert_eq!(settings.hashes, supported);
        assert_eq!(settings.merges, supported);
        if supported {
            assert_eq!(settings.target(true), Some(&identity));
            let negator = settings.target(false).expect("assigned negator");
            let reciprocal = context
                .operator_links(negator)
                .expect("reciprocal settings");
            assert_eq!(reciprocal.target(false), Some(&identity));
        } else {
            assert!(settings.target(true).is_none());
            assert!(settings.target(false).is_none());
        }
        Ok(())
    }
}
