//! Analysis owned by the semantic definitions in this module.

use crate::{
    ast::{Column as AstColumn, ColumnConstraint as AstColumnConstraint},
    context::ParseContext,
    sem::ConstraintNames,
};

impl ConstraintNames {
    pub(crate) fn ast_constraint_has_catalog_name(
        ast: &AstColumnConstraint,
        context: &impl ParseContext,
    ) -> bool {
        match ast {
            AstColumnConstraint::PrimaryKey(..)
            | AstColumnConstraint::Unique(_)
            | AstColumnConstraint::Check(_)
            | AstColumnConstraint::ForeignKey(_)
            | AstColumnConstraint::Exclusion => true,
            AstColumnConstraint::Nullability(true, ..) => Self::supports_named_not_null(context),
            _ => false,
        }
    }

    fn supports_named_not_null(context: &impl ParseContext) -> bool {
        let Some(version) = context.metadata().engine_version() else {
            return false;
        };
        let Ok(version) = version
            .to_string()
            .parse::<::sql_fun_core::PostgresVersion>()
        else {
            return false;
        };
        version.major() >= 18
    }
}

impl ConstraintNames {
    pub(crate) fn reserve_ast_column(
        ast: &AstColumn,
        context: &impl ParseContext,
        names: &mut Self,
    ) {
        for definition in &ast.constraints {
            if !definition.is_check()
                && Self::ast_constraint_has_catalog_name(&definition.constraint, context)
            {
                names.reserve(definition.name.as_deref().unwrap_or_default());
            }
        }
    }
}
