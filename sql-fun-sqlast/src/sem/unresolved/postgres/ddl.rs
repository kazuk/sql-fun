use super::required;

impl TryFrom<syn::ObjectType> for crate::sem::unresolved::AlterRelationKind {
    type Error = AnalysisError;
    fn try_from(value: syn::ObjectType) -> Result<Self, Self::Error> {
        Ok(match value {
            syn::ObjectType::ObjectTable => Self::Table,
            syn::ObjectType::ObjectView => Self::View,
            syn::ObjectType::ObjectMatview => Self::MaterializedView,
            syn::ObjectType::ObjectSequence => Self::Sequence,
            _ => return AnalysisError::raise_unexpected_input("alter relation kind"),
        })
    }
}

impl TryFromWithTokens<&syn::AlterTableStmt> for crate::sem::unresolved::AlterRelation {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::AlterTableStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        use crate::sem::unresolved::{AlterCommand, AlterRelationKind, Relation};
        let relation = required(value.get_relation().as_inner(), "alter_table.relation")?;
        let mut commands = Vec::new();
        for command in value
            .get_cmds()
            .map(|commands| commands.as_alter_table_cmd())
            .unwrap_or_default()
        {
            commands.push(AlterCommand::try_from_with_tokens(&command, tokens)?);
        }
        Ok(Self {
            name: Relation::try_from(&relation)?.name,
            kind: AlterRelationKind::try_from(required(
                value.get_objtype().as_inner(),
                "alter_table.objtype",
            )?)?,
            commands,
            if_exists: value.get_missing_ok(),
        })
    }
}

impl TryFromWithTokens<&syn::AlterTableCmd> for crate::sem::unresolved::AlterCommand {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::AlterTableCmd,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        use crate::sem::unresolved::{Column, TableConstraint};
        Ok(
            match required(value.get_subtype().as_inner(), "alter_table_cmd.subtype")? {
                syn::AlterTableType::AtChangeOwner => Self::Owner(required(
                    value.get_newowner().get_rolename(),
                    "alter_table_cmd.newowner",
                )?),
                syn::AlterTableType::AtColumnDefault => Self::Default(
                    value.get_name(),
                    Expr::try_from_with_tokens(&value.get_def(), tokens)?,
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
                kind => Self::Unsupported(format!("{kind:?}")),
            },
        )
    }
}

impl TryFromWithTokens<&syn::CreateStmt> for crate::sem::unresolved::Table {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::CreateStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        use crate::sem::unresolved::{Column, Relation, TableConstraint};
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
            name: relation.name,
            columns,
            constraints,
            temporary: relation.persistence == "t",
            if_not_exists: value.get_if_not_exists(),
            query: None,
            options: SyntaxOption::try_collect_from_with_tokens(value.get_options(), tokens)?,
        })
    }
}

impl TryFromWithTokens<&syn::ColumnDef> for crate::sem::unresolved::Column {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::ColumnDef,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        use crate::sem::unresolved::TableConstraint;
        let type_name = required(value.get_type_name().as_inner(), "column.type_name")?;
        let mut constraints = Vec::new();
        for constraint in value
            .get_constraints()
            .map(|nodes| nodes.as_constraint())
            .unwrap_or_default()
        {
            constraints.push(TableConstraint::try_from_with_tokens(&constraint, tokens)?);
        }
        Ok(Self {
            name: value.get_colname(),
            type_name: Some(TypeName::try_from_with_tokens(&type_name, tokens)?),
            constraints,
            not_null: Some(value.get_is_not_null()),
        })
    }
}

impl TryFromWithTokens<&syn::Constraint> for crate::sem::unresolved::TableConstraint {
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
            name: super::nonempty(value.get_conname()),
            keys: Name::try_from(keys)?.0,
            constraint: crate::sem::unresolved::ColumnConstraint::try_from_with_tokens(
                value, tokens,
            )?,
        })
    }
}

impl TryFromWithTokens<&syn::Constraint> for crate::sem::unresolved::ColumnConstraint {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::Constraint,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        use crate::sem::{
            Deferrable, Validity,
            column_constraints::{GeneratedColumnMode, IdentityColumnMode},
            unresolved::ForeignReference,
        };
        let kind = required(value.get_contype().as_inner(), "constraint.contype")?;
        Ok(match kind {
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
                Some(GeneratedColumnMode::from_postgres_generated_column_mode(
                    &value.get_generated_when(),
                )?),
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

impl TryFrom<&syn::Constraint> for crate::sem::unresolved::ForeignReference {
    type Error = AnalysisError;
    fn try_from(value: &syn::Constraint) -> Result<Self, Self::Error> {
        use crate::sem::{
            Validity,
            column_constraints::{Deferrability, ForeignKeyAction, ForeignKeyMatchType},
            unresolved::Relation,
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
use crate::{
    sem::{
        AnalysisError,
        unresolved::{
            Expr, Name, Statement, SyntaxOption, TryCollectFromWithTokens, TryFromWithTokens,
            TryFromWithTokensOpt, TypeName,
            ddl::{Function, FunctionParameter, ParameterMode, Schema},
        },
    },
    syn::{self, ListOpt, Opt},
};
use ::sql_fun_core::IVec;

impl TryFromWithTokens<&syn::CreateTableAsStmt> for crate::sem::unresolved::TableAs {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::CreateTableAsStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        use crate::sem::unresolved::{Into, Query};
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
        ))
    }
}

impl TryFromWithTokens<&syn::IndexStmt> for crate::sem::unresolved::Index {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::IndexStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        use crate::sem::unresolved::{IndexExpression, Relation};
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
            elements.push(Statement::try_from_with_tokens(&element, tokens)?);
        }
        Ok(Self::new(
            &value.get_schemaname(),
            elements,
            value.get_if_not_exists(),
        ))
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
        let result_type = value
            .get_return_type()
            .as_inner()
            .map(|type_name| TypeName::try_from_with_tokens(&type_name, tokens))
            .transpose()?;
        Ok(Self::new(
            Name::try_from(value.get_funcname())?,
            parameters,
            result_type,
            SyntaxOption::try_collect_from_with_tokens(value.get_options(), tokens)?,
            value.get_replace(),
            value.get_is_procedure(),
        ))
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
