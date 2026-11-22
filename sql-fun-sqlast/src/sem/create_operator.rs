//! PostgreSQL operator declaration analysis.

use crate::{
    StringSpan,
    ast::{
        Literal, ObjectKind,
        create_operator::{self as ast, OperatorOption, OperatorOptionValue},
    },
    context::ParseContext,
    names::{FullName, OperatorName, SchemaName},
    sem::{
        AnalysisError, AnalysisProblem, BinaryOperatorDefinition, ExactFunction,
        OperatorDefinition, OwnershipIdentity, TypeReference, UnaryOperatorDefinition,
        search_path::expand_operator_search_path, traversal::SemanticVisitor,
    },
};

/// Exact PostgreSQL operator signature in its creation namespace.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct OperatorIdentity {
    name: OperatorName,
    left: Option<TypeReference>,
    right: TypeReference,
}

impl OperatorIdentity {
    pub(crate) const fn name(&self) -> &OperatorName {
        &self.name
    }

    pub(crate) const fn binary(&self) -> bool {
        self.left.is_some()
    }

    fn arguments(&self) -> Vec<Option<TypeReference>> {
        self.left
            .iter()
            .chain([&self.right])
            .cloned()
            .map(Some)
            .collect()
    }

    pub(crate) fn extension_member(&self) -> Option<OwnershipIdentity> {
        let name = self.name.full_name()?;
        Some(OwnershipIdentity::with_arguments(
            ObjectKind::OPERATOR,
            &name,
            &[self.left.clone(), Some(self.right.clone())],
        ))
    }

    fn result_type<'a>(&self, context: &'a impl ParseContext) -> Option<&'a TypeReference> {
        match context.get_operator_op(&self.name, self.binary(), true)? {
            OperatorDefinition::Binary(overloads) => {
                let overload = overloads.iter().find(|overload| {
                    self.left.as_ref() == Some(overload.left_arg_type())
                        && &self.right == overload.right_arg_type()
                })?;
                Some(overload.get_return_type())
            }
            OperatorDefinition::RightUnary(overloads) => {
                let overload = overloads
                    .iter()
                    .find(|overload| &self.right == overload.arg_type())?;
                Some(overload.get_return_type())
            }
            OperatorDefinition::LeftUnary(_) => None,
        }
    }

    fn related(&self, name: OperatorName, commutator: bool) -> Self {
        if commutator {
            Self {
                name,
                left: Some(self.right.clone()),
                right: self.left.clone().unwrap_or_else(|| self.right.clone()),
            }
        } else {
            Self {
                name,
                left: self.left.clone(),
                right: self.right.clone(),
            }
        }
    }
}

/// Links retained for subsequent operator declarations, including shells.
#[derive(Clone, Debug, Default)]
pub struct OperatorLinks {
    commutator: Option<OperatorIdentity>,
    negator: Option<OperatorIdentity>,
}

impl OperatorLinks {
    pub(crate) const fn target(&self, commutator: bool) -> Option<&OperatorIdentity> {
        if commutator {
            self.commutator.as_ref()
        } else {
            self.negator.as_ref()
        }
    }

    pub(crate) fn with_reciprocal(mut self, commutator: bool, identity: &OperatorIdentity) -> Self {
        let link = if commutator {
            &mut self.commutator
        } else {
            &mut self.negator
        };
        if link.is_none() {
            *link = Some(identity.clone());
        }
        self
    }
}

/// An analyzed CREATE OPERATOR declaration.
#[derive(Clone, Debug)]
pub struct CreateOperator {
    identity: Option<OperatorIdentity>,
    implementation: Option<ExactFunction>,
    restriction: Option<ExactFunction>,
    join: Option<ExactFunction>,
    links: OperatorLinks,
    applicable: bool,
}

impl CreateOperator {
    /// Whether the declaration can be published in the analysis catalog.
    pub const fn applicable(&self) -> bool {
        self.applicable
    }

    pub(crate) fn definition(&self) -> Option<(&OperatorIdentity, OperatorDefinition)> {
        if !self.applicable {
            return None;
        }
        let identity = self.identity.as_ref()?;
        let implementation = self.implementation.as_ref()?.overload();
        let result = implementation.ret_type().as_ref()?;
        let definition = if let Some(left) = &identity.left {
            OperatorDefinition::Binary(vec![BinaryOperatorDefinition::new(
                left,
                &identity.right,
                result,
                implementation.is_strict(),
            )])
        } else {
            OperatorDefinition::RightUnary(vec![UnaryOperatorDefinition::new(
                &identity.right,
                result,
                implementation.is_strict(),
            )])
        };
        Some((identity, definition))
    }

    pub(crate) const fn links(&self) -> &OperatorLinks {
        &self.links
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
        for function in [&self.implementation, &self.restriction, &self.join]
            .into_iter()
            .flatten()
        {
            visitor.schema_function(function.name())?;
        }
        Ok(())
    }

    pub(crate) fn analyze(
        ast: &ast::CreateOperator,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let mut analysis = Declaration {
            context,
            applicable: true,
        };
        let identity = analysis.identity(ast)?;
        let function = option(ast, &["function", "procedure"]);
        let implementation = if let Some(function) = function {
            if let Some(identity) = &identity {
                analysis.function(function, &[&identity.arguments()])?
            } else {
                None
            }
        } else {
            analysis.reject("an implementation function is required", None)?;
            None
        };
        let restriction = analysis.estimator(option(ast, &["restrict"]), false)?;
        let join = analysis.estimator(option(ast, &["join"]), true)?;
        let links = analysis.attributes(
            ast,
            identity.as_ref(),
            implementation.as_ref(),
            join.is_some(),
        )?;
        Ok(Self {
            identity,
            implementation,
            restriction,
            join,
            links,
            applicable: analysis.applicable,
        })
    }
}

fn option<'a>(ast: &'a ast::CreateOperator, keys: &[&str]) -> Option<&'a OperatorOption> {
    ast.options()
        .iter()
        .rfind(|option| keys.contains(&option.key()))
}

fn builtin(name: &str) -> TypeReference {
    TypeReference::from_master_type_name(name)
}

struct Declaration<'a, C> {
    context: &'a mut C,
    applicable: bool,
}

impl<C: ParseContext> Declaration<'_, C> {
    fn reject(&mut self, message: &str, span: Option<StringSpan>) -> Result<(), AnalysisError> {
        self.applicable = false;
        self.context
            .report_analysis_problem(AnalysisProblem::InvalidOperatorDeclaration(
                message.to_owned(),
                span.as_ref().map(Into::into),
            ))
    }

    fn identity(
        &mut self,
        ast: &ast::CreateOperator,
    ) -> Result<Option<OperatorIdentity>, AnalysisError> {
        let name = match OperatorName::from_ast_name(ast.name()) {
            Ok(name) => name.full_name(),
            Err(problem) => {
                problem.report_problem_to(self.context)?;
                self.applicable = false;
                None
            }
        };
        let name = if let Some(name) = name {
            if !ast::valid_operator_token(name.name().as_str()) {
                self.reject(
                    "the declaration requires a valid PostgreSQL operator name",
                    None,
                )?;
                return Ok(None);
            }
            self.creation_name(&name, None)?
        } else {
            None
        };
        let left_option = option(ast, &["leftarg"]);
        let left = self.operand(left_option)?;
        let right = self.operand(option(ast, &["rightarg"]))?;
        let identity = if left_option.is_none() || left.is_some() {
            name.zip(right).map(|(name, right)| OperatorIdentity {
                name: OperatorName::from_full_name(&name),
                left,
                right,
            })
        } else {
            None
        };
        if option(ast, &["rightarg"]).is_none() {
            self.reject("a right operand type is required", None)?;
        }
        Ok(identity)
    }

    fn attributes(
        &mut self,
        ast: &ast::CreateOperator,
        identity: Option<&OperatorIdentity>,
        implementation: Option<&ExactFunction>,
        has_join: bool,
    ) -> Result<OperatorLinks, AnalysisError> {
        let commutator = option(ast, &["commutator"]);
        let negator = option(ast, &["negator"]);
        let hashes = self.flag(ast, &["hashes"])?;
        let merges = self.flag(ast, &["merges", "sort1", "sort2", "ltcmp", "gtcmp"])?;
        let mut links = OperatorLinks::default();
        if let Some(identity) = identity {
            if identity.result_type(self.context).is_some() {
                self.reject("this operator signature is already defined", None)?;
            }
            let result = if let Some(function) = implementation {
                function.overload().ret_type().as_ref()
            } else {
                None
            };
            links.commutator = self.validated_link(identity, result, commutator, true)?;
            links.negator = self.validated_link(identity, result, negator, false)?;
            if !identity.binary() && (commutator.is_some() || has_join || hashes || merges) {
                self.reject(
                    "only binary operators may specify a commutator, JOIN, HASHES, or MERGES",
                    None,
                )?;
            }
        }
        if let Some(function) = implementation {
            let result = function.overload().ret_type().as_ref();
            if self
                .context
                .function_returns_set(function.name(), function.overload())
                && let Some(option) = option(ast, &["function", "procedure"])
            {
                self.reject(
                    "analysis of operators implemented by set-returning functions is unsupported",
                    option.location(),
                )?;
            }
            if result.is_none() {
                self.reject(
                    "the implementation function must have a resolved return type",
                    None,
                )?;
            }
            if result != Some(&builtin("bool"))
                && (links.negator.is_some()
                    || option(ast, &["restrict"]).is_some()
                    || has_join
                    || hashes
                    || merges)
            {
                self.reject(
                    "only boolean operators may specify a negator, estimator, HASHES, or MERGES",
                    None,
                )?;
            }
        }
        Ok(links)
    }

    fn creation_name(
        &mut self,
        name: &FullName,
        span: Option<StringSpan>,
    ) -> Result<Option<FullName>, AnalysisError> {
        let exists = |schema: &SchemaName| {
            matches!(
                schema.as_str(),
                "public" | "pg_catalog" | "information_schema"
            ) || self.context.schema_definition(schema.as_str()).is_some()
        };
        if let Some(schema) = name.schema() {
            if exists(schema) {
                return Ok(Some(name.clone()));
            }
            self.reject(&format!("schema {schema} does not exist"), span)?;
        } else if let Some(schema) = self
            .context
            .get_search_path()
            .iter()
            .find(|schema| exists(schema))
        {
            return Ok(Some(name.replace_schema(schema)));
        } else {
            self.reject("no existing schema selected for operator creation", span)?;
        }
        Ok(None)
    }

    fn operand(
        &mut self,
        option: Option<&OperatorOption>,
    ) -> Result<Option<TypeReference>, AnalysisError> {
        let Some(option) = option else {
            return Ok(None);
        };
        if let Some(OperatorOptionValue::Type(name)) = option.value()
            && !name.set_of()
            && let Some(reference) = self
                .context
                .canonicalize_type_ref(&TypeReference::from_ast(name)?)
        {
            return Ok(Some(reference));
        }
        self.reject(
            &format!("{} requires an existing operand type", option.key()),
            option.location(),
        )?;

        Ok(None)
    }

    fn function(
        &mut self,
        option: &OperatorOption,
        signatures: &[&[Option<TypeReference>]],
    ) -> Result<Option<ExactFunction>, AnalysisError> {
        let name = if let Some(name) = option.name() {
            match name.parts() {
                [name] => Some(FullName::from_local_name(name)),
                [schema, name] => Some(FullName::with_schema(schema, name)),
                _ => None,
            }
        } else {
            None
        };
        if let Some(name) = name {
            let mut matches = signatures
                .iter()
                .filter_map(|signature| self.context.find_exact_function(&name, signature));
            if let Some(function) = matches.next()
                && matches.next().is_none()
                && !self.context.is_aggregate_impl(function.name())
            {
                return Ok(Some(function));
            }
        }
        self.reject(
            &format!(
                "{} requires a unique function with the exact input signature",
                option.key()
            ),
            option.location(),
        )?;
        Ok(None)
    }

    fn estimator(
        &mut self,
        option: Option<&OperatorOption>,
        join: bool,
    ) -> Result<Option<ExactFunction>, AnalysisError> {
        let Some(option) = option else {
            return Ok(None);
        };
        let mut signature = [
            "internal",
            "oid",
            "internal",
            if join { "int2" } else { "int4" },
        ]
        .map(|name| Some(builtin(name)))
        .to_vec();
        let function = if join {
            signature.push(Some(builtin("internal")));
            self.function(option, &[&signature, &signature[..4]])?
        } else {
            self.function(option, &[&signature])?
        };
        if let Some(function) = &function
            && function.overload().ret_type().as_ref() != Some(&builtin("float8"))
        {
            self.reject(
                "the estimator must return double precision",
                option.location(),
            )?;
        }
        Ok(function)
    }

    fn flag(&mut self, ast: &ast::CreateOperator, keys: &[&str]) -> Result<bool, AnalysisError> {
        let Some(option) = option(ast, keys) else {
            return Ok(false);
        };
        match option.value() {
            _ if !matches!(option.key(), "hashes" | "merges") => Ok(true),
            None => Ok(true),
            Some(OperatorOptionValue::Literal(Literal::Boolean(value))) => Ok(*value),
            Some(OperatorOptionValue::Literal(Literal::Integer(value @ (0 | 1)))) => {
                Ok(*value == 1)
            }
            Some(OperatorOptionValue::Literal(
                Literal::String(value) | Literal::Keyword(value),
            )) if ["true", "on"]
                .iter()
                .any(|expected| value.eq_ignore_ascii_case(expected)) =>
            {
                Ok(true)
            }
            Some(OperatorOptionValue::Literal(
                Literal::String(value) | Literal::Keyword(value),
            )) if ["false", "off"]
                .iter()
                .any(|expected| value.eq_ignore_ascii_case(expected)) =>
            {
                Ok(false)
            }
            _ => {
                self.reject(
                    &format!("{} requires a boolean value", option.key()),
                    option.location(),
                )?;
                Ok(false)
            }
        }
    }

    fn validated_link(
        &mut self,
        identity: &OperatorIdentity,
        result: Option<&TypeReference>,
        option: Option<&OperatorOption>,
        commutator: bool,
    ) -> Result<Option<OperatorIdentity>, AnalysisError> {
        let requested = self.link(identity, option, commutator)?;
        let inherited = match self.context.operator_links(identity) {
            Some(links) => links.target(commutator).cloned(),
            None => None,
        };
        let location = match option {
            Some(option) => option.location(),
            None => None,
        };
        if let (Some(requested), Some(inherited)) = (&requested, &inherited)
            && requested != inherited
        {
            self.reject(
                "cannot replace an existing operator shell relationship",
                location,
            )?;
        }
        let target = requested.or(inherited);
        if let Some(target) = &target {
            if let Some(links) = self.context.operator_links(target)
                && let Some(reciprocal) = links.target(commutator)
                && reciprocal != identity
            {
                self.reject(
                    "the referenced operator already has a different reciprocal relationship",
                    location,
                )?;
            }
            if let (Some(result), Some(other_result)) = (result, target.result_type(self.context))
                && result != other_result
            {
                self.reject("related operators must have the same return type", location)?;
            }
        }
        Ok(target)
    }

    fn link(
        &mut self,
        identity: &OperatorIdentity,
        option: Option<&OperatorOption>,
        commutator: bool,
    ) -> Result<Option<OperatorIdentity>, AnalysisError> {
        let Some(option) = option else {
            return Ok(None);
        };
        let name = if let Some(name) = option.name()
            && let Ok(name) = OperatorName::from_ast_name(&name)
        {
            name
        } else {
            self.reject("expected a valid operator name", option.location())?;
            return Ok(None);
        };
        let related = expand_operator_search_path(self.context, &name, |name| {
            let related = identity.related(name.clone(), commutator);
            if &related == identity
                || related.result_type(self.context).is_some()
                || self.context.operator_links(&related).is_some()
            {
                Some(related)
            } else {
                None
            }
        });
        let related = if let Some(related) = related {
            related
        } else {
            let Some(name) = name.full_name() else {
                return Ok(None);
            };
            if !ast::valid_operator_token(name.name().as_str()) {
                self.reject("the referenced operator name is invalid", option.location())?;
                return Ok(None);
            }
            let Some(name) = self.creation_name(&name, option.location())? else {
                return Ok(None);
            };
            identity.related(OperatorName::from_full_name(&name), commutator)
        };

        if !commutator && &related == identity {
            self.reject("an operator cannot be its own negator", option.location())?;
        }
        Ok(Some(related))
    }
}

#[cfg(test)]
mod test_operator_declarations {
    use crate::{
        context::{OperatorInfoRead, ParseContext},
        names::OperatorName,
        sem::{OperatorDefinition, OperatorIdentity, PgBuiltInType, SemAst},
        test_helpers::{
            ownership::{apply_ownership_sql, ownership_context},
            schema_layer,
        },
    };

    #[test]
    fn completes_shell_and_preserves_other_overloads_across_layers() -> testresult::TestResult {
        let mut context = ownership_context();
        for sql in [
            "CREATE FUNCTION iop(integer, integer) RETURNS integer LANGUAGE sql STRICT AS 'SELECT $1'",
            "CREATE FUNCTION top(text, text) RETURNS text LANGUAGE sql AS 'SELECT $1'",
            "CREATE OPERATOR %% (LEFTARG=integer, RIGHTARG=integer, FUNCTION=iop, COMMUTATOR='@#@#')",
        ] {
            context = apply_ownership_sql(context, sql)?.into_context();
        }
        let shell = OperatorIdentity {
            name: OperatorName::from_parts(Some("public"), "@#@#"),
            left: Some(PgBuiltInType::int4()),
            right: PgBuiltInType::int4(),
        };
        assert!(context.operator_links(&shell).is_some());
        assert!(context.get_operator_op(shell.name(), true, true).is_none());
        let base = context.freeze();
        let mut context = schema_layer(std::path::Path::new("operators.sql"), &base);
        for sql in [
            "CREATE OPERATOR @#@# (LEFTARG=integer, RIGHTARG=integer, PROCEDURE=iop)",
            "CREATE OPERATOR %% (LEFTARG=text, RIGHTARG=text, FUNCTION=top)",
        ] {
            context = apply_ownership_sql(context, sql)?.into_context();
        }
        assert!(context.get_operator_op(shell.name(), true, true).is_some());
        assert_eq!(
            context
                .operator_links(&shell)
                .unwrap()
                .target(true)
                .unwrap()
                .name(),
            &OperatorName::from_parts(Some("public"), "%%")
        );
        assert!(base.get_operator_op(shell.name(), true, true).is_none());
        let name = OperatorName::from_parts(Some("public"), "%%");
        let Some(OperatorDefinition::Binary(overloads)) =
            context.get_operator_op(&name, true, true)
        else {
            panic!("expected binary operator");
        };
        assert_eq!(overloads.len(), 2);
        assert!(
            overloads.iter().any(|overload| overload.is_strict()
                && overload.get_return_type() == &PgBuiltInType::int4())
        );
        assert!(overloads.iter().any(|overload| !overload.is_strict()
            && overload.get_return_type() == &PgBuiltInType::text()));
        Ok(())
    }

    #[rstest::rstest]
    #[case(
        "CREATE OPERATOR @#@# (LEFTARG=integer, RIGHTARG=integer, FUNCTION=iop, COMMUTATOR='!!!')"
    )]
    #[case("CREATE OPERATOR @#@# (LEFTARG=integer, RIGHTARG=integer, FUNCTION=top)")]
    #[case(
        "CREATE OPERATOR @#@# (LEFTARG=integer, RIGHTARG=integer, FUNCTION=iop, COMMUTATOR='@#@#')"
    )]
    fn rejects_inconsistent_shell_completion(#[case] sql: &str) -> testresult::TestResult {
        let mut context = ownership_context();
        for setup in [
            "CREATE FUNCTION iop(integer, integer) RETURNS integer LANGUAGE sql AS 'SELECT $1'",
            "CREATE FUNCTION top(integer, integer) RETURNS text LANGUAGE sql AS 'SELECT $1::text'",
            "CREATE OPERATOR %% (LEFTARG=integer, RIGHTARG=integer, FUNCTION=iop, COMMUTATOR='@#@#')",
        ] {
            context = apply_ownership_sql(context, setup)?.into_context();
        }
        let result = apply_ownership_sql(context, sql)?;
        let SemAst::CreateOperator(statement) = result.ast() else {
            panic!("expected analyzed operator declaration");
        };
        assert!(!statement.applicable());
        let shell = OperatorIdentity {
            name: OperatorName::from_parts(Some("public"), "@#@#"),
            left: Some(PgBuiltInType::int4()),
            right: PgBuiltInType::int4(),
        };
        assert!(
            result
                .context()
                .get_operator_op(shell.name(), true, true)
                .is_none()
        );
        assert_eq!(
            result
                .context()
                .operator_links(&shell)
                .unwrap()
                .target(true)
                .unwrap()
                .name(),
            &OperatorName::from_parts(Some("public"), "%%")
        );
        Ok(())
    }

    #[test]
    fn invalid_implementation_does_not_publish_operator_or_shell() -> testresult::TestResult {
        let result = apply_ownership_sql(
            ownership_context(),
            "CREATE OPERATOR %% (LEFTARG=integer, RIGHTARG=integer, FUNCTION=missing, COMMUTATOR='@#@#')",
        )?;
        let SemAst::CreateOperator(statement) = result.ast() else {
            panic!("expected analyzed operator declaration");
        };
        assert!(!statement.applicable());
        let context = result.into_context();
        assert_eq!(context.problems()[0].code(), "invalid_operator_declaration");
        let shell = OperatorIdentity {
            name: OperatorName::from_parts(Some("public"), "@#@#"),
            left: Some(PgBuiltInType::int4()),
            right: PgBuiltInType::int4(),
        };
        assert!(context.operator_links(&shell).is_none());
        assert!(
            context
                .get_operator_op(&OperatorName::from_parts(Some("public"), "%%"), true, true)
                .is_none()
        );
        Ok(())
    }
}
