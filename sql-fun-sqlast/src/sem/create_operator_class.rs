//! PostgreSQL operator class declaration and atomic catalog application.

use crate::{
    ast::create_operator_class::{CreateOperatorClass as Syntax, OperatorClassMember},
    context::ParseContext,
    names::FullName,
    sem::{
        AnalysisError, OperatorClassDefinition, OperatorFamily, OperatorFamilyKey, TypeReference,
        operator_family::{creation_name, member::resolve_family_member, MemberOwner, report_family_problem, require_index_method},
    },
};

/// An analyzed operator class declaration.
#[derive(Clone, Debug)]
pub struct CreateOperatorClass {
    change: Option<(OperatorClassDefinition, OperatorFamily)>,
}

impl CreateOperatorClass {
    /// Definition published by an applicable declaration.
    pub fn definition(&self) -> Option<&OperatorClassDefinition> {
        self.change.as_ref().map(|(definition, _)| definition)
    }

    pub(crate) fn analyze(syntax: &Syntax, context: &mut impl ParseContext) -> Result<Self, AnalysisError> {
        Ok(Self { change: Self::prepare_change(syntax, context)? })
    }

    pub(crate) fn apply<C: ParseContext>(&self, context: C) -> C {
        if let Some((class, family)) = &self.change {
            context.publish_operator_class(class, family)
        } else {
            context
        }
    }

    fn prepare_change(
        syntax: &Syntax,
        context: &mut impl ParseContext,
    ) -> Result<Option<(OperatorClassDefinition, OperatorFamily)>, AnalysisError> {
        let method = syntax.access_method();
        if !require_index_method(method, context)? {
            return Ok(None);
        }
        let Some(name) = creation_name(syntax.name(), context)? else {
            return Ok(None);
        };
        if context.operator_class_definition(&name, method).is_some() {
            report_family_problem(context, format!("operator class {name} already exists for access method {method:?}"))?;
            return Ok(None);
        }
        let Some(input) = context.canonicalize_type_ref(&TypeReference::from_ast(syntax.input_type())?) else {
            report_family_problem(context, "operator class input type does not exist")?;
            return Ok(None);
        };
        if syntax.is_default() && context.has_default_operator_class(method, &input) {
            report_family_problem(context, format!("a default operator class already exists for {input} using {method}"))?;
            return Ok(None);
        }
        let Some(mut family) = Self::family(syntax, &name, context)? else {
            return Ok(None);
        };
        let Some(storage) = Self::members(syntax, &name, &input, &mut family, context)? else {
            return Ok(None);
        };
        let class = OperatorClassDefinition::from_declaration(&name, &family.key, &input, syntax.is_default(), storage.as_ref());
        Ok(Some((class, family)))
    }

    fn family(
        syntax: &Syntax,
        name: &FullName,
        context: &mut impl ParseContext,
    ) -> Result<Option<OperatorFamily>, AnalysisError> {
        let method = syntax.access_method();
        if let Some(family) = syntax.family() {
            let Some(key) = OperatorFamilyKey::resolve(family, method, context)? else {
                let family = FullName::from_ast_name(family)?;
                report_family_problem(context, format!("operator family {family} does not exist for access method {method:?}"))?;
                return Ok(None);
            };
            return Ok(context.operator_family(&key).cloned());
        }
        let key = OperatorFamilyKey::new(name.clone(), method);
        Ok(Some(context.operator_family(&key).cloned().unwrap_or_else(|| OperatorFamily::new(key))))
    }

    fn members(
        syntax: &Syntax,
        name: &FullName,
        input: &TypeReference,
        family: &mut OperatorFamily,
        context: &mut impl ParseContext,
    ) -> Result<Option<Option<TypeReference>>, AnalysisError> {
        let method = syntax.access_method();
        let mut storage = None;
        for member in syntax.members() {
            if let OperatorClassMember::Storage(typ) = member {
                if storage.is_some() {
                    report_family_problem(context, "storage type specified more than once")?;
                    return Ok(None);
                }
                let Some(typ) = context.canonicalize_type_ref(&TypeReference::from_ast(typ)?) else {
                    report_family_problem(context, "operator class storage type does not exist")?;
                    return Ok(None);
                };
                // These builtin methods require identical input and storage types.
                if &typ != input && matches!(method, "btree" | "hash") {
                    report_family_problem(context, format!("storage type cannot be different from data type for access method {method:?}"))?;
                    return Ok(None);
                }
                storage = Some(typ);
                continue;
            }
            let Some((slot, member)) = resolve_family_member(member, &family.key, Some(input), MemberOwner::Class(name.clone()), context)? else {
                return Ok(None);
            };
            if let Err(message) = family.insert(slot, member) {
                report_family_problem(context, message)?;
                return Ok(None);
            }
        }
        Ok(Some(storage.filter(|storage| storage != input)))
    }
}
