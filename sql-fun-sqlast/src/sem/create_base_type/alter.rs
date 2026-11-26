use ::sql_fun_core::analysis_problem::StringSpan;

use crate::{
    ast::{AlterTypeSettings, BaseTypeOption, Expr, Literal, SyntaxValue},
    context::ParseContext,
    names::FullName,
    sem::{
        AnalysisError, AnalysisProblem, AstAndContextPair, ExactFunction, SemAst, TypeDefinition,
        TypeReference, TypeStorage,
        create_base_type::{InvalidDefinition, SupportFunctions, invalid},
    },
};

/// The target, settings and applicability of an ALTER TYPE SET statement.
#[derive(Clone, Debug)]
pub struct BaseTypeSettingsChange {
    name: FullName,
    target: Option<TypeReference>,
    target_error: Option<&'static str>,
    settings: Vec<BaseTypeSetting>,
}

impl BaseTypeSettingsChange {
    /// The resolved catalog key, or the requested name when lookup failed.
    #[must_use]
    pub const fn name(&self) -> &FullName {
        &self.name
    }

    /// The resolved nominal type identity, when available.
    #[must_use]
    pub const fn target(&self) -> Option<&TypeReference> {
        self.target.as_ref()
    }

    /// The reason target lookup or eligibility failed.
    #[must_use]
    pub const fn target_error(&self) -> Option<&'static str> {
        self.target_error
    }

    /// Settings in statement order, including any rejected values.
    #[must_use]
    pub fn settings(&self) -> &[BaseTypeSetting] {
        &self.settings
    }

    /// Whether the analyzed change can be applied.
    #[must_use]
    pub fn applicable(&self) -> bool {
        self.target_error.is_none()
            && self
                .settings
                .iter()
                .all(|setting| setting.error().is_none())
    }

    fn apply(&self, context: &mut impl ParseContext) {
        if !self.applicable() {
            return;
        }
        let storage = self.settings.iter().rev().find_map(|setting| {
            if let BaseTypeSettingValue::Storage(value) = setting.value {
                Some(value)
            } else {
                None
            }
        });
        if let Some(storage) = storage {
            let definition = context
                .get_type_impl(&self.name)
                .expect("resolved target in the same catalog")
                .clone()
                .with_storage(Some(storage));
            context.replace_type_settings(&self.name, &definition);
        }
    }
}

/// One analyzed base-type option.
#[derive(Clone, Debug)]
pub struct BaseTypeSetting {
    name: String,
    value: BaseTypeSettingValue,
    span: Option<StringSpan>,
}

impl BaseTypeSetting {
    /// The PostgreSQL option name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The validation and resolution outcome.
    #[must_use]
    pub const fn value(&self) -> &BaseTypeSettingValue {
        &self.value
    }

    fn analyze(
        context: &impl ParseContext,
        definition: &TypeDefinition,
        option: &BaseTypeOption,
    ) -> Self {
        Self {
            name: option.syntax().name().clone(),
            value: match validate_option(context, definition, option) {
                Ok(value) => value,
                Err(failure) => BaseTypeSettingValue::Invalid(failure.0),
            },
            span: option
                .location()
                .map(|span| StringSpan::new(span.offset(), span.len())),
        }
    }

    fn error(&self) -> Option<String> {
        match &self.value {
            BaseTypeSettingValue::Invalid(reason) => Some(reason.clone()),
            BaseTypeSettingValue::InvalidRoutine(name, error) => {
                Some(error.message(&self.name, name))
            }
            _ => None,
        }
    }
}

/// The outcome of interpreting one base-type option.
#[derive(Clone, Debug)]
pub enum BaseTypeSettingValue {
    /// A storage assignment that passed validation.
    Storage(TypeStorage),
    /// A validated support routine and its exact overload.
    Routine(Box<ExactFunction>),
    /// A support-function removal.
    RemoveRoutine,
    /// A routine reference that could not be used.
    InvalidRoutine(FullName, SupportFunctionError),
    /// An invalid option or argument.
    Invalid(String),
}

/// An unsuccessful support-function binding.
#[derive(Clone, Debug)]
pub enum SupportFunctionError {
    /// No visible routine has the required parameter types.
    NotFound,
    /// Both supported signatures are visible.
    Ambiguous,
    /// The matched routine returns the wrong type.
    InvalidReturn(Box<ExactFunction>),
    /// PostgreSQL reserves the matched handler for generated arrays.
    ReservedHandler(Box<ExactFunction>),
}

impl SupportFunctionError {
    pub(super) fn message(&self, role: &str, routine: &FullName) -> String {
        match self {
            Self::Ambiguous => format!("ambiguous {role} function {routine}"),
            Self::ReservedHandler(_) => {
                "the built-in array subscript handler is reserved for generated array types"
                    .to_owned()
            }
            Self::NotFound | Self::InvalidReturn(_) => format!("invalid {role} function {routine}"),
        }
    }
}

impl AlterTypeSettings {
    pub(in crate::sem) fn analyze(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<SemAst, AnalysisError> {
        let requested = FullName::from_ast_name(self.name())?;
        let mut change = BaseTypeSettingsChange {
            name: requested.clone(),
            target: None,
            target_error: Some("type does not exist"),
            settings: Vec::new(),
        };
        if let Some(name) = context.resolve_declared_type_name(&requested, true) {
            let definition = context
                .get_type_impl(&name)
                .expect("resolved type is in the effective catalog");
            change.name = name;
            change.target = definition.type_reference().cloned();
            change.target_error = match definition.alterable_base() {
                Some(true) => None,
                Some(false) => {
                    Some("target is not a completed base type eligible for ALTER TYPE SET")
                }
                None => Some("catalog does not establish whether this is a completed base type"),
            };
            if change.target_error.is_none() {
                change.settings = self
                    .options()
                    .iter()
                    .map(|option| BaseTypeSetting::analyze(context, definition, option))
                    .collect();
            }
        }
        let diagnostic = if let Some(reason) = change.target_error {
            Some((reason.to_owned(), None))
        } else {
            change
                .settings
                .iter()
                .find_map(|setting| Some((setting.error()?, setting.span)))
        };
        if let Some((reason, span)) = diagnostic {
            context.report_analysis_problem(AnalysisProblem::invalid_base_type_settings(
                &(&change.name).into(),
                &reason,
                span,
            ))?;
        }
        Ok(SemAst::AlterTypeSettings(Box::new(change)))
    }

    pub(in crate::sem) fn analyze_and_apply<T: ParseContext>(
        &self,
        mut context: T,
    ) -> Result<AstAndContextPair<T>, AnalysisError> {
        let statement = self.analyze(&mut context)?;
        if let SemAst::AlterTypeSettings(change) = &statement {
            change.apply(&mut context);
        }
        Ok(AstAndContextPair::new(statement, context))
    }
}

fn validate_option(
    context: &impl ParseContext,
    definition: &TypeDefinition,
    option: &BaseTypeOption,
) -> Result<BaseTypeSettingValue, InvalidDefinition> {
    // ALTER uses the literal option name. CREATE TYPE also accepts ANALYSE.
    let role = option.syntax().name().as_str();
    match role {
        "storage" => {
            if matches!(option.value(), Some(SyntaxValue::Type(ty)) if !ty.array_bounds().is_empty())
            {
                return Err(invalid("unrecognized storage"));
            }
            validate_storage(definition, option.storage()?).map(BaseTypeSettingValue::Storage)
        }
        "receive" | "send" | "typmod_in" | "typmod_out" | "analyze" | "subscript" => {
            // PostgreSQL's parser uses the same absent argument for NONE and a bare option.
            let Some(value) = option.value() else {
                return Ok(BaseTypeSettingValue::RemoveRoutine);
            };
            let routine =
                if let SyntaxValue::Expression(Expr::Literal(Literal::String(name))) = value {
                    FullName::from_local_name(name)
                } else {
                    option.qualified_name()?
                };
            let reference = definition
                .type_reference()
                .expect("eligible base type has a nominal identity");
            Ok(
                match SupportFunctions::resolve_support_function(
                    context, reference, role, &routine, true,
                ) {
                    Ok(function) => BaseTypeSettingValue::Routine(Box::new(function)),
                    Err(error) => BaseTypeSettingValue::InvalidRoutine(routine, error),
                },
            )
        }
        "input" | "output" | "internallength" | "passedbyvalue" | "alignment" | "like"
        | "category" | "preferred" | "default" | "element" | "delimiter" | "collatable" => {
            Err(invalid(&format!("type attribute {role} cannot be changed")))
        }
        _ => Err(invalid(&format!("unrecognized {role} option"))),
    }
}

fn validate_storage(
    definition: &TypeDefinition,
    requested: TypeStorage,
) -> Result<TypeStorage, InvalidDefinition> {
    if requested == TypeStorage::Plain {
        match definition.storage() {
            Some(TypeStorage::Plain) => {}
            Some(_) => return Err(invalid("cannot change type's storage to PLAIN")),
            None => {
                return Err(invalid(
                    "catalog does not establish the type's current storage",
                ));
            }
        }
    } else {
        match definition.physical_layout() {
            Some(layout) if layout.is_varlena() => {}
            Some(_) => return Err(invalid("fixed-size types must have storage PLAIN")),
            None => {
                return Err(invalid(
                    "catalog does not establish the type's physical layout",
                ));
            }
        }
    }
    Ok(requested)
}

#[cfg(test)]
mod catalog_transition_tests {
    use crate::{
        context::{TypeInfoRead, TypeInfoWrite},
        names::FullName,
        sem::{
            AnalysisProblem, BaseTypeSettingValue, SemAst, SupportFunctionError, TypeDefinition,
            pg_type::{PhysicalTypeLayout, TypeStorage},
        },
        test_helpers::ownership::{apply_ownership_sql, ownership_context},
    };

    #[test]
    fn updates_only_storage_and_rejects_partial_changes() -> testresult::TestResult {
        let mut context = ownership_context();
        let name = FullName::with_schema("public", "demo");
        let original = TypeDefinition::base(&name, 'U'.try_into()?, true)
            .with_physical_layout(PhysicalTypeLayout::new(-1, 'i', false))
            .with_storage(Some(TypeStorage::Plain));
        context.insert_shell_or_base_type(&name, &original);
        let rejected = apply_ownership_sql(
            context,
            "ALTER TYPE public.demo SET (STORAGE = extended, SEND = public.missing)",
        )?;
        assert!(matches!(rejected.ast(), SemAst::AlterTypeSettings(change)
            if !change.applicable()
                && change.target() == original.type_reference()
                && matches!(change.settings()[1].value(), BaseTypeSettingValue::InvalidRoutine(_, SupportFunctionError::NotFound))
        ));
        let context = rejected.into_context();
        assert_eq!(context.get_type_impl(&name), Some(&original));
        assert!(
            context
                .problems()
                .iter()
                .any(|problem| matches!(problem, AnalysisProblem::InvalidBaseType(_)))
        );

        let repeated = apply_ownership_sql(
            context,
            "ALTER TYPE public.demo SET (STORAGE = extended, STORAGE = plain)",
        )?;
        assert!(matches!(repeated.ast(), SemAst::AlterTypeSettings(change)
            if change.applicable()
                && matches!(change.settings()[0].value(), BaseTypeSettingValue::Storage(TypeStorage::Extended))
                && matches!(change.settings()[1].value(), BaseTypeSettingValue::Storage(TypeStorage::Plain))
        ));
        let context = repeated.into_context();
        assert_eq!(context.get_type_impl(&name), Some(&original));

        let accepted = apply_ownership_sql(
            context,
            "ALTER TYPE demo SET (STORAGE = extended, STORAGE = main, SEND = NONE)",
        )?;
        assert!(matches!(accepted.ast(), SemAst::AlterTypeSettings(change) if change.applicable()));
        let context = accepted.into_context();
        let expected = original.with_storage(Some(TypeStorage::Main));
        assert_eq!(context.get_type_impl(&name), Some(&expected));

        let rejected = apply_ownership_sql(context, "ALTER TYPE demo SET (STORAGE = plain)")?;
        assert!(
            matches!(rejected.ast(), SemAst::AlterTypeSettings(change) if !change.applicable())
        );
        assert_eq!(
            rejected.into_context().get_type_impl(&name),
            Some(&expected)
        );
        Ok(())
    }

    #[test]
    fn retains_exact_routine_binding_in_semantic_ast() -> testresult::TestResult {
        let mut context = ownership_context();
        let target = FullName::with_schema("public", "demo");
        let type_definition = TypeDefinition::base(&target, 'U'.try_into()?, false);
        context.insert_shell_or_base_type(&target, &type_definition);
        let bytea = FullName::with_schema("pg_catalog", "bytea");
        context.insert_shell_or_base_type(&bytea, &TypeDefinition::builtin(&bytea, Some(false)));

        let context = apply_ownership_sql(
            context,
            "CREATE FUNCTION public.demo_send(public.demo) RETURNS bytea LANGUAGE internal AS 'textsend'",
        )?.into_context();
        let result = apply_ownership_sql(context, "ALTER TYPE demo SET (SEND = demo_send)")?;
        assert!(matches!(result.ast(), SemAst::AlterTypeSettings(change)
            if change.applicable()
                && change.target() == type_definition.type_reference()
                && matches!(change.settings()[0].value(), BaseTypeSettingValue::Routine(function)
                    if function.name() == &FullName::with_schema("public", "demo_send"))
        ));
        assert_eq!(
            result.into_context().get_type_impl(&target),
            Some(&type_definition)
        );
        Ok(())
    }
}
