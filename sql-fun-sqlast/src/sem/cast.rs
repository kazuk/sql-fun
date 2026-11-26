use crate::{
    ast::{
        ObjectKind, ObjectTarget, TypeName,
        create_cast::{
            CastContext as AstCastContext, CastMethod as AstCastMethod, CreateCast as AstCreateCast,
        },
    },
    context::ParseContext,
    sem::{
        AnalysisError, AnalysisProblem, OwnershipIdentity, PgBuiltInType, TypeDefinition,
        TypeReference, ownership::RequestedIdentity,
    },
};

/// type cast definition
#[derive(Debug, Clone)]
pub struct CastDefinition {
    cast_context: CastContext,
    method: Option<CastMethod>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub(crate) enum CastMethod {
    #[serde(rename = "f")]
    Function,
    #[serde(rename = "b")]
    Binary,
    #[serde(rename = "i")]
    InOut,
}

/// type cast context
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CastContext {
    /// implicit and prefeerd
    ImplicitPrefeerd,
    /// implicit
    Implicit,
    /// assignment
    Assignment,
    /// explicit
    Explicit,
    /// non-conversion (same type)
    NoConversion,
}

impl CastDefinition {
    /// create with context
    #[must_use]
    pub const fn new(cc: CastContext) -> Self {
        Self {
            cast_context: cc,
            method: None,
        }
    }

    pub(crate) const fn declared(cast_context: CastContext, method: CastMethod) -> Self {
        Self {
            cast_context,
            method: Some(method),
        }
    }

    pub(crate) const fn is_implicit_binary(&self) -> bool {
        self.use_in_implicit() && matches!(self.method, Some(CastMethod::Binary))
    }

    /// is explicit
    #[must_use]
    pub const fn use_in_explicit(&self) -> bool {
        self.cast_context.use_in_explicit()
    }

    /// is implicit
    #[must_use]
    pub const fn use_in_implicit(&self) -> bool {
        self.cast_context.use_in_implicit()
    }

    /// get context
    #[must_use]
    pub const fn context(&self) -> CastContext {
        self.cast_context
    }

    /// in implicit and preferred
    #[must_use]
    pub const fn is_implicit_preferred(&self) -> bool {
        self.cast_context.is_implicit_preferred()
    }

    /// is no conversion
    #[must_use]
    pub const fn is_no_conversion(&self) -> bool {
        self.cast_context.is_no_conversion()
    }
}

impl CastContext {
    /// Construct a context for an exact type match.
    pub(crate) const fn no_conversion() -> Self {
        Self::NoConversion
    }

    /// Construct an implicit conversion context.
    pub(crate) const fn implicit() -> Self {
        Self::Implicit
    }

    /// Construct a preferred implicit context for semantic test fixtures.
    #[cfg(test)]
    pub(crate) const fn implicit_preferred() -> Self {
        Self::ImplicitPrefeerd
    }

    /// Construct an explicit cast context.
    pub(crate) const fn explicit() -> Self {
        Self::Explicit
    }

    /// Construct an assignment cast context.
    pub(crate) const fn assignment() -> Self {
        Self::Assignment
    }

    /// cast is defined, always use explicit
    #[must_use]
    pub const fn use_in_explicit(&self) -> bool {
        true
    }

    /// use in implicit
    #[must_use]
    pub const fn use_in_implicit(&self) -> bool {
        matches!(
            self,
            Self::Implicit | Self::ImplicitPrefeerd | Self::NoConversion
        )
    }

    /// use in assignment context
    #[must_use]
    pub const fn use_in_assignment(&self) -> bool {
        matches!(
            self,
            Self::Implicit | Self::ImplicitPrefeerd | Self::Assignment | Self::NoConversion
        )
    }

    /// Higher score means better match in overload resolution.
    #[must_use]
    pub const fn overload_priority_score(&self) -> i32 {
        match self {
            Self::NoConversion => 10,
            Self::ImplicitPrefeerd => 9,
            Self::Implicit => 5,
            // can not to use in overload resolution
            _ => 0,
        }
    }

    /// type cast is implicit
    #[must_use]
    pub const fn is_implicit(&self) -> bool {
        matches!(self, Self::Implicit | Self::ImplicitPrefeerd)
    }

    /// test cast is implicit and prefeered
    #[must_use]
    pub const fn is_implicit_preferred(&self) -> bool {
        matches!(self, Self::ImplicitPrefeerd)
    }

    /// test cast is non-conversion
    #[must_use]
    pub const fn is_no_conversion(&self) -> bool {
        matches!(self, Self::NoConversion)
    }
}

/// Analyzed CREATE CAST declaration.
#[derive(Debug, Clone)]
pub struct CreateCast {
    source: TypeReference,
    target: TypeReference,
    definition: CastDefinition,
}

impl CreateCast {
    pub(crate) const fn new(
        source: TypeReference,
        target: TypeReference,
        definition: CastDefinition,
    ) -> Self {
        Self {
            source,
            target,
            definition,
        }
    }

    pub(crate) const fn entry(&self) -> (&TypeReference, &TypeReference, &CastDefinition) {
        (&self.source, &self.target, &self.definition)
    }
}

impl CreateCast {
    pub(crate) fn analyze(
        ast: &AstCreateCast,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, AnalysisError> {
        match Self::validated(ast, context) {
            Ok(declaration) => {
                let domain = if Self::is_domain(&declaration.source, context) {
                    Some("source")
                } else if Self::is_domain(&declaration.target, context) {
                    Some("target")
                } else {
                    None
                };
                if let Some(endpoint) = domain {
                    context.report_analysis_problem(AnalysisProblem::IgnoredDomainCast(
                        format!(
                            "cast will be ignored because the {endpoint} data type is a domain"
                        ),
                        None,
                    ))?;
                }
                if matches!(declaration.definition.method, Some(CastMethod::Binary)) {
                    context.report_analysis_problem(
                        AnalysisProblem::UnverifiedBinaryCastCompatibility(None),
                    )?;
                }
                Ok(Some(declaration))
            }
            Err(AnalysisError::UnexpectedInput(reason)) => {
                context.report_analysis_problem(AnalysisProblem::InvalidCastDeclaration(
                    reason, None,
                ))?;
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    fn is_domain(reference: &TypeReference, context: &impl ParseContext) -> bool {
        reference.is_array() != Some(true)
            && reference
                .definition_from_catalog(context)
                .is_some_and(TypeDefinition::is_domain)
    }

    fn validated(
        ast: &AstCreateCast,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let source = Self::resolve_type(ast.source(), context)?;
        let target = Self::resolve_type(ast.target(), context)?;
        if context.has_declared_cast(&source, &target) {
            return Err(Self::invalid("cast already exists"));
        }
        let method = match ast.method() {
            AstCastMethod::Function(function) => {
                Self::validate_function(function, &source, &target, context)?;
                CastMethod::Function
            }
            AstCastMethod::Binary => CastMethod::Binary,
            AstCastMethod::InOut => CastMethod::InOut,
        };
        if source == target && !matches!(method, CastMethod::Function) {
            return Err(Self::invalid(
                "source data type and target data type are the same",
            ));
        }
        let coercion = match ast.context() {
            AstCastContext::Explicit => CastContext::explicit(),
            AstCastContext::Assignment => CastContext::assignment(),
            AstCastContext::Implicit => CastContext::implicit(),
        };
        Ok(Self::new(
            source,
            target,
            CastDefinition::declared(coercion, method),
        ))
    }

    fn resolve_type(
        ast: &TypeName,
        context: &mut impl ParseContext,
    ) -> Result<TypeReference, AnalysisError> {
        let reference = OwnershipIdentity::argument_type_from_ast(ast, context)?
            .ok_or_else(|| Self::invalid("cast type does not exist"))?;
        let definition = reference.definition_from_catalog(context);
        if definition.is_some_and(|ty| {
            ty.is_shell()
                || (ty.is_builtin()
                    && ty
                        .intrinsic_category()
                        .is_some_and(|category| category.code() == 'P'))
        }) {
            return Err(Self::invalid(
                "cast endpoint must be a defined non-pseudo type",
            ));
        }
        Ok(reference)
    }

    fn validate_function(
        function: &ObjectTarget,
        source: &TypeReference,
        target: &TypeReference,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        let request =
            RequestedIdentity::from_ast_object_target(function, ObjectKind::FUNCTION, context)?;
        let lookup = request.resolve_routine(context);
        let identity = lookup
            .identity()
            .ok_or_else(|| Self::invalid("cast function could not be resolved"))?;
        let definition = context
            .exact_routine_definition(identity)
            .ok_or_else(|| Self::invalid("cast function could not be resolved"))?;
        if !definition.is_ordinary_scalar_cast_function(context) {
            return Err(Self::invalid(
                "cast function must be an ordinary non-set-returning function",
            ));
        }
        let arguments = identity.arguments().unwrap_or_default();
        if !(1..=3).contains(&arguments.len()) {
            return Err(Self::invalid(
                "cast function must take one to three arguments",
            ));
        }
        if source == target && arguments.len() < 2 {
            return Err(Self::invalid(
                "source data type and target data type are the same",
            ));
        }
        if arguments
            .get(1)
            .is_some_and(|argument| argument.as_ref() != Some(&PgBuiltInType::int4()))
            || arguments
                .get(2)
                .is_some_and(|argument| argument.as_ref() != Some(&PgBuiltInType::bool()))
        {
            return Err(Self::invalid(
                "second and third cast function arguments must be integer and boolean",
            ));
        }
        if !arguments[0]
            .as_ref()
            .is_some_and(|argument| source.is_binary_coercible_to(argument, context))
        {
            return Err(Self::invalid(
                "cast function input is not binary-coercible from source type",
            ));
        }
        let result = definition.overloads().common_scalar_result();
        if !result
            .as_ref()
            .is_some_and(|result| result.is_binary_coercible_to(target, context))
        {
            return Err(Self::invalid(
                "cast function result is not binary-coercible to target type",
            ));
        }
        Ok(())
    }

    fn invalid(reason: &str) -> AnalysisError {
        AnalysisError::unexpected_input_error(&format!("invalid CREATE CAST: {reason}"))
    }
}

#[cfg(test)]
mod application_tests {
    use crate::{
        context::{CastInfoRead, ParseContext},
        names::FullName,
        sem::TypeReference,
        test_helpers::{
            ownership::{apply_ownership_sql, ownership_context},
            schema_layer,
        },
    };

    #[rstest::rstest]
    #[case("", false, false)]
    #[case("AS ASSIGNMENT", true, false)]
    #[case("AS IMPLICIT", true, true)]
    fn published_context_survives_freezing(
        #[case] clause: &str,
        #[case] assignment: bool,
        #[case] implicit: bool,
        #[values("WITH INOUT", "WITHOUT FUNCTION")] method: &str,
    ) -> testresult::TestResult {
        let context = apply_ownership_sql(
            ownership_context(),
            "CREATE TYPE public.cast_a AS ENUM ('ok')",
        )?
        .into_context();
        let context = apply_ownership_sql(context, "CREATE TYPE public.cast_b AS ENUM ('ok')")?
            .into_context();
        let source =
            TypeReference::concrete_type_ref(&FullName::with_schema("public", "cast_a"), false);
        let target =
            TypeReference::concrete_type_ref(&FullName::with_schema("public", "cast_b"), false);

        let declaration = format!("CREATE CAST (public.cast_a AS public.cast_b) {method} {clause}");
        let context = apply_ownership_sql(context, &declaration)?.into_context();
        let binary = method == "WITHOUT FUNCTION";
        if binary {
            let [problem] = context.problems() else {
                panic!("expected one binary cast warning");
            };
            assert_eq!(problem.code(), "unverified_binary_cast_compatibility");
            assert_eq!(
                problem.severity(),
                sql_fun_core::analysis_problem::DiagnosticSeverity::Warning
            );
        } else {
            assert!(context.problems().is_empty());
        }
        let frozen = context.freeze();
        let layer = schema_layer(std::path::Path::new("later.sql"), &frozen);
        assert!(layer.has_declared_cast(&source, &target));
        assert!(!layer.has_declared_cast(&target, &source));
        let cast = layer
            .get_explicit_cast(&source, &target)
            .expect("published cast");
        assert_eq!(
            matches!(cast.method, Some(super::CastMethod::Binary)),
            binary
        );
        assert_eq!(
            layer.get_assignment_cast(&source, &target).is_some(),
            assignment
        );
        assert_eq!(
            layer.get_implicit_cast(&source, &target).is_some(),
            implicit
        );

        let duplicate = apply_ownership_sql(layer, &declaration)?.into_context();
        assert_eq!(duplicate.problems()[0].code(), "invalid_cast_declaration");
        assert_eq!(
            duplicate.get_implicit_cast(&source, &target).is_some(),
            implicit
        );
        Ok(())
    }
}
