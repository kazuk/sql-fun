mod ddl;
mod query;

use crate::ast::{
    Access, AlterOwner, AlterSequence, BooleanExpr, BooleanOperator, Call, CallKind, Case, CaseArm,
    Cast, CastTarget, Collate, Comment, Delete, DomainConstraintTarget, DropTable, EnumType, Expr,
    Extension, FromOpt, Indirection, Insert, Literal, Name, NamedArgument, NullTest,
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

    fn try_from_cast_node(
        object: &syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let cast = CastTarget::try_from_node(object, tokens)?;
        Ok(Self::with_cast(&cast))
    }

    fn try_from_domain_constraint_node(
        object: &syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let target = DomainConstraintTarget::try_from_node(object, tokens)?;
        Ok(Self::with_domain_constraint(&target))
    }

    fn try_from_transform_node(
        object: &syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let target = TransformTarget::try_from_node(object, tokens)?;
        Ok(Self::with_transform(&target))
    }

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

    fn new(type_name: &TypeName, language: &str) -> Self {
        Self {
            type_name: type_name.clone(),
            language: language.to_owned(),
        }
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
    #[case(FullName::with_catalog("catalog", Some("app"), "items"))]
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

impl TryFromWithTokens<&syn::Node> for StatementNode {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
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
        if let Some(trigger) = value.as_create_trig_stmt().as_inner() {
            return Ok(Self::create_trigger(
                super::CreateTrigger::try_from_with_tokens(&trigger, tokens)?,
            ));
        }
        if let Some(rule) = value.as_rule_stmt().as_inner() {
            return Ok(Self::view_write(
                super::view_write::WriteDeclaration::try_from(&rule)?,
            ));
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
                Some(syn::ObjectType::ObjectTrigger | syn::ObjectType::ObjectRule)
            ) {
                return Ok(Self::view_write(
                    super::view_write::WriteDeclaration::try_from(&drop_table)?,
                ));
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
            constraints.push(super::TableConstraint::try_from_with_tokens(
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

impl TryFromWithTokens<&syn::AlterSeqStmt> for AlterSequence {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::AlterSeqStmt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let sequence = required(value.get_sequence().as_inner(), "alter_seq.sequence")?;
        let mut parts = Vec::new();
        if let Some(catalog) = nonempty(sequence.get_catalogname()) {
            parts.push(catalog);
        }
        if let Some(schema) = nonempty(sequence.get_schemaname()) {
            parts.push(schema);
        }
        parts.push(sequence.get_relname());
        Ok(Self::new(
            Name::from_parts(&parts),
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
        let mut parts = Vec::new();
        if let Some(catalog) = nonempty(sequence.get_catalogname()) {
            parts.push(catalog);
        }
        if let Some(schema) = nonempty(sequence.get_schemaname()) {
            parts.push(schema);
        }
        parts.push(sequence.get_relname());
        Ok(Self::new(
            Name::from_parts(&parts),
            SyntaxOption::try_collect_from_with_tokens(value.get_options(), tokens)?,
            value.get_owner_id(),
            value.get_for_identity(),
            value.get_if_not_exists(),
        ))
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
        let mut parts = Vec::new();
        if let Some(catalog) = nonempty(view.get_catalogname()) {
            parts.push(catalog);
        }
        if let Some(schema) = nonempty(view.get_schemaname()) {
            parts.push(schema);
        }
        parts.push(view.get_relname());
        let mut aliases = Vec::new();
        for alias in value.get_aliases().as_inner().unwrap_or_default() {
            aliases.push(required(alias.as_string().get_sval(), "view alias")?);
        }
        Ok(Self::new(
            Name::from_parts(&parts),
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

impl Name {
    #[deprecated = "use try_from"]
    pub(in crate::ast) fn postgres(nodes: syn::NodeListOpt) -> Result<Self, AnalysisError> {
        let mut parts = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            parts.push(required(node.as_string().get_sval(), "name component")?);
        }
        Ok(Self::from_parts(&parts))
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

impl Expr {
    #[deprecated = "use try_from_with_tokens"]
    pub(crate) fn postgres_function(
        value: &syn::FuncCall,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self::Call(Box::new(Call::try_from_with_tokens(
            value, tokens,
        )?)))
    }

    #[deprecated = "use try_from_with_tokens"]
    pub(crate) fn postgres(
        node: &syn::Node,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Self::try_from_with_tokens(node, tokens)
    }

    #[deprecated = "use try_from_with_tokens_opt"]
    pub(in crate::ast) fn postgres_optional(
        node: syn::Node,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Option<Self>, AnalysisError> {
        Self::try_from_with_tokens_opt(node, tokens)
    }

    #[deprecated = "use try_collect_from_with_token"]
    pub(in crate::ast) fn postgres_list(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, AnalysisError> {
        Self::try_collect_from_with_tokens(nodes, tokens)
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

impl Literal {
    #[deprecated = "use from"]
    pub(in crate::ast) fn postgres(value: &syn::AConst) -> Self {
        Self::from(value)
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

impl OperatorKind {
    #[deprecated = "use try_from"]
    pub(in crate::ast) fn postgres(kind: syn::AExprKind) -> Result<Self, AnalysisError> {
        Self::try_from(kind)
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

impl BooleanOperator {
    #[deprecated = "use try_from"]
    pub(in crate::ast) fn postgres(kind: syn::BoolExprType) -> Result<Self, AnalysisError> {
        Self::try_from(kind)
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

impl SubqueryKind {
    #[deprecated = "use try_from"]
    pub(in crate::ast) fn postgres(kind: syn::SubLinkType) -> Result<Self, AnalysisError> {
        Self::try_from(kind)
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

impl TypeName {
    #[deprecated = "use try_from_with_tokens"]
    pub(in crate::ast) fn postgres(
        value: &syn::TypeName,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Self::try_from_with_tokens(value, tokens)
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

    #[deprecated = "use try_from_with_tokens"]
    pub(in crate::ast) fn postgres(
        value: &syn::FuncCall,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Self::try_from_with_tokens(value, tokens)
    }
}

impl TryCollectFromWithTokens<syn::NodeListOpt> for Access {
    type Error = AnalysisError;
    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        let mut items = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            items.push(if let Some(indices) = node.as_a_indices().as_inner() {
                if indices.get_is_slice() {
                    Self::Slice(Slice {
                        lower: Expr::try_from_with_tokens_opt(indices.get_lidx(), tokens)?,
                        upper: Expr::try_from_with_tokens_opt(indices.get_uidx(), tokens)?,
                    })
                } else {
                    Self::Index(Expr::try_from_with_tokens(&indices.get_uidx(), tokens)?)
                }
            } else if node.is_a_star() {
                Self::Star
            } else {
                Self::Field(required(node.as_string().get_sval(), "field name")?)
            });
        }
        Ok(items)
    }
}

impl Access {
    #[deprecated = "use try_collect_from_with_tokens"]
    pub(in crate::ast) fn postgres_list(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, AnalysisError> {
        Self::try_collect_from_with_tokens(nodes, tokens)
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

impl SqlValue {
    #[deprecated = "use SqlValue::try_from"]
    pub(in crate::ast) fn postgres(value: &syn::SqlValueFunction) -> Result<Self, AnalysisError> {
        Self::try_from(value)
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

impl Literal {
    #[deprecated = "use Option::<Literal>::from"]
    fn postgres_option(node: &syn::Node) -> Option<Self> {
        Option::<Self>::from(node)
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

impl Expr {
    #[deprecated = "use try_from_with_tokens"]
    pub(crate) fn postgres_case(
        case: &syn::CaseExpr,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Self::try_from_with_tokens(case, tokens)
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

impl Expr {
    #[deprecated = "use try_from_with_tokens"]
    pub(crate) fn postgres_coalesce(
        value: &syn::CoalesceExpr,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Self::try_from_with_tokens(value, tokens)
    }
}
