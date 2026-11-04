use crate::{
    ast::Dialect,
    context::{FrozenBaseContext, SchemaFileContext},
    sem::ParseContext,
};

/// Original catalog and explicit configuration for post-analysis inspection.
/// The frozen base is shared; only the current catalog layer is copied.
#[derive(Debug)]
pub(super) struct InferenceEnvironment {
    catalog: FrozenBaseContext,
    args: sql_fun_core::SqlFunArgs,
    metadata: sql_fun_core::SqlFunMetadata,
}

impl InferenceEnvironment {
    pub(super) fn new(context: &impl ParseContext) -> Self {
        Self {
            catalog: context.retain_catalog(),
            args: context.get_args().clone(),
            metadata: context.metadata().clone(),
        }
    }

    pub(super) fn postgres_defaults(&self) -> bool {
        Dialect::from(&self.metadata) == Dialect::postgres()
    }

    pub(super) fn context(&self) -> SchemaFileContext {
        SchemaFileContext::new(
            std::path::Path::new("<assignment>"),
            &self.catalog,
            &self.args,
            &self.metadata,
        )
    }
}
