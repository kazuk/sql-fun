use crate::{context::SchemaFileContext, test_helpers::quantified};

pub(crate) fn schema() -> testresult::TestResult<SchemaFileContext> {
    quantified::schema(
        "CREATE DOMAIN amount AS integer; CREATE DOMAIN amount_array AS integer[]; CREATE DOMAIN amount_array_chain AS amount_array; CREATE DOMAIN amount_array_deep AS amount_array_chain;",
    )
}
