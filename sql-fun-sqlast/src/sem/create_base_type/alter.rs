use ::sql_fun_core::analysis_problem::StringSpan;

use crate::{
    ast::{AlterTypeSettings, BaseTypeOption, Expr, Literal, SyntaxValue},
    context::ParseContext,
    names::FullName,
    sem::{
        AnalysisError, AnalysisProblem, AstAndContextPair, SemAst, TypeDefinition,
        create_base_type::{InvalidDefinition, SupportFunctions, invalid},
        pg_type::TypeStorage,
    },
};

impl AlterTypeSettings {
    pub(in crate::sem) fn analyze(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<SemAst, AnalysisError> {
        Ok(self.analyze_change(context)?.0)
    }

    pub(in crate::sem) fn analyze_and_apply<T: ParseContext>(
        &self,
        mut context: T,
    ) -> Result<AstAndContextPair<T>, AnalysisError> {
        let (statement, change) = self.analyze_change(&mut context)?;
        if let Some((name, definition)) = change {
            context.replace_type_settings(&name, &definition);
        }
        Ok(AstAndContextPair::new(statement, context))
    }

    fn analyze_change(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<(SemAst, Option<(FullName, TypeDefinition)>), AnalysisError> {
        let target = FullName::from_ast_name(self.name())?;
        let Some(name) = context.resolve_declared_type_name(&target, true) else {
            let statement = reject(context, &target, invalid("type does not exist"), None)?;
            return Ok((statement, None));
        };
        let definition = context
            .get_type_impl(&name)
            .expect("resolved type is in the effective catalog");
        match definition.alterable_base() {
            Some(true) => {}
            Some(false) => {
                let reason =
                    invalid("target is not a completed base type eligible for ALTER TYPE SET");
                return Ok((reject(context, &name, reason, None)?, None));
            }
            None => {
                let reason =
                    invalid("catalog does not establish whether this is a completed base type");
                return Ok((reject(context, &name, reason, None)?, None));
            }
        }
        let mut storage = None;
        for option in self.options() {
            match validate_option(context, definition, option) {
                Ok(Some(value)) => storage = Some(value),
                Ok(None) => {}
                Err(failure) => {
                    let span = option
                        .location()
                        .map(|span| StringSpan::new(span.offset(), span.len()));
                    return Ok((reject(context, &name, failure, span)?, None));
                }
            }
        }
        let change = storage.map(|value| (name, definition.clone().with_storage(Some(value))));
        Ok((SemAst::AlterTypeSettings, change))
    }
}

fn reject(
    context: &mut impl ParseContext,
    name: &FullName,
    failure: InvalidDefinition,
    span: Option<StringSpan>,
) -> Result<SemAst, AnalysisError> {
    context.report_analysis_problem(AnalysisProblem::invalid_base_type_settings(
        &name.into(),
        &failure.0,
        span,
    ))?;
    Ok(SemAst::unresolved_statement())
}

fn validate_option(
    context: &impl ParseContext,
    definition: &TypeDefinition,
    option: &BaseTypeOption,
) -> Result<Option<TypeStorage>, InvalidDefinition> {
    // ALTER uses the literal option name. CREATE TYPE also accepts ANALYSE.
    let role = option.syntax().name().as_str();
    match role {
        "storage" => {
            if matches!(option.value(), Some(SyntaxValue::Type(ty)) if !ty.array_bounds().is_empty())
            {
                return Err(invalid("unrecognized storage"));
            }
            return validate_storage(definition, option.storage()?).map(Some);
        }
        "receive" | "send" | "typmod_in" | "typmod_out" | "analyze" | "subscript" => {
            // PostgreSQL's parser uses the same absent argument for NONE and a bare option.
            let Some(value) = option.value() else {
                return Ok(None);
            };
            let routine =
                if let SyntaxValue::Expression(Expr::Literal(Literal::String(name))) = value {
                    FullName::from_local_name(name)
                } else {
                    option.qualified_name()?
                };
            let reference = definition
                .type_reference()
                .ok_or_else(|| invalid("target has no nominal type identity"))?;
            SupportFunctions::check_signature(context, reference, role, &routine, true)?;
        }
        "input" | "output" | "internallength" | "passedbyvalue" | "alignment" | "like"
        | "category" | "preferred" | "default" | "element" | "delimiter" | "collatable" => {
            return Err(invalid(&format!("type attribute {role} cannot be changed")));
        }
        _ => return Err(invalid(&format!("unrecognized {role} option"))),
    }
    Ok(None)
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
            AnalysisProblem, SemAst, TypeDefinition,
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
        let context = rejected.into_context();
        assert_eq!(context.get_type_impl(&name), Some(&original));
        assert!(
            context
                .problems()
                .iter()
                .any(|problem| matches!(problem, AnalysisProblem::InvalidBaseType(_)))
        );

        let accepted = apply_ownership_sql(
            context,
            "ALTER TYPE demo SET (STORAGE = extended, STORAGE = main, SEND = NONE)",
        )?;
        assert!(matches!(accepted.ast(), SemAst::AlterTypeSettings));
        let context = accepted.into_context();
        let expected = original.with_storage(Some(TypeStorage::Main));
        assert_eq!(context.get_type_impl(&name), Some(&expected));

        let rejected = apply_ownership_sql(context, "ALTER TYPE demo SET (STORAGE = plain)")?;
        assert!(!matches!(rejected.ast(), SemAst::AlterTypeSettings));
        assert_eq!(
            rejected.into_context().get_type_impl(&name),
            Some(&expected)
        );
        Ok(())
    }
}
