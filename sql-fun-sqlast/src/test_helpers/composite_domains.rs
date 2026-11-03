//! Small domain/member catalog; no builtin catalog loading.

pub(crate) use crate::test_helpers::domain_casts::reference;

use crate::sem::{CreateDomain, TypeDefinition, TypeReference};

use crate::{
    context::{ParseContext, SchemaFileContext},
    sem::AnalysisError,
    test_helpers::{column_rename, ownership::apply_ownership_sql},
};

pub(crate) fn context(frozen: bool) -> Result<SchemaFileContext, AnalysisError> {
    let mut context = column_rename::schema(&[])?;
    for sql in [
        "CREATE DOMAIN public.scalar_d AS integer",
        "CREATE TYPE public.leaf AS (n public.scalar_d)",
        "CREATE DOMAIN public.leaf_d AS public.leaf",
        "CREATE DOMAIN public.leaf_dd AS public.leaf_d",
        "CREATE TYPE public.outer_row AS (item public.leaf_dd)",
        "CREATE DOMAIN public.outer_d AS public.outer_row",
        "CREATE TABLE public.domain_table (n public.scalar_d)",
        "CREATE DOMAIN public.table_d AS public.domain_table",
        "CREATE DOMAIN public.arr_d AS public.leaf[]",
    ] {
        context = apply_ownership_sql(context, sql)?.into_context();
    }
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    if frozen {
        context = column_rename::child(&context.freeze());
    }
    Ok(context)
}

pub(crate) fn owned_domain(name: &str, base: &TypeReference) -> TypeReference {
    TypeReference::dynamic(&TypeDefinition::from(&CreateDomain::new(
        &reference(name),
        &Some(base.clone()),
    )))
}
