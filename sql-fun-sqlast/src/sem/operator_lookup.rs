use std::collections::HashMap;

use crate::{
    context::BaseParseContext,
    names::{OperatorName, SchemaName},
    sem::{
        OperatorDefinition, OperatorFamilyKey, OperatorIdentity, TypeReference,
        operator_family::MemberObject, search_path::operator_search_keys,
    },
};

/// Collects visible operator declarations for the existing overload resolver.
pub struct OperatorLookupService<'a, C: ?Sized>(&'a C);

impl<'a, C: BaseParseContext + ?Sized> OperatorLookupService<'a, C> {
    /// Use the effective catalog of the supplied analysis context.
    #[must_use]
    pub const fn new(context: &'a C) -> Self {
        Self(context)
    }

    /// Read search strategies for an already resolved declaration in a selected
    /// family. An unavailable family differs from a known empty match set.
    #[must_use]
    pub fn search_strategies(
        &self,
        declaration: &OperatorIdentity,
        family: &OperatorFamilyKey,
    ) -> Option<Vec<i32>> {
        let family = self.0.operator_family(family)?;
        let mut strategies = family
            .members
            .iter()
            .filter_map(|(slot, member)| {
                let MemberObject::Operator(name) = &member.object else {
                    return None;
                };
                if member.order_family.is_none()
                    && declaration.matches_binary(name, &slot.left, &slot.right)
                {
                    Some(slot.number)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        strategies.sort_unstable();
        Some(strategies)
    }

    /// Known search registrations in the selected method, ordered by family and
    /// strategy. A catalog extracted with a limited scope may have more members
    /// on the source server.
    #[must_use]
    pub fn search_registrations(
        &self,
        declaration: &OperatorIdentity,
        method: &str,
    ) -> Vec<(&'a OperatorFamilyKey, i32)> {
        let mut registrations = Vec::new();
        for family in self.0.operator_families() {
            let key = family.key();
            if key.method == method
                && let Some(strategies) = self.search_strategies(declaration, key)
            {
                registrations.extend(strategies.into_iter().map(|strategy| (key, strategy)));
            }
        }
        registrations.sort_by(|(a, left), (b, right)| {
            a.name
                .schema()
                .map(SchemaName::as_str)
                .cmp(&b.name.schema().map(SchemaName::as_str))
                .then(a.name.name().as_str().cmp(b.name.name().as_str()))
                .then(left.cmp(right))
        });
        registrations
    }

    /// Read the commutation fact for the resolved declaration.
    /// An unavailable fact, a known absent link, and a comparison of an
    /// existing link to the original declaration are distinct outcomes.
    #[must_use]
    pub fn self_commutation(&self, declaration: &OperatorIdentity) -> Option<Option<bool>> {
        self.0
            .operator_links(declaration)?
            .self_commutation(declaration)
    }

    /// Resolve exact PostgreSQL operand types through the effective search path.
    #[must_use]
    pub fn exact(
        &self,
        name: &OperatorName,
        left: Option<&TypeReference>,
        right: &TypeReference,
    ) -> Option<(OperatorIdentity, &'a TypeReference)> {
        operator_search_keys(self.0, name)
            .into_iter()
            .find_map(|key| {
                let identity = OperatorIdentity::from_signature(key, left.cloned(), right.clone());
                let result = identity.result_type(self.0)?;
                Some((identity, result))
            })
    }

    /// Whether the selected index method can enforce exclusion constraints.
    #[must_use]
    pub fn exclusion_capability(&self, method: &str) -> Option<bool> {
        self.0.access_method_can_exclude(method)
    }

    pub(crate) fn visible(
        &self,
        name: &OperatorName,
        left: bool,
        right: bool,
    ) -> Option<OperatorDefinition> {
        let mut first_schema = HashMap::new();
        let mut visible = match (left, right) {
            (true, true) => OperatorDefinition::Binary(Vec::new()),
            (true, false) => OperatorDefinition::LeftUnary(Vec::new()),
            (false, true) => OperatorDefinition::RightUnary(Vec::new()),
            (false, false) => return None,
        };
        for (position, key) in operator_search_keys(self.0, name).into_iter().enumerate() {
            match (&mut visible, self.0.get_operator_op(&key, left, right)) {
                (OperatorDefinition::Binary(out), Some(OperatorDefinition::Binary(entries))) => {
                    out.extend(entries.iter().filter_map(|entry| {
                        let signature = (
                            Some(entry.left_arg_type().clone()),
                            Some(entry.right_arg_type().clone()),
                        );
                        if *first_schema.entry(signature).or_insert(position) == position {
                            Some(entry.clone().with_declaration_name(&key))
                        } else {
                            None
                        }
                    }));
                }
                (
                    OperatorDefinition::LeftUnary(out),
                    Some(OperatorDefinition::LeftUnary(entries)),
                )
                | (
                    OperatorDefinition::RightUnary(out),
                    Some(OperatorDefinition::RightUnary(entries)),
                ) => {
                    out.extend(entries.iter().filter_map(|entry| {
                        let signature = (Some(entry.arg_type().clone()), None);
                        if *first_schema.entry(signature).or_insert(position) == position {
                            Some(entry.clone().with_declaration_name(&key))
                        } else {
                            None
                        }
                    }));
                }
                _ => {}
            }
        }
        if visible.is_empty() {
            None
        } else {
            Some(visible)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        names::{FullName, OperatorName},
        sem::{
            BinaryOperatorDefinition, OperatorDefinition, OperatorFamilyKey, OperatorLookupService,
            PgBuiltInType, TypeReference,
        },
        test_helpers::operator_names::lookup_context,
    };

    #[test]
    fn earlier_schema_shadows_only_identical_signatures() {
        let mut context = lookup_context(&["a", "pg_catalog"], true, true);
        let integer = PgBuiltInType::int4();
        let text = PgBuiltInType::text();
        let catalog_name = OperatorName::from_parts(Some("pg_catalog"), "+");
        context.set_get_operator_op_result(
            &catalog_name,
            true,
            true,
            Some(&OperatorDefinition::binary(&[
                BinaryOperatorDefinition::new(&integer, &integer, &integer, true),
                BinaryOperatorDefinition::new(&text, &text, &text, true),
            ])),
        );
        let service = OperatorLookupService::new(&context);
        let requested = OperatorName::from_parts(None, "+");
        let visible = service.visible(&requested, true, true).unwrap();
        let entries = visible.binary_overloads().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0].declaration_name(),
            Some(&OperatorName::from_parts(Some("a"), "+"))
        );
        assert_eq!(entries[1].declaration_name(), Some(&catalog_name));
        let qualified = service.visible(&catalog_name, true, true).unwrap();
        assert_eq!(qualified.single_declaration_name(), Some(&catalog_name));
    }

    #[test]
    fn extracted_catalog_supplies_operator_facts_through_frozen_context() -> testresult::TestResult
    {
        let catalog = crate::test_helpers::heavy::selected_root::selected_root()
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        let service = OperatorLookupService::new(&catalog);
        for (method, family, name, operand) in [
            ("btree", "integer_ops", "=", "int4"),
            ("hash", "integer_ops", "=", "int4"),
            ("brin", "integer_minmax_ops", "=", "int4"),
            ("gist", "range_ops", "&&", "anyrange"),
            ("spgist", "range_ops", "&&", "anyrange"),
            ("gin", "array_ops", "&&", "anyarray"),
        ] {
            let operand = TypeReference::from_master_type_name(operand);
            let name = OperatorName::from_parts(Some("pg_catalog"), name);
            let (operator, _) = service.exact(&name, Some(&operand), &operand).unwrap();
            let family =
                OperatorFamilyKey::new(FullName::with_schema("pg_catalog", family), method);
            assert!(
                !service
                    .search_strategies(&operator, &family)
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(service.self_commutation(&operator), Some(Some(true)));
        }

        let integer = PgBuiltInType::int4();
        let bigint = PgBuiltInType::int8();
        let equals = OperatorName::from_parts(Some("pg_catalog"), "=");
        let (cross_type, _) = service.exact(&equals, Some(&integer), &bigint).unwrap();
        assert_eq!(service.self_commutation(&cross_type), Some(Some(false)));
        assert!(
            !service
                .search_registrations(&cross_type, "btree")
                .is_empty()
        );
        let (same_type, _) = service.exact(&equals, Some(&integer), &integer).unwrap();
        assert!(service.search_registrations(&same_type, "brin").len() > 1);
        let (less, _) = service
            .exact(
                &OperatorName::from_parts(Some("pg_catalog"), "<"),
                Some(&integer),
                &integer,
            )
            .unwrap();
        let range_family =
            OperatorFamilyKey::new(FullName::with_schema("pg_catalog", "range_ops"), "gist");
        assert_eq!(
            service.search_strategies(&less, &range_family),
            Some(vec![])
        );
        assert_eq!(service.self_commutation(&less), Some(Some(false)));
        assert_eq!(service.exclusion_capability("gist"), Some(true));
        assert_eq!(service.exclusion_capability("gin"), Some(false));
        assert_eq!(service.exclusion_capability("unknown_method"), None);

        use crate::{
            context::ParseContext,
            names::SchemaName,
            sem::{ArithExpr, ScalarConstExpr, SemScalarExpr},
            test_helpers::schema_creation,
        };
        let mut context = schema_creation::apply(
            schema_creation::child(catalog),
            "CREATE ACCESS METHOD btree TYPE INDEX HANDLER pg_catalog.bthandler;
            CREATE OPERATOR public.= (
                LEFTARG = integer, RIGHTARG = integer,
                FUNCTION = pg_catalog.int4eq, COMMUTATOR = OPERATOR(public.=)
            );
            CREATE OPERATOR FAMILY public.local_integer USING btree;
            ALTER OPERATOR FAMILY public.local_integer USING btree
                ADD OPERATOR 3 public.= (integer, integer);",
        )?;
        let family =
            OperatorFamilyKey::new(FullName::with_schema("public", "local_integer"), "btree");
        for (first, second, strategies) in [
            ("public", "pg_catalog", vec![3]),
            ("pg_catalog", "public", vec![]),
        ] {
            context.set_search_path(&[SchemaName::from(first), SchemaName::from(second)]);
            let value = SemScalarExpr::new_const(ScalarConstExpr::new_integer(1));
            let name = OperatorName::from_parts(None, "=");
            let (expression, _) =
                ArithExpr::build_op(&mut context, &name, Some(value.clone()), Some(value))?;
            let operator = expression
                .operator_declaration()
                .expect("selected declaration");
            assert_eq!(operator.name(), &OperatorName::from_parts(Some(first), "="));
            let service = OperatorLookupService::new(&context);
            assert_eq!(
                service.search_strategies(operator, &family),
                Some(strategies)
            );
        }

        Ok(())
    }
}
