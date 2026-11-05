use crate::{
    ast::{
        Column as AstColumn, Expr, Relation as AstRelation, Role, SyntaxOption,
        TableConstraint as AstTableConstraint, TryCollectFromWithTokens, TryFromWithTokens,
        TypeName,
        ddl::{
            AlterColumnType, AlterConstraintFlags, AlterDefinition, AlterIdentity, AlterOperation,
            AlterPartition, FunctionOption, GenericOption, ReplicaIdentity,
        },
        postgres::required,
    },
    error::AnalysisError,
    syn::{self, ListOpt, Opt},
};

use ::sql_fun_core::IVec;
impl FunctionOption {
    pub(crate) fn boolean_argument(node: &syn::NodeOpt) -> Result<bool, AnalysisError> {
        required(node.as_boolean().get_boolval(), "function Boolean option")
    }

    pub(crate) fn string_list(
        node: syn::NodeOpt,
        description: &str,
    ) -> Result<Vec<String>, AnalysisError> {
        let items = required(node.as_list().get_items().as_inner(), description)?;
        let mut values = Vec::new();
        for item in items {
            values.push(required(item.as_string().get_sval(), description)?);
        }
        Ok(values)
    }

    pub(crate) fn type_list(
        node: syn::NodeOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<TypeName>, AnalysisError> {
        let items = required(
            node.as_list().get_items().as_inner(),
            "function TRANSFORM types",
        )?;
        let mut values = Vec::new();
        for item in items {
            values.push(TypeName::try_from_with_tokens(
                &required(item.as_type_name().as_inner(), "function TRANSFORM type")?,
                tokens,
            )?);
        }
        Ok(values)
    }
}

impl AlterOperation {
    pub(crate) fn convert_definition(
        &self,
        value: &syn::AlterTableCmd,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<AlterDefinition, AnalysisError> {
        use crate::ast::ddl::AlterDefinition;
        let definition = value.get_def();
        Ok(match self {
            Self::AddColumn => AlterDefinition::column(AstColumn::try_from_with_tokens(
                &required(definition.as_column_def().as_inner(), "added column")?,
                tokens,
            )?),
            Self::AlterColumnType => {
                AlterDefinition::column_type(AlterColumnType::try_from_with_tokens(
                    &required(definition.as_column_def().as_inner(), "altered column type")?,
                    tokens,
                )?)
            }
            Self::ColumnDefault | Self::SetStatistics | Self::SetExpression => {
                if definition.as_inner().is_some() {
                    AlterDefinition::expression(Expr::try_from_with_tokens(&definition, tokens)?)
                } else {
                    AlterDefinition::none()
                }
            }
            Self::SetStorage | Self::SetCompression => AlterDefinition::text(&required(
                definition.as_string().get_sval(),
                "column storage option",
            )?),
            Self::SetOptions
            | Self::ResetOptions
            | Self::SetRelOptions
            | Self::ResetRelOptions
            | Self::SetIdentity => {
                AlterDefinition::options(SyntaxOption::try_collect_from_with_tokens(
                    definition.as_list().get_items(),
                    tokens,
                )?)
            }
            Self::GenericOptions | Self::AlterColumnGenericOptions => {
                AlterDefinition::generic_options(GenericOption::try_collect_from_with_tokens(
                    definition.as_list().get_items(),
                    tokens,
                )?)
            }
            Self::AddConstraint => {
                AlterDefinition::constraint(AstTableConstraint::try_from_with_tokens(
                    &required(definition.as_constraint().as_inner(), "added constraint")?,
                    tokens,
                )?)
            }
            Self::AlterConstraint => {
                AlterDefinition::constraint_flags(AlterConstraintFlags::try_from(&required(
                    definition.as_atalter_constraint().as_inner(),
                    "altered constraint",
                )?)?)
            }
            Self::Owner => AlterDefinition::role(Role::try_from(&required(
                value.get_newowner().as_inner(),
                "new owner",
            )?)?),
            Self::AddInherit | Self::DropInherit => AlterDefinition::relation(
                AstRelation::try_from(&required(
                    definition.as_range_var().as_inner(),
                    "inheritance parent",
                )?)?
                .name,
            ),
            Self::AddOf => AlterDefinition::type_name(TypeName::try_from_with_tokens(
                &required(definition.as_type_name().as_inner(), "typed table type")?,
                tokens,
            )?),
            Self::ReplicaIdentity => {
                AlterDefinition::replica(ReplicaIdentity::try_from(&required(
                    definition.as_replica_identity_stmt().as_inner(),
                    "replica identity",
                )?)?)
            }
            Self::AttachPartition | Self::DetachPartition | Self::DetachPartitionFinalize => {
                AlterDefinition::partition(AlterPartition::try_from_with_tokens(
                    &required(
                        definition.as_partition_cmd().as_inner(),
                        "partition command",
                    )?,
                    tokens,
                )?)
            }
            Self::AddIdentity => AlterDefinition::identity(AlterIdentity::try_from_with_tokens(
                &required(definition.as_constraint().as_inner(), "identity constraint")?,
                tokens,
            )?),
            _ => AlterDefinition::none(),
        })
    }
}
