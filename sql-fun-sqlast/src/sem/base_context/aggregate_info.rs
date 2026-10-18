//! Builtin aggregate names used by grouping placement analysis.
//!
//! Verified against PostgreSQL 15–18 with:
//! SELECT DISTINCT proname FROM pg_proc WHERE prokind = 'a'
//! AND pronamespace = 'pg_catalog'::regnamespace ORDER BY 1.
//! Classification also requires a catalog binding, so names absent from the
//! selected server version are not made available by this list. User catalogs
//! supply their own classification through FunctionInfoRead.
use crate::sem::FullName;

pub(super) fn builtin_aggregate(name: &FullName) -> bool {
    name.schema()
        .is_some_and(|schema| schema.as_str() == "pg_catalog")
        && matches!(
            name.name().as_str(),
            "any_value"
                | "array_agg"
                | "avg"
                | "bit_and"
                | "bit_or"
                | "bit_xor"
                | "bool_and"
                | "bool_or"
                | "corr"
                | "count"
                | "covar_pop"
                | "covar_samp"
                | "cume_dist"
                | "dense_rank"
                | "every"
                | "json_agg"
                | "json_agg_strict"
                | "json_object_agg"
                | "json_object_agg_strict"
                | "json_object_agg_unique"
                | "json_object_agg_unique_strict"
                | "jsonb_agg"
                | "jsonb_agg_strict"
                | "jsonb_object_agg"
                | "jsonb_object_agg_strict"
                | "jsonb_object_agg_unique"
                | "jsonb_object_agg_unique_strict"
                | "max"
                | "min"
                | "mode"
                | "percent_rank"
                | "percentile_cont"
                | "percentile_disc"
                | "range_agg"
                | "range_intersect_agg"
                | "rank"
                | "regr_avgx"
                | "regr_avgy"
                | "regr_count"
                | "regr_intercept"
                | "regr_r2"
                | "regr_slope"
                | "regr_sxx"
                | "regr_sxy"
                | "regr_syy"
                | "stddev"
                | "stddev_pop"
                | "stddev_samp"
                | "string_agg"
                | "sum"
                | "var_pop"
                | "var_samp"
                | "variance"
                | "xmlagg"
        )
}

#[cfg(test)]
mod tests {
    use super::builtin_aggregate;
    use crate::sem::FullName;

    #[rstest::rstest]
    #[case::aggregate("pg_catalog", "sum", true)]
    #[case::ordered_set("pg_catalog", "percentile_cont", true)]
    #[case::newer_builtin("pg_catalog", "json_agg_strict", true)]
    #[case::ordinary("pg_catalog", "abs", false)]
    #[case::grouping_operation("pg_catalog", "grouping", false)]
    #[case::other_schema("public", "sum", false)]
    fn classifies_only_builtin_aggregate_names(
        #[case] schema: &str,
        #[case] name: &str,
        #[case] aggregate: bool,
    ) {
        assert_eq!(
            builtin_aggregate(&FullName::with_schema(schema, name)),
            aggregate
        );
    }
}
