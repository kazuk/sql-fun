mod ddl;
mod query;

use crate::ast::{
    Access, AlterOwner, AlterSequence, BooleanExpr, BooleanOperator, Call, CallKind, Case, CaseArm,
    Cast, CastTarget, Collate, Comment, Delete, DomainConstraintTarget, DropTable, EnumType, Expr,
    Extension, FromOpt, Indirection, Insert, Literal, Merge, Name, NamedArgument, NullTest,
    ObjectArgument, ObjectTarget, OperatorExpr, OperatorKind, Order, Parameter, Query, Role,
    Sequence, Slice, SqlValue, StatementNode, Subquery, SubqueryKind, SyntaxOption,
    TransformTarget, TruthTest, TryCollectFromWithTokens, TryFromWithTokens, TryFromWithTokensOpt,
    TypeName, Update, VariableSet, View, ViewCheckOption, Window,
};
use crate::sem::{CompositeFieldCollection, CompositeFieldDefinition};
use crate::syn::{ListOpt, Opt};
use crate::{StringSpan, sem::AnalysisError, syn};

use ::sql_fun_core::IVec;

pub(in crate::ast) fn required<T>(value: Option<T>, field: &str) -> Result<T, AnalysisError> {
    match value {
        Some(value) => Ok(value),
        None => AnalysisError::raise_unexpected_none(field),
    }
}

#[cfg(test)]
mod test_required {
    #[test]
    fn preserves_value_and_reports_missing_field() {
        assert_eq!(super::required(Some(42), "answer").unwrap(), 42);
        assert!(
            super::required::<i32>(None, "answer")
                .unwrap_err()
                .to_string()
                .contains("answer")
        );
    }
}

fn nonempty(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}

impl TryFrom<syn::NodeListOpt> for Name {
    type Error = AnalysisError;

    fn try_from(nodes: syn::NodeListOpt) -> Result<Self, Self::Error> {
        let mut parts = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            parts.push(required(node.as_string().get_sval(), "name component")?);
        }
        Ok(Self::from_parts(&parts))
    }
}

#[cfg(test)]
mod test_name_from_nodes {
    use crate::{ast::Name, syn::NodeListOpt, test_helpers::SynBuilder};

    #[test]
    fn rejects_non_string_name_components() {
        let nodes = SynBuilder::new().as_node_list(&[crate::syn::Node::none()]);
        assert!(Name::try_from(NodeListOpt::from(&nodes)).is_err());
    }

    #[test]
    fn absent_list_is_an_empty_name() {
        assert!(Name::try_from(NodeListOpt::none()).unwrap().0.is_empty());
    }
}

impl From<&syn::RangeVar> for Name {
    fn from(value: &syn::RangeVar) -> Self {
        let mut parts = Vec::new();
        if let Some(catalog) = nonempty(value.get_catalogname()) {
            parts.push(catalog);
        }
        if let Some(schema) = nonempty(value.get_schemaname()) {
            parts.push(schema);
        }
        parts.push(value.get_relname());
        Self::from_parts(&parts)
    }
}

#[cfg(test)]
mod test_name_from_range_var {
    use crate::{
        ast::{FromOpt, Name},
        sem::{FullName, LocalName},
        syn::AliasOpt,
        test_helpers::SynBuilder,
    };

    #[rstest::rstest]
    #[case(FullName::new(&None, &None, &LocalName::from("items")))]
    #[case(FullName::with_schema("app", "items"))]
    #[case(FullName::with_catalog("catalog", "app", "items"))]
    fn preserves_relation_qualification(#[case] expected: FullName) -> testresult::TestResult {
        let source = SynBuilder::new().range_var(&expected, &AliasOpt::none());
        assert_eq!(Name::from(&source).full_name()?, expected);
        let optional = Name::from_opt(Some(&source)).expect("present relation");
        assert_eq!(optional.full_name()?, expected);
        Ok(())
    }

    #[test]
    fn absent_relation_remains_absent() {
        assert!(Name::from_opt(None::<&crate::syn::RangeVar>).is_none());
    }
}

impl ObjectTarget {
    fn try_from_node(
        object: &syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        if let Some(object) = object.as_object_with_args().as_inner() {
            let name = Name::try_from(object.get_objname())?.0;
            let argument_types = if object.get_args_unspecified() {
                None
            } else {
                let function_arguments = object.get_objfuncargs().as_inner().unwrap_or_default();
                let mut types = Vec::new();
                if function_arguments.is_empty() {
                    for argument in object.get_objargs().as_inner().unwrap_or_default() {
                        types.push(ObjectArgument::try_from_node(&argument, tokens)?);
                    }
                } else {
                    for argument in function_arguments {
                        let argument = required(
                            argument.as_function_parameter().as_inner(),
                            "object_with_args.objfuncargs.function_parameter",
                        )?;
                        types.push(ObjectArgument::try_from_function_parameter(
                            &argument, tokens,
                        )?);
                    }
                }
                Some(types)
            };
            Ok(Self::new(&name, argument_types.as_deref()))
        } else if let Some(items) = object.as_list().get_items().map(|items| items.as_string()) {
            let name: Vec<_> = items.iter().map(crate::syn::String::get_sval).collect();
            Ok(Self::new(&name, None))
        } else if let Some(item) = object.as_string().get_sval() {
            Ok(Self::new(std::slice::from_ref(&item), None))
        } else if let Some(item) = object.as_integer().get_ival() {
            let name = item.to_string();
            Ok(Self::new(std::slice::from_ref(&name), None))
        } else if let Some(value) = object.as_a_const().as_inner() {
            let Literal::Integer(value) = Literal::from(&value) else {
                return AnalysisError::raise_unexpected_input("numeric database object target");
            };
            let name = value.to_string();
            Ok(Self::new(std::slice::from_ref(&name), None))
        } else if let Some(type_name) = object.as_type_name().as_inner() {
            let type_name = TypeName::try_from_with_tokens(&type_name, tokens)?;
            Ok(Self::with_type_name(&type_name))
        } else {
            AnalysisError::raise_unexpected_input("database object target")
        }
    }
}

#[cfg(test)]
mod test_object_target {
    use crate::{ast::ObjectTarget, syn::Opt, test_helpers::postgres_node};
    use ::sql_fun_core::IVec;

    #[rstest::rstest]
    #[case::unspecified("COMMENT ON FUNCTION app.f IS 'function'", None)]
    #[case::no_arguments("COMMENT ON FUNCTION app.f() IS 'function'", Some(0))]
    #[case::typed_arguments("COMMENT ON FUNCTION app.f(integer, text) IS 'function'", Some(2))]
    fn distinguishes_unspecified_and_explicit_signatures(
        #[case] sql: &str,
        #[case] argument_count: Option<usize>,
    ) {
        let node = postgres_node(sql)
            .as_comment_stmt()
            .get_object()
            .as_inner()
            .unwrap();
        let target = ObjectTarget::try_from_node(&node, &IVec::default()).unwrap();
        assert_eq!(target.name, ["app", "f"]);
        assert_eq!(target.argument_types.as_ref().map(Vec::len), argument_count);
    }
}

impl ObjectTarget {
    fn try_from_cast_node(
        object: &syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let cast = CastTarget::try_from_node(object, tokens)?;
        Ok(Self::with_cast(&cast))
    }
}

impl ObjectTarget {
    fn try_from_domain_constraint_node(
        object: &syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let target = DomainConstraintTarget::try_from_node(object, tokens)?;
        Ok(Self::with_domain_constraint(&target))
    }
}

impl ObjectTarget {
    fn try_from_transform_node(
        object: &syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let target = TransformTarget::try_from_node(object, tokens)?;
        Ok(Self::with_transform(&target))
    }
}

impl ObjectTarget {
    fn new(name: &[String], argument_types: Option<&[ObjectArgument]>) -> Self {
        Self {
            name: name.to_vec(),
            argument_types: argument_types.map(<[ObjectArgument]>::to_vec),
            cast: None,
            type_name: None,
            domain_constraint: None,
            transform: None,
        }
    }
}

impl ObjectTarget {
    fn with_type_name(type_name: &TypeName) -> Self {
        Self {
            name: type_name.name.0.clone(),
            argument_types: None,
            cast: None,
            type_name: Some(type_name.clone()),
            domain_constraint: None,
            transform: None,
        }
    }
}

impl ObjectTarget {
    fn with_cast(cast: &CastTarget) -> Self {
        Self {
            name: Vec::new(),
            argument_types: None,
            cast: Some(cast.clone()),
            type_name: None,
            domain_constraint: None,
            transform: None,
        }
    }
}

impl ObjectTarget {
    fn with_domain_constraint(domain_constraint: &DomainConstraintTarget) -> Self {
        Self {
            name: Vec::new(),
            argument_types: None,
            cast: None,
            type_name: None,
            domain_constraint: Some(domain_constraint.clone()),
            transform: None,
        }
    }
}

impl ObjectTarget {
    fn with_transform(transform: &TransformTarget) -> Self {
        Self {
            name: Vec::new(),
            argument_types: None,
            cast: None,
            type_name: None,
            domain_constraint: None,
            transform: Some(transform.clone()),
        }
    }
}

impl ObjectArgument {
    fn try_from_node(
        argument: &syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let type_name = if argument.as_inner().is_none() {
            None
        } else if let Some(type_name) = argument.as_type_name().as_inner() {
            Some(TypeName::try_from_with_tokens(&type_name, tokens)?)
        } else if argument
            .as_string()
            .get_sval()
            .is_some_and(|value| value.eq_ignore_ascii_case("none"))
        {
            None
        } else {
            return AnalysisError::raise_unexpected_input("object_with_args.objargs.type_name");
        };
        Ok(Self::new(None, None, type_name.as_ref()))
    }
}

#[cfg(test)]
mod test_object_argument {
    use crate::{
        ast::ObjectArgument,
        syn::{Node, NodeInner},
        test_helpers::SynBuilder,
    };
    use ::sql_fun_core::IVec;

    #[rstest::rstest]
    #[case::absent(None)]
    #[case::none(Some("none"))]
    #[case::mixed_case(Some("NoNe"))]
    fn preserves_absent_argument_type(#[case] spelling: Option<&str>) {
        let builder = SynBuilder::new();
        let node = spelling.map_or_else(Node::none, |text| {
            builder.as_string_node(builder.string(text))
        });
        let argument = ObjectArgument::try_from_node(&node, &IVec::default()).unwrap();
        assert!(argument.type_name.is_none());
        assert!(argument.name.is_none());
        assert!(argument.mode.is_none());
    }

    #[test]
    fn retains_qualified_type() {
        let node = Node::from(NodeInner::from(
            SynBuilder::new().type_name(&["app", "amount"]),
        ));
        let argument = ObjectArgument::try_from_node(&node, &IVec::default()).unwrap();
        assert_eq!(argument.type_name.unwrap().name.0, ["app", "amount"]);
    }

    #[test]
    #[should_panic(expected = "unexpected input object_with_args.objargs.type_name")]
    fn rejects_non_type_argument() {
        let builder = SynBuilder::new();
        let node = builder.as_string_node(builder.string("integer"));
        assert!(ObjectArgument::try_from_node(&node, &IVec::default()).is_err());
    }
}

impl ObjectArgument {
    fn try_from_function_parameter(
        argument: &syn::FunctionParameter,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let type_name = required(
            argument.get_arg_type().as_inner(),
            "object_with_args.objfuncargs.arg_type",
        )?;
        let mode = match argument.get_mode().as_inner() {
            Some(crate::syn::FunctionParameterMode::FuncParamIn) => Some("IN"),
            Some(crate::syn::FunctionParameterMode::FuncParamOut) => Some("OUT"),
            Some(crate::syn::FunctionParameterMode::FuncParamInout) => Some("INOUT"),
            Some(crate::syn::FunctionParameterMode::FuncParamVariadic) => Some("VARIADIC"),
            _ => None,
        }
        .map(str::to_owned);
        let name = argument.get_name();
        let type_name = TypeName::try_from_with_tokens(&type_name, tokens)?;
        Ok(Self::new(
            mode.as_deref(),
            if name.is_empty() {
                None
            } else {
                Some(name.as_str())
            },
            Some(&type_name),
        ))
    }
}

#[cfg(test)]
mod test_object_function_parameter {
    use crate::{
        ast::ObjectArgument,
        syn::{FunctionParameterMode, TypeNameOpt},
        test_helpers::SynBuilder,
    };
    use ::sql_fun_core::IVec;

    #[rstest::rstest]
    #[case::input(FunctionParameterMode::FuncParamIn, "IN")]
    #[case::output(FunctionParameterMode::FuncParamOut, "OUT")]
    #[case::inout(FunctionParameterMode::FuncParamInout, "INOUT")]
    #[case::variadic(FunctionParameterMode::FuncParamVariadic, "VARIADIC")]
    fn retains_mode_name_and_type(#[case] mode: FunctionParameterMode, #[case] expected: &str) {
        let builder = SynBuilder::new();
        let parameter = builder.function_parameter(
            "value",
            TypeNameOpt::from(builder.type_name(&["int4"])),
            mode,
        );
        let argument =
            ObjectArgument::try_from_function_parameter(&parameter, &IVec::default()).unwrap();
        assert_eq!(argument.mode.as_deref(), Some(expected));
        assert_eq!(argument.name.as_deref(), Some("value"));
        assert_eq!(argument.type_name.unwrap().name.0, ["int4"]);
    }

    #[test]
    fn rejects_missing_parameter_type() {
        let parameter = SynBuilder::new().function_parameter(
            "",
            TypeNameOpt::none(),
            FunctionParameterMode::FuncParamIn,
        );
        assert!(ObjectArgument::try_from_function_parameter(&parameter, &IVec::default()).is_err());
    }
}

impl ObjectArgument {
    fn new(mode: Option<&str>, name: Option<&str>, type_name: Option<&TypeName>) -> Self {
        Self {
            mode: mode.map(str::to_owned),
            name: name.map(str::to_owned),
            type_name: type_name.cloned(),
        }
    }
}

impl CastTarget {
    fn try_from_node(
        object: &syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let Some(items) = object.as_list().get_items().as_inner() else {
            return AnalysisError::raise_unexpected_input("COMMENT ON CAST target");
        };
        if items.len() != 2 {
            return AnalysisError::raise_unexpected_input(
                "COMMENT ON CAST source and target types",
            );
        }
        let source = required(
            items[0].as_type_name().as_inner(),
            "comment_stmt.object.cast.source",
        )?;
        let target = required(
            items[1].as_type_name().as_inner(),
            "comment_stmt.object.cast.target",
        )?;
        let source = TypeName::try_from_with_tokens(&source, tokens)?;
        let target = TypeName::try_from_with_tokens(&target, tokens)?;
        Ok(Self::new(&source, &target))
    }
}

#[cfg(test)]
mod test_cast_target {
    use crate::{ast::CastTarget, syn::Node, test_helpers::SynBuilder};
    use ::sql_fun_core::IVec;

    #[test]
    #[should_panic(expected = "unexpected input COMMENT ON")]
    fn rejects_non_list_target() {
        assert!(CastTarget::try_from_node(&Node::none(), &IVec::default()).is_err());
    }

    #[rstest::rstest]
    #[case::empty(0)]
    #[case::one(1)]
    #[case::three(3)]
    #[should_panic(expected = "unexpected input COMMENT ON")]
    fn rejects_wrong_target_arity(#[case] count: usize) {
        let node = SynBuilder::new().build_list_node(vec![Node::none(); count]);
        assert!(CastTarget::try_from_node(&node, &IVec::default()).is_err());
    }

    #[test]
    fn rejects_missing_first_component() {
        let node = SynBuilder::new().build_list_node(vec![Node::none(), Node::none()]);
        let error = CastTarget::try_from_node(&node, &IVec::default()).unwrap_err();
        assert!(error.to_string().contains("comment_stmt.object"));
    }
}

impl CastTarget {
    fn new(source: &TypeName, target: &TypeName) -> Self {
        Self {
            source: source.clone(),
            target: target.clone(),
        }
    }
}

impl DomainConstraintTarget {
    fn try_from_node(
        object: &syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let Some(items) = object.as_list().get_items().as_inner() else {
            return AnalysisError::raise_unexpected_input("COMMENT ON DOMAIN CONSTRAINT target");
        };
        if items.len() != 2 {
            return AnalysisError::raise_unexpected_input("COMMENT ON DOMAIN CONSTRAINT target");
        }
        let domain = required(
            items[0].as_type_name().as_inner(),
            "comment_stmt.object.domain_constraint.domain",
        )?;
        let constraint = required(
            items[1].as_string().get_sval(),
            "comment_stmt.object.domain_constraint.constraint",
        )?;
        let domain = TypeName::try_from_with_tokens(&domain, tokens)?;
        Ok(Self::new(&domain, &constraint))
    }
}

#[cfg(test)]
mod test_domain_constraint_target {
    use crate::{ast::DomainConstraintTarget, syn::Node, test_helpers::SynBuilder};
    use ::sql_fun_core::IVec;

    #[test]
    #[should_panic(expected = "unexpected input COMMENT ON")]
    fn rejects_non_list_target() {
        assert!(DomainConstraintTarget::try_from_node(&Node::none(), &IVec::default()).is_err());
    }

    #[rstest::rstest]
    #[case::empty(0)]
    #[case::one(1)]
    #[case::three(3)]
    #[should_panic(expected = "unexpected input COMMENT ON")]
    fn rejects_wrong_target_arity(#[case] count: usize) {
        let node = SynBuilder::new().build_list_node(vec![Node::none(); count]);
        assert!(DomainConstraintTarget::try_from_node(&node, &IVec::default()).is_err());
    }

    #[test]
    fn rejects_missing_first_component() {
        let node = SynBuilder::new().build_list_node(vec![Node::none(), Node::none()]);
        let error = DomainConstraintTarget::try_from_node(&node, &IVec::default()).unwrap_err();
        assert!(error.to_string().contains("comment_stmt.object"));
    }
}

impl DomainConstraintTarget {
    fn new(domain: &TypeName, constraint: &str) -> Self {
        Self {
            domain: domain.clone(),
            constraint: constraint.to_owned(),
        }
    }
}

impl TransformTarget {
    fn try_from_node(
        object: &syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let Some(items) = object.as_list().get_items().as_inner() else {
            return AnalysisError::raise_unexpected_input("COMMENT ON TRANSFORM target");
        };
        if items.len() != 2 {
            return AnalysisError::raise_unexpected_input("COMMENT ON TRANSFORM target");
        }
        let type_name = required(
            items[0].as_type_name().as_inner(),
            "comment_stmt.object.transform.type_name",
        )?;
        let language = required(
            items[1].as_string().get_sval(),
            "comment_stmt.object.transform.language",
        )?;
        let type_name = TypeName::try_from_with_tokens(&type_name, tokens)?;
        Ok(Self::new(&type_name, &language))
    }
}

#[cfg(test)]
mod test_transform_target {
    use crate::{ast::TransformTarget, syn::Node, test_helpers::SynBuilder};
    use ::sql_fun_core::IVec;

    #[test]
    #[should_panic(expected = "unexpected input COMMENT ON")]
    fn rejects_non_list_target() {
        assert!(TransformTarget::try_from_node(&Node::none(), &IVec::default()).is_err());
    }

    #[rstest::rstest]
    #[case::empty(0)]
    #[case::one(1)]
    #[case::three(3)]
    #[should_panic(expected = "unexpected input COMMENT ON")]
    fn rejects_wrong_target_arity(#[case] count: usize) {
        let node = SynBuilder::new().build_list_node(vec![Node::none(); count]);
        assert!(TransformTarget::try_from_node(&node, &IVec::default()).is_err());
    }

    #[test]
    fn rejects_missing_first_component() {
        let node = SynBuilder::new().build_list_node(vec![Node::none(), Node::none()]);
        let error = TransformTarget::try_from_node(&node, &IVec::default()).unwrap_err();
        assert!(error.to_string().contains("comment_stmt.object"));
    }
}

impl TransformTarget {
    fn new(type_name: &TypeName, language: &str) -> Self {
        Self {
            type_name: type_name.clone(),
            language: language.to_owned(),
        }
    }
}

impl TryFromWithTokens<&syn::Node> for StatementNode {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        if value
            .as_alter_default_privileges_stmt()
            .as_inner()
            .is_some()
        {
            todo!("common-AST conversion for ALTER DEFAULT PRIVILEGES (#891)");
        }
        if value.as_create_foreign_table_stmt().as_inner().is_some() {
            todo!("common-AST conversion for CREATE FOREIGN TABLE (#891)");
        }
        if let Some(query) = value.as_select_stmt().as_inner() {
            return Ok(Self::query(Query::try_from_with_tokens(&query, tokens)?));
        }
        if let Some(insert) = value.as_insert_stmt().as_inner() {
            return Ok(Self::insert(Insert::try_from_with_tokens(&insert, tokens)?));
        }
        if let Some(update) = value.as_update_stmt().as_inner() {
            return Ok(Self::update(Update::try_from_with_tokens(&update, tokens)?));
        }
        if let Some(delete) = value.as_delete_stmt().as_inner() {
            return Ok(Self::delete(Delete::try_from_with_tokens(&delete, tokens)?));
        }
        if let Some(merge) = value.as_merge_stmt().as_inner() {
            return Ok(Self::merge(Merge::try_from_with_tokens(&merge, tokens)?));
        }
        if let Some(trigger) = value.as_create_trig_stmt().as_inner() {
            return Ok(Self::create_trigger(
                super::CreateTrigger::try_from_with_tokens(&trigger, tokens)?,
            ));
        }
        if let Some(rule) = value.as_rule_stmt().as_inner() {
            return Ok(Self::create_rule(super::CreateRule::try_from_with_tokens(
                &rule, tokens,
            )?));
        }
        if let Some(view) = value.as_view_stmt().as_inner() {
            return Ok(Self::view(View::try_from_with_tokens(&view, tokens)?));
        }
        if let Some(extension) = value.as_create_extension_stmt().as_inner() {
            return Ok(Self::extension(Extension::try_from_with_tokens(
                &extension, tokens,
            )?));
        }
        if let Some(variable_set) = value.as_variable_set_stmt().as_inner() {
            return Ok(Self::variable_set(VariableSet::try_from_with_tokens(
                &variable_set,
                tokens,
            )?));
        }
        if let Some(enum_type) = value.as_create_enum_stmt().as_inner() {
            return Ok(Self::enum_type(EnumType::try_from(&enum_type)?));
        }
        if let Some(sequence) = value.as_create_seq_stmt().as_inner() {
            return Ok(Self::sequence(Sequence::try_from_with_tokens(
                &sequence, tokens,
            )?));
        }
        if let Some(drop_table) = value.as_drop_stmt().as_inner() {
            if matches!(
                drop_table.get_remove_type().as_inner(),
                Some(syn::ObjectType::ObjectTrigger)
            ) {
                return Ok(Self::drop_trigger(super::DropTrigger::try_from(
                    &drop_table,
                )?));
            }
            if matches!(
                drop_table.get_remove_type().as_inner(),
                Some(syn::ObjectType::ObjectRule)
            ) {
                return Ok(Self::drop_rule(super::DropRule::try_from(&drop_table)?));
            }
            return Ok(Self::drop_table(DropTable::try_from(&drop_table)?));
        }
        if let Some(alter_sequence) = value.as_alter_seq_stmt().as_inner() {
            return Ok(Self::alter_sequence(AlterSequence::try_from_with_tokens(
                &alter_sequence,
                tokens,
            )?));
        }
        if let Some(comment) = value.as_comment_stmt().as_inner() {
            return Ok(Self::comment(Comment::try_from_with_tokens(
                &comment, tokens,
            )?));
        }
        if let Some(alter_owner) = value.as_alter_owner_stmt().as_inner() {
            return Ok(Self::alter_owner(AlterOwner::try_from_with_tokens(
                &alter_owner,
                tokens,
            )?));
        }
        if let Some(composite_type) = value.as_composite_type_stmt().as_inner() {
            return Ok(Self::composite_type(
                crate::ast::CompositeType::try_from_with_tokens(&composite_type, tokens)?,
            ));
        }
        if let Some(create_table) = value.as_create_stmt().as_inner() {
            return Ok(Self::table(super::Table::try_from_with_tokens(
                &create_table,
                tokens,
            )?));
        }
        if let Some(create_function) = value.as_create_function_stmt().as_inner() {
            return Ok(Self::function(super::ddl::Function::try_from_with_tokens(
                &create_function,
                tokens,
            )?));
        }
        if let Some(create_schema) = value.as_create_schema_stmt().as_inner() {
            return Ok(Self::schema(super::ddl::Schema::try_from_with_tokens(
                &create_schema,
                tokens,
            )?));
        }
        if let Some(create_domain) = value.as_create_domain_stmt().as_inner() {
            return Ok(Self::domain(super::ddl::Domain::try_from_with_tokens(
                &create_domain,
                tokens,
            )?));
        }
        if let Some(rename) = value.as_rename_stmt().as_inner()
            && (matches!(
                rename.get_rename_type().as_inner(),
                Some(syn::ObjectType::ObjectTable)
            ) || (matches!(
                rename.get_rename_type().as_inner(),
                Some(syn::ObjectType::ObjectColumn)
            ) && matches!(
                rename.get_relation_type().as_inner(),
                Some(syn::ObjectType::ObjectTable)
            )))
        {
            return Ok(Self::alter_relation(
                super::AlterRelation::try_from_with_tokens(&rename, tokens)?,
            ));
        }
        if let Some(alter_table) = value.as_alter_table_stmt().as_inner() {
            return Ok(Self::alter_relation(
                super::AlterRelation::try_from_with_tokens(&alter_table, tokens)?,
            ));
        }
        if let Some(create_table_as) = value.as_create_table_as_stmt().as_inner() {
            return Ok(Self::table_as(super::ddl::TableAs::try_from_with_tokens(
                &create_table_as,
                tokens,
            )?));
        }
        if let Some(create_index) = value.as_index_stmt().as_inner() {
            return Ok(Self::index(super::ddl::Index::try_from_with_tokens(
                &create_index,
                tokens,
            )?));
        }
        AnalysisError::raise_unexpected_input("top-level PostgreSQL statement")
    }
}

#[cfg(test)]
mod unimplemented_utility_conversion_tests {
    #[rstest::rstest]
    #[case::large_objects("ALTER DEFAULT PRIVILEGES GRANT SELECT ON LARGE OBJECTS TO PUBLIC")]
    #[case::foreign_like("CREATE FOREIGN TABLE f (LIKE t) SERVER s")]
    #[should_panic(expected = "common-AST conversion")]
    fn retains_explicit_todo(#[case] sql: &str) {
        let _ = crate::parse_statements(sql, crate::ast::Dialect::postgres()).next();
    }
}

impl TryFromWithTokens<&syn::CreateDomainStmt> for super::ddl::Domain {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::CreateDomainStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let base = required(value.get_type_name().as_inner(), "create_domain.type_name")?;
        let mut constraints = Vec::new();
        for constraint in value
            .get_constraints()
            .map(|constraints| constraints.as_constraint())
            .unwrap_or_default()
        {
            constraints.push(super::TableConstraint::from_postgres_inline(
                &constraint,
                tokens,
            )?);
        }
        Ok(Self::new(
            Name::try_from(value.get_domainname())?,
            TypeName::try_from_with_tokens(&base, tokens)?,
            source_span(base.get_location(), tokens).unwrap_or_else(|| StringSpan::new(0, 0)),
            constraints,
            Name::try_from(value.get_coll_clause().get_collname())?,
        ))
    }
}

impl TryFrom<&syn::CompositeTypeStmt> for crate::sem::CreateCompositeType {
    type Error = AnalysisError;

    fn try_from(value: &syn::CompositeTypeStmt) -> Result<Self, Self::Error> {
        let typevar = required(
            value.get_typevar().as_inner(),
            "composite_type_stmt.typevar",
        )?;
        let fields = required(
            value.get_coldeflist().map(|fields| fields.as_column_def()),
            "composite_type_stmt.coldeflist",
        )?;
        let mut composite_fields = CompositeFieldCollection::default();
        for field in fields {
            composite_fields.push(CompositeFieldDefinition::try_from(&field)?);
        }
        Ok(Self::new(
            &crate::sem::FullName::try_from(typevar)?,
            &composite_fields,
        ))
    }
}

impl TryFromWithTokens<&syn::AlterOwnerStmt> for AlterOwner {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::AlterOwnerStmt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let relation = Name::from_opt(value.get_relation().as_inner().as_ref());
        let object = value
            .get_object()
            .as_inner()
            .map(|object| ObjectTarget::try_from_node(&object, tokens))
            .transpose()?;
        let owner = required(value.get_newowner().as_inner(), "alter_owner.newowner")?;
        Ok(Self::new(
            value
                .get_object_type()
                .as_inner()
                .map(|object_kind| crate::sem::ObjectKind::from(object_kind as u32)),
            relation,
            object,
            Role::new(
                owner
                    .get_roletype()
                    .as_inner()
                    .map_or(0, |kind| kind as u32),
                &owner.get_rolename(),
            ),
        ))
    }
}

impl TryFromWithTokens<&syn::CommentStmt> for Comment {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::CommentStmt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let object_kind = required(value.get_objtype().as_inner(), "comment_stmt.objtype")?;
        let object = required(value.get_object().as_inner(), "comment_stmt.object")?;
        let target = if matches!(object_kind, crate::syn::ObjectType::ObjectCast) {
            ObjectTarget::try_from_cast_node(&object, tokens)?
        } else if matches!(object_kind, crate::syn::ObjectType::ObjectDomconstraint) {
            ObjectTarget::try_from_domain_constraint_node(&object, tokens)?
        } else if matches!(object_kind, crate::syn::ObjectType::ObjectTransform) {
            ObjectTarget::try_from_transform_node(&object, tokens)?
        } else {
            ObjectTarget::try_from_node(&object, tokens)?
        };
        Ok(Self::new(
            target,
            crate::sem::ObjectKind::from(object_kind as u32),
            &value.get_comment(),
        ))
    }
}

#[cfg(test)]
mod test_comment_target_conversion {
    use crate::{
        ast::{Comment, TryFromWithTokens},
        syn::Opt,
        test_helpers::postgres_node,
    };
    use ::sql_fun_core::IVec;

    #[test]
    fn preserves_cast_source_and_target_types() {
        let node = postgres_node("COMMENT ON CAST (app.source AS app.target) IS 'conversion'")
            .as_comment_stmt()
            .as_inner()
            .unwrap();
        let comment = Comment::try_from_with_tokens(&node, &IVec::default()).unwrap();
        let cast = comment.target.cast.unwrap();
        assert_eq!(cast.source.name.0, ["app", "source"]);
        assert_eq!(cast.target.name.0, ["app", "target"]);
    }

    #[test]
    fn preserves_domain_constraint_identity() {
        let node =
            postgres_node("COMMENT ON CONSTRAINT positive ON DOMAIN app.amount IS 'constraint'")
                .as_comment_stmt()
                .as_inner()
                .unwrap();
        let comment = Comment::try_from_with_tokens(&node, &IVec::default()).unwrap();
        let target = comment.target.domain_constraint.unwrap();
        assert_eq!(target.domain.name.0, ["app", "amount"]);
        assert_eq!(target.constraint, "positive");
    }

    #[test]
    fn preserves_transform_type_and_language() {
        let node = postgres_node("COMMENT ON TRANSFORM FOR app.amount LANGUAGE sql IS 'transform'")
            .as_comment_stmt()
            .as_inner()
            .unwrap();
        let comment = Comment::try_from_with_tokens(&node, &IVec::default()).unwrap();
        let target = comment.target.transform.unwrap();
        assert_eq!(target.type_name.name.0, ["app", "amount"]);
        assert_eq!(target.language, "sql");
    }
}

impl TryFromWithTokens<&syn::AlterSeqStmt> for AlterSequence {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::AlterSeqStmt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let sequence = required(value.get_sequence().as_inner(), "alter_seq.sequence")?;
        Ok(Self::new(
            Name::from(&sequence),
            SyntaxOption::try_collect_from_with_tokens(value.get_options(), tokens)?,
            value.get_for_identity(),
            value.get_missing_ok(),
        ))
    }
}

impl TryFrom<&syn::DropStmt> for DropTable {
    type Error = AnalysisError;

    fn try_from(value: &syn::DropStmt) -> Result<Self, Self::Error> {
        if !matches!(
            value.get_remove_type().as_inner(),
            Some(syn::ObjectType::ObjectTable)
        ) {
            return AnalysisError::raise_unexpected_input("drop statement object type");
        }
        let objects = required(value.get_objects().as_inner(), "drop_stmt.objects")?;
        let names = objects
            .iter()
            .map(|object| Name::try_from(object.as_list().get_items()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self::new(
            names,
            value.get_missing_ok(),
            matches!(
                value.get_behavior().as_inner(),
                Some(syn::DropBehavior::DropCascade)
            ),
            value.get_concurrent(),
        ))
    }
}

impl TryFromWithTokens<&syn::CreateSeqStmt> for Sequence {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::CreateSeqStmt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let sequence = required(value.get_sequence().as_inner(), "create_seq.sequence")?;
        Ok(Self::new(
            Name::from(&sequence),
            SyntaxOption::try_collect_from_with_tokens(value.get_options(), tokens)?,
            value.get_owner_id(),
            value.get_for_identity(),
            value.get_if_not_exists(),
        )
        .with_temporary(sequence.get_relpersistence() == "t"))
    }
}

impl TryFrom<&syn::CreateEnumStmt> for EnumType {
    type Error = AnalysisError;

    fn try_from(value: &syn::CreateEnumStmt) -> Result<Self, Self::Error> {
        let Some(values) = value.get_vals().map(|values| values.as_string()) else {
            return AnalysisError::raise_unexpected_none("create_enum_stmt.vals");
        };
        Ok(Self::new(
            Name::try_from(value.get_type_name())?,
            values.iter().map(|value| value.get_sval()).collect(),
        ))
    }
}

impl TryFromWithTokens<&syn::VariableSetStmt> for VariableSet {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::VariableSetStmt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let name = value.get_name();
        let kind =
            super::VariableSetKind::try_from(required(value.get_kind().as_inner(), "SET kind")?)?;
        let transaction_modes = matches!(kind, super::VariableSetKind::Multi)
            && (name.eq_ignore_ascii_case("transaction")
                || name.eq_ignore_ascii_case("session characteristics"));
        let arguments = if transaction_modes {
            Vec::new()
        } else {
            Expr::try_collect_from_with_tokens(value.get_args(), tokens)?
        };
        let mut statement = Self::new(&name, arguments);
        statement.kind = kind;
        statement.is_local = value.get_is_local();
        if transaction_modes {
            statement.transaction_options = Some(SyntaxOption::try_collect_from_with_tokens(
                value.get_args(),
                tokens,
            )?);
        }
        Ok(statement)
    }
}

impl TryFromWithTokens<&syn::CreateExtensionStmt> for Extension {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::CreateExtensionStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let mut statement = Self::new(&value.get_extname());
        statement.if_not_exists = value.get_if_not_exists();
        statement.options =
            SyntaxOption::try_collect_from_with_tokens(value.get_options(), tokens)?;
        Ok(statement)
    }
}
impl TryFrom<syn::VariableSetKind> for super::VariableSetKind {
    type Error = AnalysisError;
    fn try_from(value: syn::VariableSetKind) -> Result<Self, Self::Error> {
        Ok(match value {
            syn::VariableSetKind::VarSetValue => Self::Value,
            syn::VariableSetKind::VarSetDefault => Self::Default,
            syn::VariableSetKind::VarSetCurrent => Self::Current,
            syn::VariableSetKind::VarSetMulti => Self::Multi,
            syn::VariableSetKind::VarReset => Self::Reset,
            syn::VariableSetKind::VarResetAll => Self::ResetAll,
            _ => return AnalysisError::raise_unexpected_input("SET kind"),
        })
    }
}

impl TryFromWithTokens<&syn::ViewStmt> for View {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::ViewStmt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let view = required(value.get_view().as_inner(), "view_stmt.view")?;
        let query = required(
            value.get_query().as_select_stmt().as_inner(),
            "view_stmt.query",
        )?;
        let mut aliases = Vec::new();
        for alias in value.get_aliases().as_inner().unwrap_or_default() {
            aliases.push(required(alias.as_string().get_sval(), "view alias")?);
        }
        Ok(Self::new(
            Name::from(&view),
            aliases,
            Query::try_from_with_tokens(&query, tokens)?,
            value.get_replace(),
            SyntaxOption::try_collect_from_with_tokens(value.get_options(), tokens)?,
            ViewCheckOption::from(value.get_with_check_option().as_inner()),
            view.get_relpersistence() == "t",
            false,
            false,
        ))
    }
}

impl From<Option<syn::ViewCheckOption>> for ViewCheckOption {
    fn from(value: Option<syn::ViewCheckOption>) -> Self {
        match value {
            Some(syn::ViewCheckOption::LocalCheckOption) => Self::Local,
            Some(syn::ViewCheckOption::CascadedCheckOption) => Self::Cascaded,
            Some(syn::ViewCheckOption::Undefined)
            | Some(syn::ViewCheckOption::NoCheckOption)
            | Some(syn::ViewCheckOption::OptionNone)
            | None => Self::None,
        }
    }
}

impl TryFromWithTokens<&syn::ColumnRef> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        column: &syn::ColumnRef,
        _tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let mut parts = Vec::new();
        let mut star = false;
        for field in column.get_fields().as_inner().unwrap_or_default() {
            if field.is_a_star() {
                star = true;
            } else {
                parts.push(required(field.as_string().get_sval(), "column name")?);
            }
        }
        let name = Name::from_parts(&parts);
        Ok(if star {
            Self::Star(name)
        } else {
            Self::Column(name)
        })
    }
}

impl TryFromWithTokens<&syn::AExpr> for OperatorExpr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::AExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            name: Name::try_from(value.get_name())?,
            kind: OperatorKind::try_from(required(value.get_kind().as_inner(), "operator kind")?)?,
            left: Expr::try_from_with_tokens_opt(value.get_lexpr(), tokens)?,
            right: Expr::try_from_with_tokens_opt(value.get_rexpr(), tokens)?,
            escape: None,
        })
    }
}

impl TryFromWithTokens<&syn::AExpr> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::AExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Operator(Box::new(
            OperatorExpr::try_from_with_tokens(value, tokens)?,
        )))
    }
}

impl TryFromWithTokens<&syn::BoolExpr> for BooleanExpr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::BoolExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            operator: BooleanOperator::try_from(required(
                value.get_boolop().as_inner(),
                "boolean operator",
            )?)?,
            arguments: Expr::try_collect_from_with_tokens(value.get_args(), tokens)?,
        })
    }
}

impl TryFromWithTokens<&syn::BoolExpr> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::BoolExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Boolean(Box::new(BooleanExpr::try_from_with_tokens(
            value, tokens,
        )?)))
    }
}

impl TryFromWithTokens<&syn::NullTest> for NullTest {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        test: &syn::NullTest,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            argument: Expr::try_from_with_tokens(&test.get_arg(), tokens)?,
            negated: matches!(
                test.get_nulltesttype().as_inner(),
                Some(syn::NullTestType::IsNotNull)
            ),
            row: test.get_argisrow(),
        })
    }
}

impl TryFromWithTokens<&syn::NullTest> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        test: &syn::NullTest,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::NullTest(Box::new(NullTest::try_from_with_tokens(
            test, tokens,
        )?)))
    }
}

impl TryFromWithTokens<&syn::BooleanTest> for TruthTest {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        test: &syn::BooleanTest,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let (value, negated) = match required(test.get_booltesttype().as_inner(), "truth test")? {
            syn::BoolTestType::IsTrue => (Some(true), false),
            syn::BoolTestType::IsNotTrue => (Some(true), true),
            syn::BoolTestType::IsFalse => (Some(false), false),
            syn::BoolTestType::IsNotFalse => (Some(false), true),
            syn::BoolTestType::IsUnknown => (None, false),
            syn::BoolTestType::IsNotUnknown => (None, true),
            _ => return AnalysisError::raise_unexpected_input("truth test"),
        };
        Ok(Self {
            argument: Expr::try_from_with_tokens(&test.get_arg(), tokens)?,
            value,
            negated,
        })
    }
}

impl TryFromWithTokens<&syn::BooleanTest> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        test: &syn::BooleanTest,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::TruthTest(Box::new(TruthTest::try_from_with_tokens(
            test, tokens,
        )?)))
    }
}

impl TryFromWithTokens<&syn::TypeCast> for Cast {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        cast: &syn::TypeCast,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            argument: Expr::try_from_with_tokens(&cast.get_arg(), tokens)?,
            target: TypeName::try_from_with_tokens(
                &required(cast.get_type_name().as_inner(), "cast type")?,
                tokens,
            )?,
        })
    }
}

impl TryFromWithTokens<&syn::TypeCast> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        cast: &syn::TypeCast,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Cast(Box::new(Cast::try_from_with_tokens(
            cast, tokens,
        )?)))
    }
}

impl TryFromWithTokens<&syn::CollateClause> for Collate {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        collate: &syn::CollateClause,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            argument: Expr::try_from_with_tokens(&collate.get_arg(), tokens)?,
            name: Name::try_from(collate.get_collname())?,
            location: if collate.get_location() < 0 {
                None
            } else {
                source_span(collate.get_location() + "COLLATE".len() as i32, tokens)
            },
        })
    }
}

impl TryFromWithTokens<&syn::CollateClause> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        collate: &syn::CollateClause,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Collate(Box::new(Collate::try_from_with_tokens(
            collate, tokens,
        )?)))
    }
}

impl TryFromWithTokens<&syn::SubLink> for Subquery {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        link: &syn::SubLink,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            query: Query::try_from_with_tokens(
                &required(link.get_subselect().as_select_stmt().as_inner(), "subquery")?,
                tokens,
            )?,
            kind: SubqueryKind::try_from(required(
                link.get_sub_link_type().as_inner(),
                "subquery kind",
            )?)?,
            test: Expr::try_from_with_tokens_opt(link.get_testexpr(), tokens)?,
            operator: Name::try_from(link.get_oper_name())?,
        })
    }
}

impl TryFromWithTokens<&syn::SubLink> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        link: &syn::SubLink,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Subquery(Box::new(Subquery::try_from_with_tokens(
            link, tokens,
        )?)))
    }
}

impl TryFromWithTokens<&syn::AArrayExpr> for Vec<Expr> {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::AArrayExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Expr::try_collect_from_with_tokens(value.get_elements(), tokens)
    }
}

impl TryFromWithTokens<&syn::AArrayExpr> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        array: &syn::AArrayExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Array(Vec::<Self>::try_from_with_tokens(
            array, tokens,
        )?))
    }
}

impl TryFromWithTokens<&syn::AIndirection> for Indirection {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        indirection: &syn::AIndirection,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            argument: Expr::try_from_with_tokens(&indirection.get_arg(), tokens)?,
            items: Access::try_collect_from_with_tokens(indirection.get_indirection(), tokens)?,
        })
    }
}

impl TryFromWithTokens<&syn::AIndirection> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        indirection: &syn::AIndirection,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Indirection(Box::new(
            Indirection::try_from_with_tokens(indirection, tokens)?,
        )))
    }
}

impl TryFromWithTokens<&syn::NamedArgExpr> for NamedArgument {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        argument: &syn::NamedArgExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            name: argument.get_name(),
            argument: Expr::try_from_with_tokens(&argument.get_arg(), tokens)?,
            location: source_span(argument.get_location(), tokens),
        })
    }
}

impl TryFromWithTokens<&syn::NamedArgExpr> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        argument: &syn::NamedArgExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::NamedArgument(Box::new(
            NamedArgument::try_from_with_tokens(argument, tokens)?,
        )))
    }
}

impl TryFromWithTokens<&syn::ParamRef> for Parameter {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::ParamRef,
        _tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            position: value.get_number() as u32,
            name: None,
            type_name: None,
        })
    }
}

impl TryFromWithTokens<&syn::ParamRef> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::ParamRef,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Parameter(Parameter::try_from_with_tokens(
            value, tokens,
        )?))
    }
}

impl TryFromWithTokens<&syn::SqlValueFunction> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::SqlValueFunction,
        _tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::SqlValue(SqlValue::try_from(value)?))
    }
}

impl TryFromWithTokens<&syn::MinMaxExpr> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        minmax: &syn::MinMaxExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let name = match minmax.get_op().as_inner() {
            Some(syn::MinMaxOp::IsGreatest) => "greatest",
            Some(syn::MinMaxOp::IsLeast) => "least",
            _ => return AnalysisError::raise_unexpected_input("min/max operation"),
        };
        Ok(Self::Call(Box::new(Call::special(
            name,
            Self::try_collect_from_with_tokens(minmax.get_args(), tokens)?,
        ))))
    }
}

impl TryFromWithTokens<&syn::GroupingFunc> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        grouping: &syn::GroupingFunc,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Grouping(Self::try_collect_from_with_tokens(
            grouping.get_args(),
            tokens,
        )?))
    }
}

impl TryFromWithTokens<&syn::FuncCall> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::FuncCall,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Call(Box::new(Call::try_from_with_tokens(
            value, tokens,
        )?)))
    }
}

impl TryFromWithTokens<&syn::Node> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        node: &syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        crate::sem::with_stack(|| Self::convert_postgres(node, tokens))
    }
}

impl Expr {
    #[inline(never)]
    fn convert_postgres(
        node: &syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        if node.is_a_const() {
            return Ok(Self::Literal(Literal::from(&node.as_a_const())));
        }
        if let Some(column) = node.as_column_ref().as_inner() {
            return Self::try_from_with_tokens(&column, tokens);
        }
        if let Some(call) = node.as_func_call().as_inner() {
            return Ok(Self::Call(Box::new(Call::try_from_with_tokens(
                &call, tokens,
            )?)));
        }
        if let Some(operator) = node.as_a_expr().as_inner() {
            return Self::try_from_with_tokens(&operator, tokens);
        }
        if let Some(boolean) = node.as_bool_expr().as_inner() {
            return Self::try_from_with_tokens(&boolean, tokens);
        }
        if let Some(test) = node.as_null_test().as_inner() {
            return Self::try_from_with_tokens(&test, tokens);
        }
        if let Some(test) = node.as_boolean_test().as_inner() {
            return Self::try_from_with_tokens(&test, tokens);
        }
        if let Some(cast) = node.as_type_cast().as_inner() {
            return Self::try_from_with_tokens(&cast, tokens);
        }
        if let Some(collate) = node.as_collate_clause().as_inner() {
            return Self::try_from_with_tokens(&collate, tokens);
        }
        if let Some(case) = node.as_case_expr().as_inner() {
            return Self::try_from_with_tokens(&case, tokens);
        }
        if let Some(link) = node.as_sub_link().as_inner() {
            return Self::try_from_with_tokens(&link, tokens);
        }
        if let Some(array) = node.as_a_array_expr().as_inner() {
            return Self::try_from_with_tokens(&array, tokens);
        }
        if let Some(row) = node.as_row_expr().as_inner() {
            return Ok(Self::Row(Self::try_collect_from_with_tokens(
                row.get_args(),
                tokens,
            )?));
        }
        if let Some(list) = node.as_list().as_inner() {
            return Ok(Self::List(Self::try_collect_from_with_tokens(
                list.get_items(),
                tokens,
            )?));
        }
        if let Some(indirection) = node.as_a_indirection().as_inner() {
            return Self::try_from_with_tokens(&indirection, tokens);
        }
        if let Some(argument) = node.as_named_arg_expr().as_inner() {
            return Self::try_from_with_tokens(&argument, tokens);
        }
        if let Some(parameter) = node.as_param_ref().as_inner() {
            return Self::try_from_with_tokens(&parameter, tokens);
        }
        if let Some(value) = node.as_sqlvalue_function().as_inner() {
            return Self::try_from_with_tokens(&value, tokens);
        }
        if let Some(coalesce) = node.as_coalesce_expr().as_inner() {
            return Self::try_from_with_tokens(&coalesce, tokens);
        }
        if let Some(minmax) = node.as_min_max_expr().as_inner() {
            return Self::try_from_with_tokens(&minmax, tokens);
        }
        if node.is_set_to_default() {
            return Ok(Self::Default);
        }
        if let Some(cursor) = node.as_current_of_expr().as_inner() {
            return Ok(Self::CurrentOf(super::CurrentOf {
                name: cursor.get_cursor_name(),
                parameter: cursor.get_cursor_param(),
            }));
        }
        if node.is_merge_support_func() {
            return Ok(Self::MergeAction);
        }
        if let Some(document) =
            super::document::DocumentExpr::try_from_with_tokens_opt(node, tokens)?
        {
            return Ok(Self::Document(document));
        }
        if let Some(grouping) = node.as_grouping_func().as_inner() {
            return Self::try_from_with_tokens(&grouping, tokens);
        }
        if let Some(value) = Option::<Literal>::from(node) {
            return Ok(Self::Literal(value));
        }
        AnalysisError::raise_unexpected_input(&format!("expression syntax: {node:?}"))
    }
}

impl TryCollectFromWithTokens<syn::NodeListOpt> for Expr {
    type Error = AnalysisError;

    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        nodes
            .as_inner()
            .unwrap_or_default()
            .into_iter()
            .map(|node| Self::try_from_with_tokens(&node, tokens))
            .collect()
    }
}

impl TryFromWithTokensOpt<syn::Node> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens_opt(
        node: syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Option<Self>, Self::Error> {
        node.as_inner()
            .map(|node| Self::try_from_with_tokens(&node, tokens))
            .transpose()
    }
}

impl From<&syn::AConst> for Literal {
    fn from(value: &syn::AConst) -> Self {
        if let Some(value) = value.as_ival().get_ival() {
            Self::Integer(value)
        } else if let Some(value) = value.as_fval().get_fval() {
            Self::Numeric(value)
        } else if let Some(value) = value.as_boolval().get_boolval() {
            Self::Boolean(value)
        } else if let Some(value) = value.as_sval().get_sval() {
            Self::String(value)
        } else if let Some(value) = value.as_bsval().get_bsval() {
            Self::BitString(value)
        } else {
            Self::Null
        }
    }
}

impl TryFrom<syn::AExprKind> for OperatorKind {
    type Error = AnalysisError;

    fn try_from(kind: syn::AExprKind) -> Result<Self, Self::Error> {
        Ok(match kind {
            syn::AExprKind::AexprOp => Self::Ordinary,
            syn::AExprKind::AexprOpAny => Self::Any,
            syn::AExprKind::AexprOpAll => Self::All,
            syn::AExprKind::AexprDistinct => Self::Distinct,
            syn::AExprKind::AexprNotDistinct => Self::NotDistinct,
            syn::AExprKind::AexprNullif => Self::NullIf,
            syn::AExprKind::AexprIn => Self::In,
            syn::AExprKind::AexprLike => Self::Like,
            syn::AExprKind::AexprIlike => Self::ILike,
            syn::AExprKind::AexprSimilar => Self::Similar,
            syn::AExprKind::AexprBetween => Self::Between,
            syn::AExprKind::AexprNotBetween => Self::NotBetween,
            syn::AExprKind::AexprBetweenSym => Self::BetweenSymmetric,
            syn::AExprKind::AexprNotBetweenSym => Self::NotBetweenSymmetric,
            _ => return AnalysisError::raise_unexpected_input("operator kind"),
        })
    }
}

impl TryFrom<syn::BoolExprType> for BooleanOperator {
    type Error = AnalysisError;

    fn try_from(kind: syn::BoolExprType) -> Result<Self, Self::Error> {
        match kind {
            syn::BoolExprType::AndExpr => Ok(Self::And),
            syn::BoolExprType::OrExpr => Ok(Self::Or),
            syn::BoolExprType::NotExpr => Ok(Self::Not),
            _ => AnalysisError::raise_unexpected_input("boolean operator"),
        }
    }
}

impl TryFrom<syn::SubLinkType> for SubqueryKind {
    type Error = AnalysisError;

    fn try_from(value: syn::SubLinkType) -> Result<Self, Self::Error> {
        match value {
            syn::SubLinkType::ExistsSublink => Ok(Self::Exists),
            syn::SubLinkType::AllSublink => Ok(Self::All),
            syn::SubLinkType::AnySublink => Ok(Self::Any),
            syn::SubLinkType::RowcompareSublink => Ok(Self::Row),
            syn::SubLinkType::ExprSublink => Ok(Self::Scalar),
            syn::SubLinkType::MultiexprSublink => Ok(Self::MultiExpression),
            syn::SubLinkType::ArraySublink => Ok(Self::Array),
            _ => AnalysisError::raise_unexpected_input("subquery kind"),
        }
    }
}

impl TryFromWithTokens<&syn::TypeName> for TypeName {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::TypeName,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let mut bounds = Vec::new();
        for bound in value.get_array_bounds().as_inner().unwrap_or_default() {
            let bound = required(bound.as_integer().get_ival(), "array bound")?;
            bounds.push(if bound < 0 { None } else { Some(bound) });
        }
        Ok(Self {
            name: Name::try_from(value.get_names())?,
            modifiers: Expr::try_collect_from_with_tokens(value.get_typmods(), tokens)?,
            array_bounds: bounds,
            set_of: value.get_setof(),
            percent_type: value.get_pct_type(),
            sqlite_name_snapshot: None,
            sqlite_parts: Vec::new(),
            sqlite_modifiers: None,
            location: source_span(value.get_location(), tokens),
        })
    }
}

#[cfg(test)]
mod test_type_name_conversion {
    use crate::{
        ast::{TryFromWithTokens, TypeName},
        syn::Opt,
        test_helpers::postgres_expression,
    };
    use ::sql_fun_core::IVec;

    #[test]
    fn preserves_modifiers_and_array_dimensions() {
        let node = postgres_expression("NULL::numeric(8, 2)[3][]")
            .as_type_cast()
            .get_type_name()
            .as_inner()
            .unwrap();
        let name = TypeName::try_from_with_tokens(&node, &IVec::default()).unwrap();
        assert_eq!(name.name.0, ["pg_catalog", "numeric"]);
        assert_eq!(name.array_bounds, [Some(3), None]);
        assert_eq!(name.modifiers.len(), 2);
        assert!(!name.set_of);
        assert!(!name.percent_type);
    }
}

fn source_span(location: i32, tokens: &IVec<syn::ScanToken>) -> Option<StringSpan> {
    if location < 0 {
        return None;
    }
    let mut span = None;
    let mut expect_name = true;
    for token in tokens {
        if token.get_start() < location
            || matches!(
                token.get_token().as_inner(),
                Some(syn::Token::SqlComment | syn::Token::CComment)
            )
        {
            continue;
        }
        if expect_name && token.is_ident() || !expect_name && token.is_dot() {
            let next = StringSpan::new(
                token.get_start() as usize,
                (token.get_end() - token.get_start()) as usize,
            );
            if let Some(span) = &mut span {
                StringSpan::extend(span, &next);
            } else {
                span = Some(next);
            }
            expect_name = !expect_name;
        } else {
            break;
        }
    }
    span
}

#[cfg(test)]
mod test_source_span {
    use crate::PostgresDialect;
    use crate::ast::postgres::source_span;

    #[test]
    fn preserves_quoted_qualified_name_and_intervening_comment() -> testresult::TestResult {
        let sql = "select \"schema name\" /* source */ . \"function name\"(1)";
        let tokens = PostgresDialect::scan(sql)?;
        let span = source_span(7, &tokens).expect("function name span");
        assert_eq!(
            span.get_from(sql),
            Some("\"schema name\" /* source */ . \"function name\""),
        );
        assert!(source_span(-1, &tokens).is_none());
        Ok(())
    }
}

fn call_argument_token_span(location: i32, tokens: &IVec<syn::ScanToken>) -> Option<StringSpan> {
    if location < 0 {
        return None;
    }
    for token in tokens {
        if token.get_start() == location {
            return Some(StringSpan::new(
                location as usize,
                (token.get_end() - token.get_start()) as usize,
            ));
        }
    }
    None
}

// Keep absent parser anchors absent rather than guessing an expression location.
fn call_argument_span(node: &syn::Node, tokens: &IVec<syn::ScanToken>) -> Option<StringSpan> {
    if let Some(value) = node.as_column_ref().as_inner() {
        return call_argument_token_span(value.get_location(), tokens);
    }
    if let Some(value) = node.as_param_ref().as_inner() {
        return call_argument_token_span(value.get_location(), tokens);
    }
    if let Some(value) = node.as_func_call().as_inner() {
        return call_argument_token_span(value.get_location(), tokens);
    }
    if let Some(value) = node.as_a_expr().as_inner() {
        return call_argument_token_span(value.get_location(), tokens);
    }
    if let Some(value) = node.as_type_cast().as_inner() {
        return call_argument_token_span(value.get_location(), tokens);
    }
    if let Some(value) = node.as_a_array_expr().as_inner() {
        return call_argument_token_span(value.get_location(), tokens);
    }
    if let Some(value) = node.as_row_expr().as_inner() {
        return call_argument_token_span(value.get_location(), tokens);
    }
    if let Some(value) = node.as_case_expr().as_inner() {
        return call_argument_token_span(value.get_location(), tokens);
    }
    if let Some(value) = node.as_named_arg_expr().as_inner() {
        return call_argument_token_span(value.get_location(), tokens);
    }
    None
}

#[cfg(test)]
mod test_call_argument_span {
    use crate::{PostgresDialect, test_helpers::postgres_expression};

    #[rstest::rstest]
    #[case::column("column_name", Some("column_name"))]
    #[case::parameter("$1", Some("$1"))]
    #[case::call("f(1)", Some("f"))]
    #[case::operator("1 + 2", Some("+"))]
    #[case::cast("1::int", Some("::"))]
    #[case::array("ARRAY[1]", Some("ARRAY"))]
    #[case::row("ROW(1)", Some("ROW"))]
    #[case::case("CASE WHEN true THEN 1 END", Some("CASE"))]
    #[case::literal("42", None)]
    fn uses_parser_anchor(#[case] expression: &str, #[case] expected: Option<&str>) {
        let sql = format!("SELECT {expression}");
        let tokens = PostgresDialect::scan(&sql).unwrap();
        let node = postgres_expression(expression);
        let span = super::call_argument_span(&node, &tokens);
        let text = if let Some(span) = span {
            span.get_from(&sql)
        } else {
            None
        };
        assert_eq!(text, expected);
        assert!(super::call_argument_token_span(-1, &tokens).is_none());
        assert!(super::call_argument_token_span(sql.len() as i32, &tokens).is_none());
    }
}

impl TryFromWithTokens<&syn::FuncCall> for Call {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::FuncCall,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            sql_syntax: matches!(
                value.get_funcformat().as_inner(),
                Some(syn::CoercionForm::CoerceSqlSyntax)
            ),
            name: Name::try_from(value.get_funcname())?,
            kind: CallKind::ordinary(),
            within_order: Vec::new(),
            arguments: Expr::try_collect_from_with_tokens(value.get_args(), tokens)?,
            argument_locations: value
                .get_args()
                .as_inner()
                .unwrap_or_default()
                .into_iter()
                .map(|node| call_argument_span(&node, tokens))
                .collect(),
            star: value.get_agg_star(),
            distinct: value.get_agg_distinct(),
            variadic: value.get_func_variadic(),
            within_group: value.get_agg_within_group(),
            order: Order::try_collect_from_with_tokens(value.get_agg_order(), tokens)?,
            filter: Expr::try_from_with_tokens_opt(value.get_agg_filter(), tokens)?,
            over: value
                .get_over()
                .as_inner()
                .map(|window| Window::try_from_with_tokens(&window, tokens))
                .transpose()?,
            location: source_span(value.get_location(), tokens),
        })
    }
}

impl Call {
    fn special(name: &str, arguments: Vec<Expr>) -> Self {
        Self {
            sql_syntax: false,
            kind: CallKind::special(name),
            name: Name::local(name),
            arguments,
            argument_locations: Vec::new(),
            star: false,
            distinct: false,
            variadic: false,
            within_group: false,
            order: Vec::new(),
            filter: None,
            over: None,
            location: None,
            within_order: Vec::new(),
        }
    }
}

impl TryCollectFromWithTokens<syn::NodeListOpt> for Access {
    type Error = AnalysisError;
    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        nodes
            .as_inner()
            .unwrap_or_default()
            .into_iter()
            .map(|node| Self::try_from_with_tokens(&node, tokens))
            .collect()
    }
}

impl TryFromWithTokens<&syn::AIndices> for Slice {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        indices: &syn::AIndices,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            lower: Expr::try_from_with_tokens_opt(indices.get_lidx(), tokens)?,
            upper: Expr::try_from_with_tokens_opt(indices.get_uidx(), tokens)?,
        })
    }
}

impl TryFromWithTokens<&syn::Node> for Access {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        node: &syn::Node,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        if let Some(indices) = node.as_a_indices().as_inner() {
            if indices.get_is_slice() {
                Ok(Self::Slice(Slice::try_from_with_tokens(&indices, tokens)?))
            } else {
                Ok(Self::Index(Expr::try_from_with_tokens(
                    &indices.get_uidx(),
                    tokens,
                )?))
            }
        } else if node.is_a_star() {
            Ok(Self::Star)
        } else {
            Ok(Self::Field(required(
                node.as_string().get_sval(),
                "field name",
            )?))
        }
    }
}

#[cfg(test)]
mod test_access_conversion {
    use crate::{
        ast::{Access, TryFromWithTokens},
        syn::{ListOpt, Node},
        test_helpers::postgres_expression,
    };
    use ::sql_fun_core::IVec;

    #[rstest::rstest]
    #[case::closed("(items)[1:3]", true, true)]
    #[case::open_lower("(items)[:3]", false, true)]
    #[case::open_upper("(items)[1:]", true, false)]
    #[case::open_both("(items)[:]", false, false)]
    fn preserves_slice_bounds(#[case] sql: &str, #[case] lower: bool, #[case] upper: bool) {
        let node = postgres_expression(sql)
            .as_a_indirection()
            .get_indirection()
            .get(0);
        let Access::Slice(slice) = Access::try_from_with_tokens(&node, &IVec::default()).unwrap()
        else {
            panic!("slice")
        };
        assert_eq!(slice.lower.is_some(), lower);
        assert_eq!(slice.upper.is_some(), upper);
    }

    #[test]
    fn preserves_index_expression() {
        let node = postgres_expression("(items)[2]")
            .as_a_indirection()
            .get_indirection()
            .get(0);
        let access = Access::try_from_with_tokens(&node, &IVec::default()).unwrap();
        assert!(matches!(
            access,
            Access::Index(crate::ast::Expr::Literal(crate::ast::Literal::Integer(2)))
        ));
    }

    #[test]
    fn preserves_field_name() {
        let node = postgres_expression("(item).field")
            .as_a_indirection()
            .get_indirection()
            .get(0);
        let Access::Field(name) = Access::try_from_with_tokens(&node, &IVec::default()).unwrap()
        else {
            panic!("field access")
        };
        assert_eq!(name, "field");
    }

    #[test]
    fn preserves_star() {
        let node = postgres_expression("(item).*")
            .as_a_indirection()
            .get_indirection()
            .get(0);
        assert!(matches!(
            Access::try_from_with_tokens(&node, &IVec::default()).unwrap(),
            Access::Star
        ));
    }

    #[test]
    fn rejects_invalid_field() {
        assert!(Access::try_from_with_tokens(&Node::none(), &IVec::default()).is_err());
    }
}

impl TryFrom<&syn::SqlValueFunction> for SqlValue {
    type Error = AnalysisError;

    fn try_from(value: &syn::SqlValueFunction) -> Result<Self, Self::Error> {
        let (name, precision) = match required(value.get_op().as_inner(), "SQL value")? {
            syn::SqlValueFunctionOp::SvfopCurrentDate => ("current_date", false),
            syn::SqlValueFunctionOp::SvfopCurrentTime => ("current_time", false),
            syn::SqlValueFunctionOp::SvfopCurrentTimeN => ("current_time", true),
            syn::SqlValueFunctionOp::SvfopCurrentTimestamp => ("current_timestamp", false),
            syn::SqlValueFunctionOp::SvfopCurrentTimestampN => ("current_timestamp", true),
            syn::SqlValueFunctionOp::SvfopLocaltime => ("localtime", false),
            syn::SqlValueFunctionOp::SvfopLocaltimeN => ("localtime", true),
            syn::SqlValueFunctionOp::SvfopLocaltimestamp => ("localtimestamp", false),
            syn::SqlValueFunctionOp::SvfopLocaltimestampN => ("localtimestamp", true),
            syn::SqlValueFunctionOp::SvfopCurrentRole => ("current_role", false),
            syn::SqlValueFunctionOp::SvfopCurrentUser => ("current_user", false),
            syn::SqlValueFunctionOp::SvfopUser => ("user", false),
            syn::SqlValueFunctionOp::SvfopSessionUser => ("session_user", false),
            syn::SqlValueFunctionOp::SvfopCurrentCatalog => ("current_catalog", false),
            syn::SqlValueFunctionOp::SvfopCurrentSchema => ("current_schema", false),
            _ => return AnalysisError::raise_unexpected_input("SQL value"),
        };
        Ok(Self {
            name: name.to_owned(),
            precision: if precision {
                Some(value.get_typmod())
            } else {
                None
            },
        })
    }
}

#[cfg(test)]
mod test_sql_value_conversion {
    use crate::{ast::SqlValue, syn::Opt, test_helpers::postgres_expression};

    #[rstest::rstest]
    #[case("current_date", "current_date", None)]
    #[case("current_time", "current_time", None)]
    #[case("current_time(2)", "current_time", Some(2))]
    #[case("current_timestamp", "current_timestamp", None)]
    #[case("current_timestamp(3)", "current_timestamp", Some(3))]
    #[case("localtime", "localtime", None)]
    #[case("localtime(0)", "localtime", Some(0))]
    #[case("localtimestamp", "localtimestamp", None)]
    #[case("localtimestamp(6)", "localtimestamp", Some(6))]
    #[case("current_role", "current_role", None)]
    #[case("current_user", "current_user", None)]
    #[case("user", "user", None)]
    #[case("session_user", "session_user", None)]
    #[case("current_catalog", "current_catalog", None)]
    #[case("current_schema", "current_schema", None)]
    fn preserves_name_and_explicit_precision(
        #[case] sql: &str,
        #[case] name: &str,
        #[case] precision: Option<i32>,
    ) {
        let node = postgres_expression(sql)
            .as_sqlvalue_function()
            .as_inner()
            .unwrap();
        let value = SqlValue::try_from(&node).unwrap();
        assert_eq!(value.name, name);
        assert_eq!(value.precision, precision);
    }
}

impl From<&syn::Node> for Option<Literal> {
    fn from(node: &syn::Node) -> Self {
        if let Some(value) = node.as_string().get_sval() {
            Some(Literal::String(value))
        } else if let Some(value) = node.as_integer().get_ival() {
            Some(Literal::Integer(value))
        } else if let Some(value) = node.as_float().get_fval() {
            Some(Literal::Numeric(value))
        } else {
            node.as_boolean().get_boolval().map(Literal::Boolean)
        }
    }
}

impl TryFromWithTokens<&syn::CaseExpr> for Expr {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        case: &syn::CaseExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let mut arms = Vec::new();
        for arm in case.get_args().as_inner().unwrap_or_default() {
            let arm = required(arm.as_case_when().as_inner(), "case arm")?;
            arms.push(CaseArm {
                condition: Self::try_from_with_tokens(&arm.get_expr(), tokens)?,
                result: Self::try_from_with_tokens(&arm.get_result(), tokens)?,
            });
        }
        Ok(Self::Case(Box::new(Case {
            argument: Self::try_from_with_tokens_opt(case.get_arg(), tokens)?,
            arms,
            otherwise: Self::try_from_with_tokens_opt(case.get_defresult(), tokens)?,
        })))
    }
}

impl TryFromWithTokens<&syn::CoalesceExpr> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::CoalesceExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Call(Box::new(Call::special(
            "coalesce",
            Self::try_collect_from_with_tokens(value.get_args(), tokens)?,
        ))))
    }
}
