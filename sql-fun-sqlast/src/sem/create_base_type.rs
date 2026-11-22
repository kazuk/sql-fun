use crate::{
    ast::{BaseTypeOption, CreateType, Expr, Literal, SyntaxValue},
    context::ParseContext,
    names::FullName,
    sem::{
        AnalysisError, AnalysisProblem, AstAndContextPair, SemAst, TypeDefinition, TypeReference,
        pg_type::TypeCategory,
    },
};

pub(super) fn analyze(
    ast: &CreateType,
    context: &mut impl ParseContext,
) -> Result<SemAst, AnalysisError> {
    let name = context.apply_default_schema(&FullName::from_ast_name(ast.name())?)?;
    let reference = TypeReference::concrete_type_ref(&name, false);
    let Some(options) = ast.definition() else {
        return if context.permits_type_creation(&name)? {
            Ok(SemAst::CreateBaseType(TypeDefinition::Shell(reference)))
        } else {
            Ok(SemAst::rejected_base_type(
                name,
                false,
                invalid("type already exists"),
            ))
        };
    };
    if context
        .get_type_impl(&name)
        .is_some_and(|definition| !definition.is_shell())
    {
        context.permits_type_creation(&name)?;
        return Ok(SemAst::rejected_base_type(
            name,
            true,
            invalid("type already exists"),
        ));
    }
    match validate_definition(options, &name, &reference, context) {
        Ok(definition) => Ok(SemAst::CreateBaseType(definition)),
        Err(failure) => {
            context.report_analysis_problem(AnalysisProblem::invalid_base_type(
                &(&name).into(),
                &failure.0,
            ))?;
            Ok(SemAst::rejected_base_type(name, true, failure))
        }
    }
}

/// A shell or base declaration rejected during semantic analysis.
#[derive(Debug, Clone)]
pub struct RejectedBaseType {
    name: FullName,
    completion: bool,
    reason: String,
}

impl RejectedBaseType {
    /// The qualified name targeted by the rejected declaration.
    #[must_use]
    pub const fn name(&self) -> &FullName {
        &self.name
    }

    /// Whether the declaration attempted to complete a shell.
    #[must_use]
    pub const fn is_completion(&self) -> bool {
        self.completion
    }

    /// The reason the declaration could not be applied.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl SemAst {
    fn rejected_base_type(name: FullName, completion: bool, failure: InvalidDefinition) -> Self {
        Self::RejectedBaseType(RejectedBaseType {
            name,
            completion,
            reason: failure.0,
        })
    }
}

fn validate_definition(
    options: &[BaseTypeOption],
    name: &FullName,
    reference: &TypeReference,
    context: &impl ParseContext,
) -> Result<TypeDefinition, InvalidDefinition> {
    if context.get_type_impl(name).is_none() {
        return Err(invalid("a shell declaration is required"));
    }
    let mut support = std::collections::BTreeMap::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut category = TypeCategory::try_from('U').expect("valid default category");
    let mut collatable = false;
    let mut layout = TypeLayout::default();
    for option in options {
        let option = option.syntax();
        let role = match option.name().as_str() {
            "analyse" => "analyze",
            name => name,
        };
        if !seen.insert(role) {
            return Err(invalid(&format!("duplicate {role} option")));
        }
        let value = option.value().as_ref();
        match role {
            "input" | "output" | "receive" | "send" | "typmod_in" | "typmod_out" | "analyze"
            | "subscript" => insert_support_function(&mut support, role, value)?,
            "like" => inherit_layout(context, value)?,
            "element" => validate_element(context, value)?,
            "category" => {
                category = TypeCategory::try_from(option_text(value)?.as_str()).map_err(invalid)?
            }
            "collatable" => collatable = option_boolean(value)?,
            "preferred" => validate_preferred(value)?,
            "delimiter" | "default" => validate_text(value)?,
            "internallength" => layout.length = parse_length(value)?,
            "alignment" => layout.alignment = parse_alignment(value)?,
            "storage" => layout.plain_storage = parse_storage(value)?,
            "passedbyvalue" => layout.by_value = option_boolean(value)?,
            unknown => return Err(invalid(&format!("unrecognized {unknown} option"))),
        }
    }
    layout.validate()?;
    for required in ["input", "output"] {
        if !support.contains_key(required) {
            return Err(invalid(&format!("{required} function is required")));
        }
    }
    for (role, routine) in support {
        check_support_function(context, reference, role, &routine)?;
    }
    Ok(TypeDefinition::base(name, category, collatable))
}

pub(super) fn apply<T: ParseContext>(
    ast: &CreateType,
    mut context: T,
) -> Result<AstAndContextPair<T>, AnalysisError> {
    let statement = analyze(ast, &mut context)?;
    if let SemAst::CreateBaseType(definition) = &statement {
        let name = definition.full_name().expect("named definition");
        context.insert_shell_or_base_type(name, definition);
    }
    Ok(AstAndContextPair::new(statement, context))
}

#[cfg(test)]
mod application_tests {
    use crate::{
        context::{BaseParseContext, TypeInfoRead},
        names::FullName,
        sem::{AnalysisProblem, SemAst},
        test_helpers::ownership::{apply_ownership_sql, ownership_context},
    };

    #[rstest::rstest]
    #[case(
        "INTERNALLENGTH = 4, STORAGE = extended",
        "fixed-size types must have storage PLAIN"
    )]
    #[case(
        "CATEGORY = ''",
        "type category must begin with a printable ASCII character"
    )]
    fn rejected_completion_retains_shell_and_allows_next_statement(
        #[case] options: &str,
        #[case] expected: &str,
    ) -> testresult::TestResult {
        let context =
            apply_ownership_sql(ownership_context(), "CREATE TYPE public.demo")?.into_context();
        let attempt =
            apply_ownership_sql(context, &format!("CREATE TYPE public.demo ({options})"))?;
        let name = FullName::with_schema("public", "demo");
        assert!(matches!(
            attempt.ast(),
            SemAst::RejectedBaseType(rejected)
                if rejected.name() == &name && rejected.is_completion() && rejected.reason() == expected
        ));
        let context = attempt.into_context();
        assert!(
            context
                .problems()
                .iter()
                .any(|problem| { matches!(problem, AnalysisProblem::InvalidBaseType(_)) })
        );
        assert!(
            context
                .get_type_impl(&name)
                .is_some_and(|definition| definition.is_shell())
        );
        assert!(context.get_type(&name).is_none());
        let next = apply_ownership_sql(context, "CREATE TYPE public.other")?;
        assert!(matches!(next.ast(), SemAst::CreateBaseType(_)));
        Ok(())
    }
}

struct TypeLayout {
    length: i16,
    alignment: i16,
    by_value: bool,
    plain_storage: bool,
}

impl Default for TypeLayout {
    fn default() -> Self {
        Self {
            length: -1,
            alignment: 4,
            by_value: false,
            plain_storage: true,
        }
    }
}

impl TypeLayout {
    fn validate(&self) -> Result<(), InvalidDefinition> {
        if self.by_value {
            if !matches!(self.length, 1 | 2 | 4 | 8) || self.length != self.alignment {
                return Err(invalid(
                    "invalid length or alignment for a passed-by-value type",
                ));
            }
        } else if (self.length == -1 && self.alignment < 4)
            || (self.length == -2 && self.alignment != 1)
        {
            return Err(invalid("invalid alignment for a variable-length type"));
        }
        if self.length != -1 && !self.plain_storage {
            return Err(invalid("fixed-size types must have storage PLAIN"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod layout_tests {
    use crate::sem::create_base_type::TypeLayout;

    #[rstest::rstest]
    #[case(4, 4, true, true, true)]
    #[case(4, 8, true, true, false)]
    #[case(-1, 4, true, true, false)]
    #[case(-1, 2, false, true, false)]
    #[case(-2, 1, false, true, true)]
    #[case(-2, 4, false, true, false)]
    #[case(4, 4, false, false, false)]
    #[case(-1, 8, false, false, true)]
    fn validates_layout_combinations(
        #[case] length: i16,
        #[case] alignment: i16,
        #[case] by_value: bool,
        #[case] plain_storage: bool,
        #[case] valid: bool,
    ) {
        let layout = TypeLayout {
            length,
            alignment,
            by_value,
            plain_storage,
        };
        assert_eq!(layout.validate().is_ok(), valid);
    }
}

fn insert_support_function<'a>(
    support: &mut std::collections::BTreeMap<&'a str, FullName>,
    role: &'a str,
    value: Option<&SyntaxValue>,
) -> Result<(), InvalidDefinition> {
    support.insert(role, option_name(value)?);
    Ok(())
}

fn inherit_layout(
    context: &impl ParseContext,
    value: Option<&SyntaxValue>,
) -> Result<(), InvalidDefinition> {
    let target = option_name(value)?;
    if context.get_type(&target).is_none() {
        return Err(invalid(&format!("type {target} does not exist")));
    }
    todo!("validate inherited PostgreSQL base type layout (#1211)")
}

fn validate_element(
    context: &impl ParseContext,
    value: Option<&SyntaxValue>,
) -> Result<(), InvalidDefinition> {
    let target = option_name(value)?;
    let Some(definition) = context.get_type(&target) else {
        return Err(invalid(&format!("type {target} does not exist")));
    };
    if definition.is_builtin()
        && definition
            .intrinsic_category()
            .is_some_and(|category| category.code() == 'P')
    {
        return Err(invalid("a pseudo-type cannot be an element type"));
    }
    Ok(())
}

fn option_text(value: Option<&SyntaxValue>) -> Result<String, InvalidDefinition> {
    match value {
        Some(SyntaxValue::Type(ty)) => Ok(ty.name().parts().join(".")),
        Some(SyntaxValue::Expression(Expr::Column(name))) => Ok(name.parts().join(".")),
        Some(SyntaxValue::Expression(Expr::Literal(
            Literal::String(value) | Literal::Keyword(value) | Literal::Numeric(value),
        ))) => Ok(value.clone()),
        Some(SyntaxValue::Expression(Expr::Literal(Literal::Integer(value)))) => {
            Ok(value.to_string())
        }
        _ => Err(invalid("expected an option value")),
    }
}

fn validate_text(value: Option<&SyntaxValue>) -> Result<(), InvalidDefinition> {
    option_text(value).map(drop)
}

fn parse_length(value: Option<&SyntaxValue>) -> Result<i16, InvalidDefinition> {
    match value {
        Some(SyntaxValue::Expression(Expr::Literal(Literal::Integer(length)))) => {
            let length = i16::try_from(*length).map_err(|_| invalid("invalid internal length"))?;
            if length > 0 || matches!(length, -1 | -2) {
                Ok(length)
            } else {
                Err(invalid("invalid internal length"))
            }
        }
        _ if option_text(value)?.eq_ignore_ascii_case("variable") => Ok(-1),
        _ => Err(invalid("invalid internal length")),
    }
}

fn parse_alignment(value: Option<&SyntaxValue>) -> Result<i16, InvalidDefinition> {
    match option_text(value)?.to_ascii_lowercase().as_str() {
        "char" | "pg_catalog.bpchar" => Ok(1),
        "int2" | "pg_catalog.int2" => Ok(2),
        "int4" | "pg_catalog.int4" => Ok(4),
        "double" | "float8" | "pg_catalog.float8" => Ok(8),
        _ => Err(invalid("unrecognized alignment")),
    }
}

fn parse_storage(value: Option<&SyntaxValue>) -> Result<bool, InvalidDefinition> {
    match option_text(value)?.to_ascii_lowercase().as_str() {
        "plain" => Ok(true),
        "external" | "extended" | "main" => Ok(false),
        _ => Err(invalid("unrecognized storage")),
    }
}

fn validate_preferred(value: Option<&SyntaxValue>) -> Result<(), InvalidDefinition> {
    option_boolean(value).map(drop)
}

fn option_boolean(value: Option<&SyntaxValue>) -> Result<bool, InvalidDefinition> {
    match value {
        None => Ok(true),
        Some(SyntaxValue::Expression(Expr::Literal(Literal::Boolean(value)))) => Ok(*value),
        Some(SyntaxValue::Expression(Expr::Literal(Literal::Integer(0)))) => Ok(false),
        Some(SyntaxValue::Expression(Expr::Literal(Literal::Integer(1)))) => Ok(true),
        _ => parse_boolean(&option_text(value)?),
    }
}

fn option_name(value: Option<&SyntaxValue>) -> Result<FullName, InvalidDefinition> {
    match value {
        Some(SyntaxValue::Type(ty)) => {
            FullName::from_ast_name(ty.name()).map_err(|_| invalid("invalid qualified name"))
        }
        Some(SyntaxValue::Expression(Expr::Column(name))) => {
            FullName::from_ast_name(name).map_err(|_| invalid("invalid qualified name"))
        }
        _ => Err(invalid("expected a qualified name or identifier")),
    }
}

fn check_support_function(
    context: &impl ParseContext,
    base: &TypeReference,
    role: &str,
    routine: &FullName,
) -> Result<(), InvalidDefinition> {
    let builtin =
        |name| TypeReference::concrete_type_ref(&FullName::with_schema("pg_catalog", name), false);
    let (args, result, alternate) = match role {
        "input" => (vec![builtin("cstring")], base.clone(), true),
        "output" => (vec![base.clone()], builtin("cstring"), false),
        "receive" => (vec![builtin("internal")], base.clone(), true),
        "send" => (vec![base.clone()], builtin("bytea"), false),
        "typmod_in" => (
            vec![TypeReference::concrete_type_ref(
                &FullName::with_schema("pg_catalog", "cstring"),
                true,
            )],
            builtin("int4"),
            false,
        ),
        "typmod_out" => (vec![builtin("int4")], builtin("cstring"), false),
        "analyze" => (vec![builtin("internal")], builtin("bool"), false),
        "subscript" => (vec![builtin("internal")], builtin("internal"), false),
        _ => unreachable!("only recognized support roles reach signature lookup"),
    };
    let mut args = args.into_iter().map(Some).collect::<Vec<_>>();
    let exact = match context.find_exact_function(routine, &args) {
        None if alternate => {
            args.extend([Some(builtin("oid")), Some(builtin("int4"))]);
            context.find_exact_function(routine, &args)
        }
        found => found,
    };
    if exact.is_some_and(|function| function.overload().ret_type().as_ref() == Some(&result)) {
        Ok(())
    } else {
        Err(invalid(&format!("invalid {role} function {routine}")))
    }
}

struct InvalidDefinition(String);

fn invalid(message: &str) -> InvalidDefinition {
    InvalidDefinition(message.to_owned())
}

fn parse_boolean(value: &str) -> Result<bool, InvalidDefinition> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "on" => Ok(true),
        "false" | "off" => Ok(false),
        _ => Err(invalid("COLLATABLE requires a boolean")),
    }
}
