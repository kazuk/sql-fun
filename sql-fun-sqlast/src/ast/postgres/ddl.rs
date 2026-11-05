use crate::{
    ast::{
        AlterCommand as AstAlterCommand, AlterRelation, AlterRelationKind as AstAlterRelationKind,
        Column as AstColumn, ColumnConstraint, CommitAction, CompositeType, ConstraintExtras,
        CreateTableMode, ExclusionElement, Expr, ForeignReference, Index,
        IndexExpression as AstIndexExpression, Name, PartitionBound, PartitionDatum, PartitionSpec,
        Relation as AstRelation, Role, StatementNode, SyntaxOption, Table, TableAs,
        TableConstraint as AstTableConstraint, TableExtras, TryCollectFromWithTokens, TryFromOpt,
        TryFromWithTokens, TryFromWithTokensOpt, TypeName, VariableSet,
        ddl::{
            AlterColumnType, AlterConstraintFlags, AlterIdentity, AlterOperation, AlterPartition,
            CompositeField, Function, FunctionBody, FunctionBodyStatement, FunctionOption,
            FunctionParameter, GenericAction, GenericOption, ParameterMode, PostgresAlterCommand,
            ReplicaIdentity, Schema,
        },
        postgres::{nonempty, required},
    },
    error::AnalysisError,
    syn::{self, ListOpt, Opt},
};

impl TryFrom<syn::ObjectType> for AstAlterRelationKind {
    type Error = AnalysisError;
    fn try_from(value: syn::ObjectType) -> Result<Self, Self::Error> {
        Ok(match value {
            syn::ObjectType::ObjectTable => Self::Table,
            syn::ObjectType::ObjectForeignTable => Self::ForeignTable,
            syn::ObjectType::ObjectIndex => Self::Index,
            syn::ObjectType::ObjectType => Self::CompositeType,
            syn::ObjectType::ObjectView => Self::View,
            syn::ObjectType::ObjectMatview => Self::MaterializedView,
            syn::ObjectType::ObjectSequence => Self::Sequence,
            _ => return AnalysisError::raise_unexpected_input("alter relation kind"),
        })
    }
}

impl TryFromWithTokens<&syn::RenameStmt> for AlterRelation {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::RenameStmt,
        _tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let relation = required(value.get_relation().as_inner(), "rename.relation")?;
        let (kind, command, only) =
            match required(value.get_rename_type().as_inner(), "rename.rename_type")? {
                syn::ObjectType::ObjectTable => (
                    AstAlterRelationKind::table(),
                    AstAlterCommand::Rename(Name::from_parts(&[value.get_newname()])),
                    false,
                ),
                syn::ObjectType::ObjectColumn => (
                    AstAlterRelationKind::try_from(required(
                        value.get_relation_type().as_inner(),
                        "rename.relation_type",
                    )?)?,
                    AstAlterCommand::RenameColumn(value.get_subname(), value.get_newname()),
                    !relation.get_inh(),
                ),
                _ => return AnalysisError::raise_unexpected_input("expected relation rename"),
            };
        Ok(Self {
            only: kind.supports_only() && only,
            name: Name::from(&relation),
            kind,
            commands: vec![command],
            if_exists: value.get_missing_ok(),
        })
    }
}

impl TryFromWithTokens<&syn::AlterTableStmt> for AlterRelation {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::AlterTableStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        use crate::ast::{AlterCommand, AlterRelationKind, Relation};
        let relation = required(value.get_relation().as_inner(), "alter_table.relation")?;
        let kind = AlterRelationKind::try_from(required(
            value.get_objtype().as_inner(),
            "alter_table.objtype",
        )?)?;
        let mut commands = Vec::new();
        for command in value
            .get_cmds()
            .map(|commands| commands.as_alter_table_cmd())
            .unwrap_or_default()
        {
            commands.push(AlterCommand::try_from_with_tokens(&command, tokens)?);
        }
        Ok(Self {
            only: kind.supports_only() && !relation.get_inh(),
            name: Relation::try_from(&relation)?.name,
            kind,
            commands,
            if_exists: value.get_missing_ok(),
        })
    }
}

impl TryFromWithTokens<&syn::AlterTableCmd> for AstAlterCommand {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::AlterTableCmd,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::postgres(PostgresAlterCommand::try_from_with_tokens(
            value, tokens,
        )?))
    }
}

impl AstAlterCommand {
    fn postgres_legacy(
        value: &syn::AlterTableCmd,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Option<Self>, AnalysisError> {
        use crate::ast::{Column, TableConstraint};
        Ok(Some(
            match required(value.get_subtype().as_inner(), "alter_table_cmd.subtype")? {
                syn::AlterTableType::AtChangeOwner => Self::Owner(Role::try_from(&required(
                    value.get_newowner().as_inner(),
                    "alter_table_cmd.newowner",
                )?)?),
                syn::AlterTableType::AtColumnDefault => Self::Default(
                    value.get_name(),
                    if value.get_def().as_inner().is_some() {
                        Expr::try_from_with_tokens(&value.get_def(), tokens)?
                    } else {
                        return Ok(None);
                    },
                ),
                syn::AlterTableType::AtAddConstraint => {
                    Self::Constraint(TableConstraint::try_from_with_tokens(
                        &required(
                            value.get_def().as_constraint().as_inner(),
                            "alter_table_cmd.constraint",
                        )?,
                        tokens,
                    )?)
                }
                syn::AlterTableType::AtClusterOn => Self::Cluster(value.get_name()),
                syn::AlterTableType::AtAddColumn => Self::AddColumn(Column::try_from_with_tokens(
                    &required(
                        value.get_def().as_column_def().as_inner(),
                        "alter_table_cmd.column",
                    )?,
                    tokens,
                )?),
                syn::AlterTableType::AtDropColumn => Self::DropColumn(value.get_name()),
                _ => return Ok(None),
            },
        ))
    }
}

impl TryFromWithTokens<&syn::CreateStmt> for Table {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::CreateStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        use crate::ast::{Column, Relation, TableConstraint};
        let relation = required(value.get_relation().as_inner(), "create_table.relation")?;
        let relation = Relation::try_from(&relation)?;
        let mut columns = Vec::new();
        let mut constraints = Vec::new();
        for element in value.get_table_elts().as_inner().unwrap_or_default() {
            if let Some(column) = element.as_column_def().as_inner() {
                columns.push(Column::try_from_with_tokens(&column, tokens)?);
            } else if let Some(constraint) = element.as_constraint().as_inner() {
                constraints.push(TableConstraint::try_from_with_tokens(&constraint, tokens)?);
            } else {
                return AnalysisError::raise_unexpected_input("create table element");
            }
        }
        Ok(Self {
            extras: TableExtras::try_from_with_tokens(value, tokens)?,
            name: relation.name,
            columns,
            constraints,
            temporary: relation.persistence == "t",
            mode: CreateTableMode::from_if_not_exists(value.get_if_not_exists()),
            query: None,
            options: SyntaxOption::try_collect_from_with_tokens(value.get_options(), tokens)?,
        })
    }
}

impl TryFromWithTokens<&syn::ColumnDef> for AstColumn {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::ColumnDef,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        use crate::ast::TableConstraint;
        let mut type_mod = None;
        let type_name = if let Some(type_name) = value.get_type_name().as_inner() {
            let type_mod_value = type_name.get_typemod();
            if type_mod_value != 0 {
                type_mod = Some(type_mod_value);
            }
            Some(TypeName::try_from_with_tokens(&type_name, tokens)?)
        } else {
            None
        };

        let collation = if let Some(clause) = value.get_coll_clause().as_inner() {
            Name::try_from(clause.get_collname())?
        } else {
            Name::default()
        };
        let mut constraints = Vec::new();
        for constraint in value
            .get_constraints()
            .map(|nodes| nodes.as_constraint())
            .unwrap_or_default()
        {
            constraints.push(TableConstraint::from_postgres_inline(&constraint, tokens)?);
        }
        Ok(Self {
            collation,
            compression: nonempty(value.get_compression()),
            storage: nonempty(value.get_storage_name()),
            name: value.get_colname(),
            type_mod,
            type_name,
            constraints,
            not_null: Some(value.get_is_not_null()),
        })
    }
}

impl TryFromWithTokens<&syn::Constraint> for AstTableConstraint {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::Constraint,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let keys = if matches!(
            value.get_contype().as_inner(),
            Some(syn::ConstrType::ConstrForeign)
        ) {
            value.get_fk_attrs()
        } else {
            value.get_keys()
        };
        Ok(Self {
            extras: ConstraintExtras::try_from_with_tokens(value, tokens)?,
            name: nonempty(value.get_conname()),
            keys: Name::try_from(keys)?.0,
            order: Vec::new(),
            auto_increment: false,
            constraint: ColumnConstraint::try_from_with_tokens(value, tokens)?,
        })
    }
}

impl TryFromWithTokens<&syn::Constraint> for ColumnConstraint {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::Constraint,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        use crate::{
            ast::ForeignReference,
            ast::{Deferrable, GeneratedColumnMode, IdentityColumnMode, Validity},
        };
        let kind = required(value.get_contype().as_inner(), "constraint.contype")?;
        Ok(match kind {
            syn::ConstrType::ConstrAttrEnforced => Self::Enforcement(true),
            syn::ConstrType::ConstrAttrNotEnforced => Self::Enforcement(false),
            syn::ConstrType::ConstrNull => {
                Self::Nullability(false, None, Some(Validity::from_constraint(value)))
            }
            syn::ConstrType::ConstrNotnull => {
                Self::Nullability(true, None, Some(Validity::from_constraint(value)))
            }
            syn::ConstrType::ConstrDefault => Self::Default(Expr::try_from_with_tokens(
                &required(value.get_raw_expr().as_inner(), "constraint.raw_expr")?,
                tokens,
            )?),
            syn::ConstrType::ConstrCheck => Self::Check(Expr::try_from_with_tokens(
                &required(value.get_raw_expr().as_inner(), "constraint.raw_expr")?,
                tokens,
            )?),
            syn::ConstrType::ConstrPrimary => {
                Self::PrimaryKey(None, Deferrable::try_from_constraint(value)?)
            }
            syn::ConstrType::ConstrUnique => Self::Unique(None),
            syn::ConstrType::ConstrIdentity => Self::Identity(
                IdentityColumnMode::from_postgres_generate_when(&value.get_generated_when())?,
            ),
            syn::ConstrType::ConstrGenerated => Self::Generated(
                Expr::try_from_with_tokens(
                    &required(value.get_raw_expr().as_inner(), "constraint.raw_expr")?,
                    tokens,
                )?,
                Some(match value.get_generated_kind().as_str() {
                    "s" => GeneratedColumnMode::stored(),
                    "v" => GeneratedColumnMode::r#virtual(),
                    _ => return AnalysisError::raise_unexpected_input("raw generated-column mode"),
                }),
            ),
            syn::ConstrType::ConstrForeign => Self::ForeignKey(ForeignReference::try_from(value)?),
            syn::ConstrType::ConstrExclusion => Self::Exclusion,
            syn::ConstrType::ConstrAttrDeferrable => Self::Deferrability(true),
            syn::ConstrType::ConstrAttrNotDeferrable => Self::Deferrability(false),
            syn::ConstrType::ConstrAttrDeferred => Self::Deferred,
            syn::ConstrType::ConstrAttrImmediate => Self::Immediate,
            _ => return AnalysisError::raise_unexpected_input("constraint.contype"),
        })
    }
}

impl TryFrom<&syn::Constraint> for ForeignReference {
    type Error = AnalysisError;
    fn try_from(value: &syn::Constraint) -> Result<Self, Self::Error> {
        use crate::{
            ast::Relation,
            ast::{Deferrability, ForeignKeyAction, ForeignKeyMatchType, Validity},
        };
        let table = required(value.get_pktable().as_inner(), "constraint.pktable")?;
        Ok(Self {
            table: Relation::try_from(&table)?.name,
            columns: Name::try_from(value.get_pk_attrs())?.0,
            match_type: ForeignKeyMatchType::try_from_postgres_constraint(value)?,
            delete_action: ForeignKeyAction::try_from_delete_postgres_constraint(value)?,
            update_action: ForeignKeyAction::try_from_update_postgres_constraint(value)?,
            deferrability: Some(Deferrability::new(
                value.get_deferrable(),
                value.get_initdeferred(),
            )),
            validity: Some(Validity::from_constraint(value)),
            ignored_matches: Vec::new(),
        })
    }
}

use ::sql_fun_core::IVec;

impl TryFromWithTokens<&syn::CreateTableAsStmt> for TableAs {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::CreateTableAsStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        use crate::ast::{Into, Query};
        let into = required(value.get_into().as_inner(), "create_table_as.into")?;
        let query = required(
            value.get_query().as_select_stmt().as_inner(),
            "create_table_as.query",
        )?;
        let kind = required(value.get_objtype().as_inner(), "create_table_as.objtype")?;
        Ok(Self::new(
            Into::try_from_with_tokens(&into, tokens)?,
            Query::try_from_with_tokens(&query, tokens)?,
            matches!(kind, syn::ObjectType::ObjectMatview),
            value.get_if_not_exists(),
        )
        .with_skip_data(into.get_skip_data()))
    }
}

impl TryFromWithTokens<&syn::IndexStmt> for Index {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::IndexStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        use crate::ast::{IndexExpression, Relation};
        let relation = required(value.get_relation().as_inner(), "index.relation")?;
        let mut keys = Vec::new();
        for key in value
            .get_index_params()
            .map(|keys| keys.as_index_elem())
            .unwrap_or_default()
        {
            keys.push(IndexExpression::try_from_with_tokens(&key, tokens)?);
        }
        let mut includes = Vec::new();
        for key in value
            .get_index_including_params()
            .map(|keys| keys.as_index_elem())
            .unwrap_or_default()
        {
            includes.push(IndexExpression::try_from_with_tokens(&key, tokens)?);
        }
        Ok(Self::new(
            &value.get_idxname(),
            Relation::try_from(&relation)?,
            keys,
            includes,
            value.get_unique(),
            Expr::try_from_with_tokens_opt(value.get_where_clause(), tokens)?,
            SyntaxOption::try_collect_from_with_tokens(value.get_options(), tokens)?,
            &value.get_access_method(),
        )
        .with_syntax_options(
            value.get_concurrent(),
            value.get_if_not_exists(),
            value.get_nulls_not_distinct(),
            &value.get_table_space(),
        ))
    }
}

impl TryFromWithTokens<&syn::CreateSchemaStmt> for Schema {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::CreateSchemaStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let mut elements = Vec::new();
        for element in value.get_schema_elts().as_inner().unwrap_or_default() {
            elements.push(StatementNode::try_from_with_tokens(&element, tokens)?);
        }
        Ok(
            Self::new(&value.get_schemaname(), elements, value.get_if_not_exists())
                .with_authorization(value.get_authrole().as_inner().map(|role| {
                    Role::new(
                        role.get_roletype().as_inner().map_or(0, |kind| kind as u32),
                        &role.get_rolename(),
                    )
                })),
        )
    }
}

impl TryFromWithTokens<&syn::CreateFunctionStmt> for Function {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::CreateFunctionStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let mut parameters = Vec::new();
        for param in value
            .get_parameters()
            .map(|nodes| nodes.as_function_parameter())
            .unwrap_or_default()
        {
            parameters.push(FunctionParameter::try_from_with_tokens(&param, tokens)?);
        }
        let result_type = if let Some(type_name) = value.get_return_type().as_inner() {
            Some(TypeName::try_from_with_tokens(&type_name, tokens)?)
        } else {
            None
        };
        let body = if let Some(node) = value.get_sql_body().as_inner() {
            Some(FunctionBody::try_from_with_tokens(&node, tokens)?)
        } else {
            None
        };
        Ok(Self::new(
            Name::try_from(value.get_funcname())?,
            parameters,
            result_type,
            FunctionOption::try_collect_from_with_tokens(value.get_options(), tokens)?,
            value.get_replace(),
            value.get_is_procedure(),
        )
        .with_body(body))
    }
}

impl TryFromWithTokens<&syn::FunctionParameter> for FunctionParameter {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::FunctionParameter,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let type_name = required(
            value.get_arg_type().as_inner(),
            "function_parameter.arg_type",
        )?;
        Ok(Self::new(
            &value.get_name(),
            TypeName::try_from_with_tokens(&type_name, tokens)?,
            ParameterMode::try_from(required(
                value.get_mode().as_inner(),
                "function_parameter.mode",
            )?)?,
            Expr::try_from_with_tokens_opt(value.get_defexpr(), tokens)?,
        ))
    }
}

impl TryFrom<syn::FunctionParameterMode> for ParameterMode {
    type Error = AnalysisError;
    fn try_from(value: syn::FunctionParameterMode) -> Result<Self, Self::Error> {
        match value {
            syn::FunctionParameterMode::FuncParamIn
            | syn::FunctionParameterMode::FuncParamDefault => Ok(Self::In),
            syn::FunctionParameterMode::FuncParamOut => Ok(Self::Out),
            syn::FunctionParameterMode::FuncParamInout => Ok(Self::InOut),
            syn::FunctionParameterMode::FuncParamVariadic => Ok(Self::Variadic),
            syn::FunctionParameterMode::FuncParamTable => Ok(Self::Table),
            _ => AnalysisError::raise_unexpected_input("function_parameter.mode"),
        }
    }
}

impl TryCollectFromWithTokens<syn::NodeListOpt> for FunctionOption {
    type Error = AnalysisError;
    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        let mut options = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            let option = required(node.as_def_elem().as_inner(), "function option")?;
            options.push(Self::try_from_with_tokens(&option, tokens)?);
        }
        Ok(options)
    }
}

impl TryFromWithTokens<&syn::DefElem> for FunctionOption {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::DefElem,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let argument = value.get_arg();
        Ok(match value.get_defname().as_str() {
            "as" => Self::Body(Self::string_list(argument, "function AS")?),
            "language" => Self::Language(required(
                argument.as_string().get_sval(),
                "function language",
            )?),
            "transform" => Self::Transform(Self::type_list(argument, tokens)?),
            "window" => Self::Window,
            "volatility" => Self::Volatility(required(
                argument.as_string().get_sval(),
                "function volatility",
            )?),
            "leakproof" => Self::Leakproof(Self::boolean_argument(&argument)?),
            "strict" => Self::Strict(Self::boolean_argument(&argument)?),
            "security" => Self::SecurityDefiner(Self::boolean_argument(&argument)?),
            "parallel" => Self::Parallel(required(
                argument.as_string().get_sval(),
                "function parallel mode",
            )?),
            "cost" => Self::Cost(Expr::try_from_with_tokens(&argument, tokens)?),
            "rows" => Self::Rows(Expr::try_from_with_tokens(&argument, tokens)?),
            "support" => Self::Support(Name::try_from(argument.as_list().get_items())?),
            "set" => Self::Setting(VariableSet::try_from_with_tokens(
                &required(
                    argument.as_variable_set_stmt().as_inner(),
                    "function setting",
                )?,
                tokens,
            )?),
            _ => return AnalysisError::raise_unexpected_input("function option"),
        })
    }
}

impl TryFromWithTokens<&syn::Node> for FunctionBody {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        node: &syn::Node,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        if let Some(return_statement) = node.as_return_stmt().as_inner() {
            return Ok(Self::Return(Expr::try_from_with_tokens(
                &return_statement.get_returnval(),
                tokens,
            )?));
        }
        let lists = required(node.as_list().get_items().as_inner(), "SQL function body")?;
        let mut statements = Vec::new();
        for list in lists {
            let body = required(list.as_list().as_inner(), "SQL function statement list")?
                .get_items()
                .as_inner()
                .unwrap_or_default();
            for statement in body {
                statements.push(FunctionBodyStatement::try_from_with_tokens(
                    &statement, tokens,
                )?);
            }
        }
        Ok(Self::Atomic(statements))
    }
}

impl TryFromWithTokens<&syn::Node> for FunctionBodyStatement {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        node: &syn::Node,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        if let Some(return_statement) = node.as_return_stmt().as_inner() {
            Ok(Self::return_expression(Expr::try_from_with_tokens(
                &return_statement.get_returnval(),
                tokens,
            )?))
        } else {
            Ok(Self::statement(StatementNode::try_from_with_tokens(
                node, tokens,
            )?))
        }
    }
}

impl TryFromWithTokens<&syn::CompositeTypeStmt> for CompositeType {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::CompositeTypeStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let relation = required(value.get_typevar().as_inner(), "composite type name")?;
        let mut fields = Vec::new();
        for field in value
            .get_coldeflist()
            .map(|fields| fields.as_column_def())
            .unwrap_or_default()
        {
            fields.push(CompositeField::try_from_with_tokens(&field, tokens)?);
        }
        Ok(Self::new(&AstRelation::try_from(&relation)?.name, &fields))
    }
}

impl TryFromWithTokens<&syn::ColumnDef> for CompositeField {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::ColumnDef,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let collation = if let Some(collation) = value.get_coll_clause().as_inner() {
            Some(Name::try_from(collation.get_collname())?)
        } else {
            None
        };
        Ok(Self::new(
            &value.get_colname(),
            &TypeName::try_from_with_tokens(
                &required(value.get_type_name().as_inner(), "composite field type")?,
                tokens,
            )?,
            collation.as_ref(),
            value.get_is_not_null(),
        ))
    }
}

impl TryFrom<syn::AlterTableType> for AlterOperation {
    type Error = AnalysisError;
    fn try_from(value: syn::AlterTableType) -> Result<Self, Self::Error> {
        use syn::AlterTableType as Source;
        Ok(match value {
            Source::AtAddColumn => Self::AddColumn,
            Source::AtColumnDefault => Self::ColumnDefault,
            Source::AtDropNotNull => Self::DropNotNull,
            Source::AtSetNotNull => Self::SetNotNull,
            Source::AtSetExpression => Self::SetExpression,
            Source::AtDropExpression => Self::DropExpression,
            Source::AtSetStatistics => Self::SetStatistics,
            Source::AtSetOptions => Self::SetOptions,
            Source::AtResetOptions => Self::ResetOptions,
            Source::AtSetStorage => Self::SetStorage,
            Source::AtSetCompression => Self::SetCompression,
            Source::AtDropColumn => Self::DropColumn,
            Source::AtAddConstraint => Self::AddConstraint,
            Source::AtAlterConstraint => Self::AlterConstraint,
            Source::AtValidateConstraint => Self::ValidateConstraint,
            Source::AtDropConstraint => Self::DropConstraint,
            Source::AtAlterColumnType => Self::AlterColumnType,
            Source::AtAlterColumnGenericOptions => Self::AlterColumnGenericOptions,
            Source::AtChangeOwner => Self::Owner,
            Source::AtClusterOn => Self::Cluster,
            Source::AtDropCluster => Self::DropCluster,
            Source::AtSetLogged => Self::SetLogged,
            Source::AtSetUnLogged => Self::SetUnlogged,
            Source::AtDropOids => Self::DropOids,
            Source::AtSetAccessMethod => Self::SetAccessMethod,
            Source::AtSetTableSpace => Self::SetTablespace,
            Source::AtSetRelOptions => Self::SetRelOptions,
            Source::AtResetRelOptions => Self::ResetRelOptions,
            Source::AtEnableTrig => Self::EnableTrigger,
            Source::AtEnableAlwaysTrig => Self::EnableAlwaysTrigger,
            Source::AtEnableReplicaTrig => Self::EnableReplicaTrigger,
            Source::AtDisableTrig => Self::DisableTrigger,
            Source::AtEnableTrigAll => Self::EnableTriggerAll,
            Source::AtDisableTrigAll => Self::DisableTriggerAll,
            Source::AtEnableTrigUser => Self::EnableTriggerUser,
            Source::AtDisableTrigUser => Self::DisableTriggerUser,
            Source::AtEnableRule => Self::EnableRule,
            Source::AtEnableAlwaysRule => Self::EnableAlwaysRule,
            Source::AtEnableReplicaRule => Self::EnableReplicaRule,
            Source::AtDisableRule => Self::DisableRule,
            Source::AtAddInherit => Self::AddInherit,
            Source::AtDropInherit => Self::DropInherit,
            Source::AtAddOf => Self::AddOf,
            Source::AtDropOf => Self::DropOf,
            Source::AtReplicaIdentity => Self::ReplicaIdentity,
            Source::AtEnableRowSecurity => Self::EnableRowSecurity,
            Source::AtDisableRowSecurity => Self::DisableRowSecurity,
            Source::AtForceRowSecurity => Self::ForceRowSecurity,
            Source::AtNoForceRowSecurity => Self::NoForceRowSecurity,
            Source::AtGenericOptions => Self::GenericOptions,
            Source::AtAttachPartition => Self::AttachPartition,
            Source::AtDetachPartition => Self::DetachPartition,
            Source::AtDetachPartitionFinalize => Self::DetachPartitionFinalize,
            Source::AtAddIdentity => Self::AddIdentity,
            Source::AtSetIdentity => Self::SetIdentity,
            Source::AtDropIdentity => Self::DropIdentity,
            _ => {
                return AnalysisError::raise_unexpected_input(
                    "ALTER action is not present in raw SQL syntax",
                );
            }
        })
    }
}

impl TryFromWithTokens<&syn::AlterTableCmd> for PostgresAlterCommand {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::AlterTableCmd,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let operation =
            AlterOperation::try_from(required(value.get_subtype().as_inner(), "ALTER action")?)?;
        let definition = operation.convert_definition(value, tokens)?;
        Ok(Self::new(
            operation,
            &value.get_name(),
            value.get_num(),
            &definition,
            value.get_missing_ok(),
            matches!(
                value.get_behavior().as_inner(),
                Some(syn::DropBehavior::DropCascade)
            ),
            AstAlterCommand::postgres_legacy(value, tokens)?.as_ref(),
        ))
    }
}

impl TryFromWithTokens<&syn::ColumnDef> for AlterColumnType {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::ColumnDef,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let collation = if let Some(collation) = value.get_coll_clause().as_inner() {
            Some(Name::try_from(collation.get_collname())?)
        } else {
            None
        };
        Ok(Self::new(
            &TypeName::try_from_with_tokens(
                &required(value.get_type_name().as_inner(), "altered column type")?,
                tokens,
            )?,
            collation.as_ref(),
            Expr::try_from_with_tokens_opt(value.get_raw_default(), tokens)?.as_ref(),
        ))
    }
}

impl TryFrom<&syn::AtAlterConstraint> for AlterConstraintFlags {
    type Error = AnalysisError;
    fn try_from(value: &syn::AtAlterConstraint) -> Result<Self, Self::Error> {
        Ok(Self {
            name: value.get_conname(),
            deferrability: if value.get_alter_deferrability() {
                Some((value.get_deferrable(), value.get_initdeferred()))
            } else {
                None
            },
            enforced: if value.get_alter_enforceability() {
                Some(value.get_is_enforced())
            } else {
                None
            },
            no_inherit: if value.get_alter_inheritability() {
                Some(value.get_noinherit())
            } else {
                None
            },
        })
    }
}

impl TryFrom<&syn::RoleSpec> for Role {
    type Error = AnalysisError;
    fn try_from(value: &syn::RoleSpec) -> Result<Self, Self::Error> {
        Ok(Self::new(
            required(value.get_roletype().as_inner(), "role kind")? as u32,
            &value.get_rolename(),
        ))
    }
}

impl TryFrom<&syn::ReplicaIdentityStmt> for ReplicaIdentity {
    type Error = AnalysisError;
    fn try_from(value: &syn::ReplicaIdentityStmt) -> Result<Self, Self::Error> {
        Ok(Self::new(&value.get_identity_type(), &value.get_name()))
    }
}

impl TryFromWithTokens<&syn::PartitionCmd> for AlterPartition {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::PartitionCmd,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let relation = AstRelation::try_from(&required(
            value.get_name().as_inner(),
            "partition relation",
        )?)?
        .name;
        let bound = if let Some(bound) = value.get_bound().as_inner() {
            Some(PartitionBound::try_from_with_tokens(&bound, tokens)?)
        } else {
            None
        };
        Ok(Self::new(&relation, bound.as_ref(), value.get_concurrent()))
    }
}

impl TryFromWithTokens<&syn::Constraint> for AlterIdentity {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::Constraint,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::new(
            &value.get_generated_when(),
            &SyntaxOption::try_collect_from_with_tokens(value.get_options(), tokens)?,
        ))
    }
}

impl TryCollectFromWithTokens<syn::NodeListOpt> for GenericOption {
    type Error = AnalysisError;
    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        let mut options = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            options.push(Self::try_from_with_tokens(
                &required(node.as_def_elem().as_inner(), "generic option")?,
                tokens,
            )?);
        }
        Ok(options)
    }
}

impl TryFromWithTokens<&syn::DefElem> for GenericOption {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::DefElem,
        _tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let action = GenericAction::try_from(required(
            value.get_defaction().as_inner(),
            "generic option action",
        )?)?;
        Ok(Self::new(
            action,
            &value.get_defname(),
            value.get_arg().as_string().get_sval().as_deref(),
        ))
    }
}

impl TryFrom<syn::DefElemAction> for GenericAction {
    type Error = AnalysisError;
    fn try_from(value: syn::DefElemAction) -> Result<Self, Self::Error> {
        Ok(match value {
            syn::DefElemAction::DefelemUnspec => Self::Unspecified,
            syn::DefElemAction::DefelemAdd => Self::Add,
            syn::DefElemAction::DefelemSet => Self::Set,
            syn::DefElemAction::DefelemDrop => Self::Drop,
            _ => return AnalysisError::raise_unexpected_input("generic option action"),
        })
    }
}

impl TryFromWithTokens<&syn::CreateStmt> for TableExtras {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        v: &syn::CreateStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let mut inherits = Vec::new();
        for node in v.get_inh_relations().as_inner().unwrap_or_default() {
            let relation = required(node.as_range_var().as_inner(), "table inheritance relation")?;
            inherits.push(AstRelation::try_from(&relation)?.name);
        }
        let of_type = if let Some(type_name) = v.get_of_typename().as_inner() {
            Some(TypeName::try_from_with_tokens(&type_name, tokens)?)
        } else {
            None
        };
        let partition_spec = if let Some(spec) = v.get_partspec().as_inner() {
            Some(PartitionSpec::try_from_with_tokens(&spec, tokens)?)
        } else {
            None
        };
        let partition_bound = if let Some(bound) = v.get_partbound().as_inner() {
            Some(PartitionBound::try_from_with_tokens(&bound, tokens)?)
        } else {
            None
        };
        Ok(Self {
            unlogged: v.get_relation().get_relpersistence().as_deref() == Some("u"),
            inherits,
            of_type,
            on_commit: CommitAction::try_from_opt(v.get_oncommit().as_inner())?,
            tablespace: nonempty(v.get_tablespacename()),
            access_method: nonempty(v.get_access_method()),
            partition_spec,
            partition_bound,
        })
    }
}
impl TryFromWithTokens<&syn::PartitionSpec> for PartitionSpec {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        v: &syn::PartitionSpec,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let strategy = match required(v.get_strategy().as_inner(), "partition strategy")? {
            syn::PartitionStrategy::List => "LIST",
            syn::PartitionStrategy::Range => "RANGE",
            syn::PartitionStrategy::Hash => "HASH",
            _ => return AnalysisError::raise_unexpected_input("partition strategy"),
        };
        let mut keys = Vec::new();
        for node in v.get_part_params().as_inner().unwrap_or_default() {
            let p = required(node.as_partition_elem().as_inner(), "partition key")?;
            keys.push(AstIndexExpression::try_from_with_tokens(&p, tokens)?);
        }
        Ok(Self {
            strategy: strategy.to_owned(),
            keys,
        })
    }
}

impl TryFromWithTokens<&syn::PartitionElem> for AstIndexExpression {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::PartitionElem,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            name: nonempty(value.get_name()),
            alias: None,
            expression: Expr::try_from_with_tokens_opt(value.get_expr(), tokens)?,
            collation: Name::try_from(value.get_collation())?,
            operator_class: Name::try_from(value.get_opclass())?,
            options: Vec::new(),
            descending: None,
            using_order: false,
            nulls_first: None,
        })
    }
}

impl TryFromWithTokens<&syn::PartitionBoundSpec> for PartitionBound {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        v: &syn::PartitionBoundSpec,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            strategy: v.get_strategy(),
            is_default: v.get_is_default(),
            modulus: v.get_modulus(),
            remainder: v.get_remainder(),
            list: Expr::try_collect_from_with_tokens(v.get_listdatums(), tokens)?,
            lower: PartitionDatum::try_collect_from_with_tokens(v.get_lowerdatums(), tokens)?,
            upper: PartitionDatum::try_collect_from_with_tokens(v.get_upperdatums(), tokens)?,
        })
    }
}
impl TryFromWithTokens<&syn::Node> for PartitionDatum {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        v: &syn::Node,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let Some(d) = v.as_partition_range_datum().as_inner() else {
            if let Some(column) = v.as_column_ref().as_inner() {
                let name = Name::try_from(column.get_fields())?;
                if name.0.as_slice() == ["minvalue"] {
                    return Ok(Self::Min);
                }
                if name.0.as_slice() == ["maxvalue"] {
                    return Ok(Self::Max);
                }
            }
            return Ok(Self::Value(Expr::try_from_with_tokens(v, tokens)?));
        };
        Ok(
            match required(d.get_kind().as_inner(), "partition datum kind")? {
                syn::PartitionRangeDatumKind::PartitionRangeDatumMinvalue => Self::Min,
                syn::PartitionRangeDatumKind::PartitionRangeDatumMaxvalue => Self::Max,
                syn::PartitionRangeDatumKind::PartitionRangeDatumValue => {
                    Self::Value(Expr::try_from_with_tokens(
                        &required(d.get_value().as_inner(), "partition value")?,
                        tokens,
                    )?)
                }
                _ => return AnalysisError::raise_unexpected_input("partition datum kind"),
            },
        )
    }
}
impl TryFromWithTokens<&syn::Constraint> for ConstraintExtras {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        v: &syn::Constraint,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let mut exclusions = Vec::new();
        for node in v.get_exclusions().as_inner().unwrap_or_default() {
            exclusions.push(ExclusionElement::try_from_with_tokens(&node, tokens)?);
        }
        Ok(Self {
            enforced: Self::explicit_enforcement(v, tokens),
            without_overlaps: v.get_without_overlaps(),
            fk_with_period: v.get_fk_with_period(),
            pk_with_period: v.get_pk_with_period(),
            deferrable: v.get_deferrable(),
            initially_deferred: v.get_initdeferred(),
            skip_validation: v.get_skip_validation(),
            no_inherit: v.get_is_no_inherit(),
            nulls_not_distinct: v.get_nulls_not_distinct(),
            include: Name::try_from(v.get_including())?.0,
            options: SyntaxOption::try_collect_from_with_tokens(v.get_options(), tokens)?,
            index_name: nonempty(v.get_indexname()),
            tablespace: nonempty(v.get_indexspace()),
            access_method: nonempty(v.get_access_method()),
            predicate: Expr::try_from_with_tokens_opt(v.get_where_clause(), tokens)?,
            exclusions,
            delete_columns: Name::try_from(v.get_fk_del_set_cols())?.0,
        })
    }
}

impl TryFromWithTokens<&syn::Node> for ExclusionElement {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::Node,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let pair = required(value.as_list().as_inner(), "exclusion pair")?
            .get_items()
            .as_inner()
            .unwrap_or_default();
        let [expression, operator] = pair.as_slice() else {
            return AnalysisError::raise_unexpected_input("exclusion pair");
        };
        Ok(Self {
            expression: AstIndexExpression::try_from_with_tokens(
                &required(
                    expression.as_index_elem().as_inner(),
                    "exclusion expression",
                )?,
                tokens,
            )?,
            operator: Name::try_from(
                required(operator.as_list().as_inner(), "exclusion operator")?.get_items(),
            )?,
        })
    }
}

impl TryCollectFromWithTokens<syn::NodeListOpt> for PartitionDatum {
    type Error = AnalysisError;
    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        nodes
            .as_inner()
            .unwrap_or_default()
            .iter()
            .map(|node| Self::try_from_with_tokens(node, tokens))
            .collect()
    }
}

impl AstTableConstraint {
    pub(in crate::ast) fn from_postgres_inline(
        constraint: &syn::Constraint,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let mut converted = Self::try_from_with_tokens(constraint, tokens)?;
        // Inline enforcement is represented by a separate attribute node.
        converted.extras.enforced = None;
        Ok(converted)
    }
}
