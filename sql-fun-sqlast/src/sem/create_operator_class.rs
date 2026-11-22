//! PostgreSQL operator class declaration and atomic catalog application.

use crate::{
    ast::{
        TypeName,
        create_operator_class::{CreateOperatorClass as Syntax, OperatorClassMember},
    },
    context::ParseContext,
    names::FullName,
    sem::{
        AccessMethodKind, AnalysisError, OperatorClassDefinition, OperatorFamily,
        OperatorFamilyKey, TypeReference,
        operator_family::{
            FamilyMember, FamilySlot, MemberOwner, creation_name, member::resolve_family_member,
            report_family_problem, require_index_method,
        },
    },
};

/// An analyzed operator class declaration, including rejected declarations.
#[derive(Clone, Debug)]
pub struct CreateOperatorClass {
    syntax: Syntax,
    name: Option<FullName>,
    method_kind: Option<AccessMethodKind>,
    input_type: Option<TypeReference>,
    family: Option<OperatorFamilyKey>,
    members: Vec<MemberAnalysis>,
    failures: Vec<ClassFailure>,
    change: Option<(OperatorClassDefinition, OperatorFamily)>,
}

/// A declaration outcome that prevents catalog publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassFailure {
    /// The specified access method is missing or is a table method.
    AccessMethod,
    /// The creation namespace could not be selected.
    Namespace,
    /// The input type could not be resolved.
    InputType,
    /// The explicitly named family could not be resolved.
    Family,
    /// The class identity is already occupied.
    DuplicateClass,
    /// Another class is already the default for this method and input type.
    DuplicateDefault,
    /// A member binding or validation failed.
    Member,
}

#[derive(Clone, Debug)]
enum MemberAnalysis {
    Blocked,
    BindingFailed,
    Storage {
        typ: TypeReference,
        accepted: bool,
    },
    Bound {
        binding: Box<(FamilySlot, FamilyMember)>,
        accepted: bool,
    },
}

impl CreateOperatorClass {
    /// Definition published by an applicable declaration.
    pub fn definition(&self) -> Option<&OperatorClassDefinition> {
        self.change.as_ref().map(|(definition, _)| definition)
    }

    /// Name as written in the declaration.
    pub fn requested_name(&self) -> &[String] {
        self.syntax.name().parts()
    }

    /// Method name as written in the declaration.
    pub fn requested_method(&self) -> &str {
        self.syntax.access_method()
    }

    /// Method kind when the requested method resolved.
    pub const fn method_kind(&self) -> Option<AccessMethodKind> {
        self.method_kind
    }

    /// Exact creation identity when the namespace resolved.
    pub const fn name(&self) -> Option<&FullName> {
        self.name.as_ref()
    }

    /// Exact input type when it resolved.
    pub const fn input_type(&self) -> Option<&TypeReference> {
        self.input_type.as_ref()
    }

    /// Exact family selected by the declaration.
    pub const fn family_key(&self) -> Option<&OperatorFamilyKey> {
        self.family.as_ref()
    }

    /// Whether the validated declaration can publish a catalog change.
    pub const fn is_applicable(&self) -> bool {
        self.change.is_some()
    }

    /// Reasons the declaration could not be published.
    pub fn failures(&self) -> &[ClassFailure] {
        &self.failures
    }

    /// Applicability of a clause in declaration order, absent if binding was blocked.
    pub fn member_applicability(&self, index: usize) -> Option<bool> {
        match self.members.get(index)? {
            MemberAnalysis::Blocked => None,
            MemberAnalysis::BindingFailed => Some(false),
            MemberAnalysis::Storage { accepted, .. } | MemberAnalysis::Bound { accepted, .. } => {
                Some(*accepted)
            }
        }
    }

    pub(crate) fn analyze(
        syntax: &Syntax,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let method = syntax.access_method();
        let mut statement = Self {
            syntax: syntax.clone(),
            name: creation_name(syntax.name(), context)?,
            method_kind: context.access_method(method).map(|method| method.kind()),
            input_type: context
                .canonicalize_type_ref(&TypeReference::from_ast(syntax.input_type())?),
            family: None,
            members: vec![MemberAnalysis::Blocked; syntax.members().len()],
            failures: Vec::new(),
            change: None,
        };
        if !require_index_method(method, context)? {
            statement.failures.push(ClassFailure::AccessMethod);
        }
        if statement.input_type.is_none() {
            report_family_problem(context, "operator class input type does not exist")?;
            statement.failures.push(ClassFailure::InputType);
        }
        let Some(name) = statement.name.clone() else {
            statement.failures.push(ClassFailure::Namespace);
            return Ok(statement);
        };
        if context.operator_class_definition(&name, method).is_some() {
            report_family_problem(
                context,
                format!("operator class {name} already exists for access method {method:?}"),
            )?;
            statement.failures.push(ClassFailure::DuplicateClass);
        }
        let Some(mut family) = Self::family(syntax, &name, context)? else {
            statement.failures.push(ClassFailure::Family);
            return Ok(statement);
        };
        statement.family = Some(family.key.clone());
        let Some(input) = statement.input_type.clone() else {
            return Ok(statement);
        };
        if syntax.is_default() && context.has_default_operator_class(method, &input) {
            report_family_problem(
                context,
                format!("a default operator class already exists for {input} using {method}"),
            )?;
            statement.failures.push(ClassFailure::DuplicateDefault);
        }
        statement.members = Self::analyze_members(syntax, &name, &input, &family, context)?;
        if statement.members.iter().any(|member| !member.accepted()) {
            statement.failures.push(ClassFailure::Member);
        }
        if statement.failures.is_empty() {
            let mut storage = None;
            for member in &statement.members {
                match member {
                    MemberAnalysis::Bound { binding, .. } => {
                        let (slot, member) = binding.as_ref();
                        family.members.insert(slot.clone(), member.clone());
                    }
                    MemberAnalysis::Storage { typ, .. } if typ != &input => storage = Some(typ),
                    _ => {}
                }
            }
            let class = OperatorClassDefinition::from_declaration(
                &name,
                &family.key,
                &input,
                syntax.is_default(),
                storage,
            );
            statement.change = Some((class, family));
        }
        Ok(statement)
    }

    pub(crate) fn apply<C: ParseContext>(&self, context: C) -> C {
        if let Some((class, family)) = &self.change {
            context.publish_operator_class(class, family)
        } else {
            context
        }
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
                report_family_problem(
                    context,
                    format!("operator family {family} does not exist for access method {method:?}"),
                )?;
                return Ok(None);
            };
            return Ok(context.operator_family(&key).cloned());
        }
        let key = OperatorFamilyKey::new(name.clone(), method);
        Ok(Some(
            context
                .operator_family(&key)
                .cloned()
                .unwrap_or_else(|| OperatorFamily::new(key)),
        ))
    }

    fn analyze_members(
        syntax: &Syntax,
        name: &FullName,
        input: &TypeReference,
        family: &OperatorFamily,
        context: &mut impl ParseContext,
    ) -> Result<Vec<MemberAnalysis>, AnalysisError> {
        let mut staged = family.clone();
        let mut storage_seen = false;
        let mut outcomes = Vec::with_capacity(syntax.members().len());
        for member in syntax.members() {
            if let OperatorClassMember::Storage(typ) = member {
                outcomes.push(Self::analyze_storage(
                    typ,
                    syntax.access_method(),
                    input,
                    storage_seen,
                    context,
                )?);
                storage_seen = true;
                continue;
            }
            let Some((slot, member)) = resolve_family_member(
                member,
                &family.key,
                Some(input),
                MemberOwner::Class(name.clone()),
                context,
            )?
            else {
                outcomes.push(MemberAnalysis::BindingFailed);
                continue;
            };
            let accepted = match staged.insert(slot.clone(), member.clone()) {
                Ok(()) => true,
                Err(message) => {
                    report_family_problem(context, message)?;
                    false
                }
            };
            outcomes.push(MemberAnalysis::Bound {
                binding: Box::new((slot, member)),
                accepted,
            });
        }
        Ok(outcomes)
    }

    fn analyze_storage(
        typ: &TypeName,
        method: &str,
        input: &TypeReference,
        duplicate: bool,
        context: &mut impl ParseContext,
    ) -> Result<MemberAnalysis, AnalysisError> {
        let Some(typ) = context.canonicalize_type_ref(&TypeReference::from_ast(typ)?) else {
            report_family_problem(context, "operator class storage type does not exist")?;
            return Ok(MemberAnalysis::BindingFailed);
        };
        let accepted = if duplicate {
            report_family_problem(context, "storage type specified more than once")?;
            false
        } else if &typ != input && matches!(method, "btree" | "hash") {
            report_family_problem(
                context,
                format!(
                    "storage type cannot be different from data type for access method {method:?}"
                ),
            )?;
            false
        } else {
            true
        };
        Ok(MemberAnalysis::Storage { typ, accepted })
    }
}

impl MemberAnalysis {
    const fn accepted(&self) -> bool {
        match self {
            Self::Blocked | Self::BindingFailed => false,
            Self::Storage { accepted, .. } | Self::Bound { accepted, .. } => *accepted,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        context::{ConflictInfoRead, OperatorFamilyInfoRead, ParseContext},
        names::{FullName, SchemaName},
        sem::{OperatorFamilyKey, PgBuiltInType},
        test_helpers::{conflict_masters::Masters, schema_creation},
    };

    #[test]
    fn invalid_class_does_not_publish_its_implicit_family() -> testresult::TestResult {
        let mut builtin = Masters::new().context();
        builtin.setup_type(PgBuiltInType::int4());
        let mut child = schema_creation::child(builtin.freeze());
        child.set_search_path(&[SchemaName::from("public")]);
        let child = schema_creation::apply(
            child,
            "CREATE ACCESS METHOD project_method TYPE INDEX HANDLER handler;
             CREATE OPERATOR CLASS broken FOR TYPE integer USING project_method
                 AS STORAGE integer, STORAGE integer;
             CREATE OPERATOR CLASS accepted FOR TYPE integer USING project_method
                 AS STORAGE integer;",
        )?;
        let frozen = child.freeze();
        let broken = FullName::with_schema("public", "broken");
        let accepted = FullName::with_schema("public", "accepted");
        assert!(
            frozen
                .operator_class_definition(&broken, "project_method")
                .is_none()
        );
        assert!(
            frozen
                .operator_family(&OperatorFamilyKey::new(broken, "project_method"))
                .is_none()
        );
        assert!(
            frozen
                .operator_class_definition(&accepted, "project_method")
                .is_some()
        );
        assert!(
            frozen
                .operator_family(&OperatorFamilyKey::new(accepted, "project_method"))
                .is_some()
        );
        Ok(())
    }

    #[test]
    fn rejected_member_keeps_the_header_and_later_member_outcomes() -> testresult::TestResult {
        use crate::{
            sem::{ClassFailure, SemAst},
            test_helpers::ownership::apply_ownership_sql,
        };

        let mut builtin = Masters::new().context();
        builtin.setup_type(PgBuiltInType::int4());
        let mut context = schema_creation::child(builtin.freeze());
        context.set_search_path(&[SchemaName::from("public")]);
        let context = schema_creation::apply(
            context,
            "CREATE ACCESS METHOD project_method TYPE INDEX HANDLER handler;
             CREATE OPERATOR FAMILY project_family USING project_method;",
        )?;
        let result = apply_ownership_sql(
            context,
            "CREATE OPERATOR CLASS rejected FOR TYPE integer USING project_method
             FAMILY project_family AS FUNCTION 2 missing(integer), STORAGE integer",
        )?;
        let SemAst::CreateOperatorClass(class) = result.ast() else {
            panic!("expected operator class");
        };
        let name = FullName::with_schema("public", "rejected");
        assert_eq!(class.name(), Some(&name));
        assert_eq!(class.input_type(), Some(&PgBuiltInType::int4()));
        assert_eq!(
            class.family_key(),
            Some(&OperatorFamilyKey::new(
                FullName::with_schema("public", "project_family"),
                "project_method"
            ))
        );
        assert_eq!(class.failures(), [ClassFailure::Member]);
        assert_eq!(class.member_applicability(0), Some(false));
        assert_eq!(class.member_applicability(1), Some(true));
        assert!(!class.is_applicable());
        assert!(class.definition().is_none());
        let context = result.into_context();
        assert_eq!(
            context.problems()[0].code(),
            "invalid_operator_family_declaration"
        );
        assert!(
            context
                .operator_class_definition(&name, "project_method")
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn conflict_lookup_resolves_class_in_frozen_parent() -> testresult::TestResult {
        let mut builtin = Masters::new().context();
        builtin.setup_type(PgBuiltInType::int4());
        let mut context = schema_creation::child(builtin.freeze());
        context.set_search_path(&[SchemaName::from("public")]);
        let frozen = schema_creation::apply(
            context,
            "CREATE ACCESS METHOD btree TYPE INDEX HANDLER handler;
             CREATE OPERATOR CLASS schema_ops FOR TYPE integer USING btree AS STORAGE integer;",
        )?
        .freeze();
        let requested = FullName::from_local_name("schema_ops");
        assert_eq!(
            frozen
                .lookup_operator_class(&requested, "btree")
                .resolved_name(),
            Some(&FullName::with_schema("public", "schema_ops"))
        );
        Ok(())
    }
}
