//! PostgreSQL ALTER OPERATOR FAMILY ADD and DROP analysis.

use crate::{
    AstAndContextPair,
    ast::{
        create_operator_class::OperatorClassMember,
        operator_family::{
            AlterOperatorFamily as Syntax, AlterOperatorFamilyAction, OperatorFamilyDropMember,
            OperatorFamilyMemberKind,
        },
    },
    context::ParseContext,
    names::FullName,
    sem::{AnalysisError, SemAst},
};

use crate::sem::operator_family::{
    FamilySlot, MemberKind, MemberOwner, OperatorFamily, OperatorFamilyKey,
    member::{resolve_family_member, resolve_family_types, validate_slot_number},
    report_family_problem, require_index_method,
};

/// Analyzed PostgreSQL operator family membership change.
#[derive(Clone, Debug)]
pub struct AlterOperatorFamily {
    target: Option<OperatorFamilyKey>,
    updated: Option<OperatorFamily>,
}

impl AlterOperatorFamily {
    /// The resolved target family, if found.
    pub const fn target(&self) -> Option<&OperatorFamilyKey> {
        self.target.as_ref()
    }

    /// Whether all membership changes are valid and can apply together.
    pub const fn is_applicable(&self) -> bool {
        self.updated.is_some()
    }

    pub(crate) fn analyze(
        syntax: &Syntax,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let mut statement = Self {
            target: None,
            updated: None,
        };
        let method = syntax.access_method();
        if !require_index_method(method, context)? {
            return Ok(statement);
        }
        let family = match OperatorFamilyKey::resolve(syntax.name(), method, context)? {
            Some(key) => context.operator_family(&key).cloned(),
            None => None,
        };
        let Some(mut family) = family else {
            report_family_problem(
                context,
                format!(
                    "operator family {} does not exist for access method {method:?}",
                    FullName::from_ast_name(syntax.name())?
                ),
            )?;
            return Ok(statement);
        };
        statement.target = Some(family.key.clone());
        let valid = match syntax.action() {
            AlterOperatorFamilyAction::Add(members) => {
                Self::add_members(&mut family, members, context)?
            }
            AlterOperatorFamilyAction::Drop(members) => {
                Self::drop_members(&mut family, members, method, context)?
            }
        };
        if valid {
            statement.updated = Some(family);
        }
        Ok(statement)
    }

    fn add_members(
        family: &mut OperatorFamily,
        members: &[OperatorClassMember],
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        for member in members {
            let Some((slot, member)) =
                resolve_family_member(member, &family.key, None, MemberOwner::Loose, context)?
            else {
                return Ok(false);
            };
            if let Err(problem) = family.insert(slot, member) {
                report_family_problem(context, problem)?;
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn drop_members(
        family: &mut OperatorFamily,
        members: &[OperatorFamilyDropMember],
        method: &str,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        for member in members {
            let kind = match member.kind() {
                OperatorFamilyMemberKind::Operator => MemberKind::Operator,
                OperatorFamilyMemberKind::Function => MemberKind::Function,
            };
            let number = member.number();
            if !validate_slot_number(kind, number, method, context)? {
                return Ok(false);
            }
            let Some((left, right)) = resolve_family_types(member.operands(), context)? else {
                return Ok(false);
            };
            let slot = FamilySlot {
                kind,
                number,
                left,
                right,
            };
            if let Err(problem) = family.remove_loose_member(&slot) {
                report_family_problem(context, problem)?;
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(crate) fn analyze_and_apply<C: ParseContext>(
        syntax: &Syntax,
        mut context: C,
    ) -> Result<AstAndContextPair<C>, AnalysisError> {
        let statement = Self::analyze(syntax, &mut context)?;
        let context = if let Some(family) = &statement.updated {
            context.publish_operator_family(family)
        } else {
            context
        };
        Ok(AstAndContextPair::new(
            SemAst::AlterOperatorFamily(Box::new(statement)),
            context,
        ))
    }
}

impl OperatorFamily {
    fn remove_loose_member(&mut self, slot: &FamilySlot) -> Result<(), String> {
        let member = self.members.get(slot).ok_or_else(|| {
            format!(
                "{:?} slot {} for ({}, {}) is absent from the operator family",
                slot.kind, slot.number, slot.left, slot.right
            )
        })?;
        match &member.owner {
            MemberOwner::Loose => {
                self.members.remove(slot);
                Ok(())
            }
            MemberOwner::Class(class) => {
                Err(format!("operator class {class} requires this member"))
            }
            MemberOwner::Family => Err(format!(
                "operator family {} requires this member",
                self.key.name
            )),
            MemberOwner::Builtin => Err("the database system requires this member".to_owned()),
        }
    }
}

#[cfg(test)]
mod test_operator_family_layers {
    use std::path::Path;

    use crate::{
        context::{OperatorFamilyInfoRead, OperatorFamilyInfoWrite, ParseContext},
        names::{FullName, OperatorName, SchemaName},
        sem::{
            BinaryOperatorDefinition, OperatorDefinition, PgBuiltInType, SemAst,
            operator_family::{FamilySlot, MemberKind, OperatorFamily, OperatorFamilyKey},
        },
        test_helpers::{
            TestParseContext,
            ownership::apply_ownership_sql,
            schema_creation::{apply, child},
            schema_layer,
        },
    };

    #[test]
    fn failed_batches_preserve_effective_members_and_drop_shadows_frozen_ancestor()
    -> testresult::TestResult {
        let mut base = TestParseContext::default();
        let int = PgBuiltInType::int4();
        base.setup_type(int.clone());
        let boolean_operator = OperatorDefinition::binary(&[BinaryOperatorDefinition::new(
            &int,
            &int,
            &PgBuiltInType::bool(),
            true,
        )]);
        for token in ["=", "<"] {
            base.set_get_operator_op_result(
                &OperatorName::from_parts(Some("pg_catalog"), token),
                true,
                true,
                Some(&boolean_operator),
            );
        }
        let key = OperatorFamilyKey::new(FullName::with_schema("public", "ops"), "btree");
        let parent = schema_layer(Path::new("operator-family-parent.sql"), &base.freeze())
            .publish_operator_family(&OperatorFamily::new(key.clone()));
        let parent = apply(
            parent,
            "CREATE ACCESS METHOD btree TYPE INDEX HANDLER handler; \
             ALTER OPERATOR FAMILY public.ops USING btree ADD OPERATOR 3 = (integer, integer)",
        )?
        .freeze();

        let slot = FamilySlot {
            kind: MemberKind::Operator,
            number: 3,
            left: int.clone(),
            right: int,
        };
        let mut layer = child(parent.clone());
        layer.set_search_path(&[SchemaName::from("public")]);
        let result = apply_ownership_sql(
            layer,
            "ALTER OPERATOR FAMILY ops USING btree ADD \
             OPERATOR 1 < (integer, integer), OPERATOR 2 = (integer, integer)",
        )?;
        let SemAst::AlterOperatorFamily(statement) = result.ast() else {
            panic!("expected an analyzed ALTER OPERATOR FAMILY");
        };
        assert!(!statement.is_applicable());
        let layer = result.into_context();
        assert_eq!(
            layer.problems()[0].code(),
            "invalid_operator_family_declaration"
        );
        assert_eq!(layer.operator_family(&key).unwrap().members.len(), 1);

        let layer = apply(
            layer,
            "ALTER OPERATOR FAMILY ops USING btree DROP OPERATOR 3 (integer), OPERATOR 4 (integer)",
        )?;
        assert!(
            layer
                .operator_family(&key)
                .unwrap()
                .members
                .contains_key(&slot)
        );

        let layer = apply(
            layer,
            "ALTER OPERATOR FAMILY ops USING btree DROP OPERATOR 3 (integer)",
        )?;
        assert!(layer.operator_family(&key).unwrap().members.is_empty());
        assert!(
            parent
                .operator_family(&key)
                .unwrap()
                .members
                .contains_key(&slot)
        );

        let layer = apply(
            layer,
            "ALTER OPERATOR FAMILY ops USING btree ADD \
             OPERATOR 1 < (integer, integer), OPERATOR 3 = (integer, integer)",
        )?
        .freeze();
        let members = &layer.operator_family(&key).unwrap().members;
        assert_eq!(members.len(), 2);
        assert!(members.contains_key(&slot));
        assert_eq!(parent.operator_family(&key).unwrap().members.len(), 1);
        Ok(())
    }
}
