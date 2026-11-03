use crate::{
    Api, InitializedSchema, JsonRpcError, LayerDiagnostics,
    json_rpc::JsonRpcRequest,
    process_request as local_process_request,
    session::{
        SessionState, context_handle_registry::ContextHandleRegistry,
        initialization::Initialization,
    },
};
use std::sync::Arc;

use sql_fun_server_api::{
    SqlFunServerApiError,
    payloads::{
        AnalysisProbrem, CollectionHandle, ContextHandle, DiagnosticSeverity, InitializeSchemaArgs,
        ReadCollectionArgs, ReleaseCollectionArgs, SchemaContext, StatementByteRange,
    },
};

/// A request's mutable access to connection state, with independently long-lived
/// schema borrows. No lock guard or registry mutation mechanism reaches handlers.
pub(super) struct Session<'request, 'ctx> {
    contexts: &'ctx ContextHandleRegistry,
    state: &'request mut SessionState<'ctx>,
}

impl<'request, 'ctx> Session<'request, 'ctx> {
    pub(super) const fn new(
        contexts: &'ctx ContextHandleRegistry,
        state: &'request mut SessionState<'ctx>,
    ) -> Self {
        Self { contexts, state }
    }

    pub(super) fn register_context(
        &self,
        schema: &Arc<InitializedSchema>,
    ) -> Result<ContextHandle, SqlFunServerApiError> {
        self.contexts
            .register(schema)
            .map_err(|error| SqlFunServerApiError::new_unexpected(&error.to_string()))
    }

    pub(super) fn context(&self, handle: &ContextHandle) -> Option<&'ctx InitializedSchema> {
        self.contexts.resolve(handle)
    }

    pub(super) fn expand_asterisk(&self, handle: &ContextHandle) -> bool {
        self.contexts
            .rewrite_options(handle)
            .is_some_and(|options| options.expand_asterisk() == Some(true))
    }

    /// Borrow the cache associated with an already validated schema registration.
    pub(super) fn cache_context(
        &mut self,
        handle: &ContextHandle,
    ) -> &mut ::sql_fun_sqlast::CacheContext {
        self.state
            .cache_contexts
            .entry(handle.clone())
            .or_insert_with(::sql_fun_sqlast::CacheContext::new)
    }

    pub(super) fn collection_count(&self) -> usize {
        self.state.collections.len()
    }

    pub(super) fn register_collection<S, L, M>(
        &mut self,
        source: S,
        length: L,
        map: M,
    ) -> Result<CollectionHandle, SqlFunServerApiError>
    where
        S: Send + 'ctx,
        L: Fn(&S) -> usize + Send + 'ctx,
        M: Send + 'ctx,
        for<'read> &'read S: IntoIterator,
        for<'read> M: Fn(<&'read S as IntoIterator>::Item) -> ::serde_json::Value,
    {
        self.state
            .collections
            .register(source, length, map)
            .map_err(Into::into)
    }

    pub(super) fn read_collection(
        &self,
        args: &ReadCollectionArgs,
    ) -> Result<::serde_json::Value, SqlFunServerApiError> {
        let page = self.state.collections.read(
            args.collection_handle(),
            *args.start_offset(),
            *args.count(),
        )?;
        ::serde_json::to_value(page)
            .map_err(|error| SqlFunServerApiError::new_unexpected(&error.to_string()))
    }

    pub(super) fn release_collection(
        &mut self,
        args: &ReleaseCollectionArgs,
    ) -> Result<(), SqlFunServerApiError> {
        self.state
            .collections
            .release(args.collection_handle())
            .map_err(Into::into)
    }

    async fn initialize_schema_context(
        &mut self,
        api: &Api,
        args: &InitializeSchemaArgs,
    ) -> Result<SchemaContext, SqlFunServerApiError> {
        let initialization = Initialization::from_request(args)?;
        let schema = Arc::new(initialization.initialize(api).await?);
        let context_handle = self
            .contexts
            .register_with_options(&schema, *initialization.rewrite_options())
            .map_err(|error| SqlFunServerApiError::new_unexpected(&error.to_string()))?;
        let retained = self.context(&context_handle).expect("registered schema");
        let diagnostics = Diagnostics::from(retained);
        let handle = self.register_collection(diagnostics, Diagnostics::len, Diagnostics::map)?;
        // Reading and decoding this page cannot fail for the mapping below. Roll
        // back its registration if serialization contracts ever change.
        let page = self.state.collections.read(&handle, 0, None)?;
        let page = match ::serde_json::from_value(
            ::serde_json::to_value(page).expect("JSON values serialize"),
        ) {
            Ok(page) => page,
            Err(error) => {
                self.state.collections.release(&handle)?;
                return Err(SqlFunServerApiError::new_unexpected(&error.to_string()));
            }
        };
        Ok(SchemaContext::new(
            &context_handle,
            schema.actual_hash(),
            page,
        ))
    }

    fn tokenize_sql(
        &mut self,
        args: &::sql_fun_server_api::payloads::TokenizeSqlArgs,
    ) -> Result<::serde_json::Value, SqlFunServerApiError> {
        let stream = ::sql_fun_sqlast::tokenize_sql(args.source(), args.dialect())
            .map_err(|error| SqlFunServerApiError::tokenization_failed(&error.to_string()))?;
        let tokens: Vec<_> = stream
            .tokens()
            .map(::sql_fun_server_api::payloads::SqlToken::from)
            .collect();
        let handle = self.register_collection(tokens, Vec::len, Self::token_value)?;
        let result = self
            .state
            .collections
            .read(&handle, 0, None)
            .map_err(SqlFunServerApiError::from)
            .and_then(|page| {
                ::serde_json::to_value(page)
                    .map_err(|error| SqlFunServerApiError::new_unexpected(&error.to_string()))
            });
        if result.is_err() {
            self.state.collections.release(&handle)?;
        }
        result
    }

    fn token_value(token: &::sql_fun_server_api::payloads::SqlToken) -> ::serde_json::Value {
        ::serde_json::to_value(token).expect("presentation token serializes")
    }

    pub(super) async fn process_request(
        &mut self,
        api: &Api,
        request: &JsonRpcRequest,
    ) -> Result<::serde_json::Value, JsonRpcError> {
        let method = request.method();
        match method {
            "tokenize_sql" => Ok(self.tokenize_sql(&Self::arguments(request)?)?),
            "analyze_query" => {
                let args = Self::arguments(request)?;
                Ok(::serde_json::to_value(self.analyze_query(&args)?)?)
            }
            "initialize_schema_context" => {
                let args: InitializeSchemaArgs = Self::arguments(request)?;
                let result = self.initialize_schema_context(api, &args).await?;
                Ok(::serde_json::to_value(result)?)
            }
            "read_collection" => Ok(self.read_collection(&Self::arguments(request)?)?),
            "release_collection" => {
                self.release_collection(&Self::arguments(request)?)?;
                Ok(::serde_json::Value::Null)
            }
            _ => local_process_request(request),
        }
    }

    fn arguments<T: ::serde::de::DeserializeOwned>(
        request: &JsonRpcRequest,
    ) -> Result<T, JsonRpcError> {
        let params = request
            .params()
            .ok_or_else(|| JsonRpcError::parameter_required(request.method()))?;
        Ok(::serde_json::from_value(params.clone())?)
    }
}

#[cfg(test)]
mod tests;

/// This source borrows cached diagnostics and maps only the requested items.
struct Diagnostics<'ctx> {
    layers: &'ctx [Arc<LayerDiagnostics>],
}

impl<'ctx> From<&'ctx InitializedSchema> for Diagnostics<'ctx> {
    fn from(schema: &'ctx InitializedSchema) -> Self {
        Self {
            layers: schema.analysis().diagnostics(),
        }
    }
}

impl Diagnostics<'_> {
    fn len(&self) -> usize {
        self.layers.iter().map(|layer| layer.problems().len()).sum()
    }
}

#[derive(Clone, Copy)]
struct Diagnostic<'ctx> {
    path: &'ctx std::path::Path,
    problem: &'ctx ::sql_fun_sqlast::sem::AnalysisProblem,
    presentation: Option<&'ctx ::sql_fun_core::DiagnosticPresentation>,
}

impl Diagnostics<'_> {
    const fn span(span: ::sql_fun_core::analysis_problem::StringSpan) -> StatementByteRange {
        StatementByteRange::new(span.offset(), span.len())
    }

    fn map(item: Diagnostic<'_>) -> ::serde_json::Value {
        let span = item.problem.statement_relative_span().map(Self::span);
        let severity = DiagnosticSeverity::from(item.problem.severity());
        ::serde_json::to_value(
            AnalysisProbrem::new(item.problem.code(), severity, item.path, span)
                .with_presentation(item.presentation),
        )
        .expect("diagnostic serializes")
    }
}

impl<'read> IntoIterator for &'read Diagnostics<'_> {
    type Item = Diagnostic<'read>;
    type IntoIter = Box<dyn Iterator<Item = Self::Item> + 'read>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.layers.iter().flat_map(LayerProblems::items))
    }
}

struct LayerProblems<'ctx> {
    path: &'ctx std::path::Path,
    problems: &'ctx [::sql_fun_sqlast::sem::AnalysisProblem],
    presentations: &'ctx [Option<::sql_fun_core::DiagnosticPresentation>],
}

impl<'ctx> LayerProblems<'ctx> {
    fn items(layer: &'ctx Arc<LayerDiagnostics>) -> impl Iterator<Item = Diagnostic<'ctx>> {
        Self {
            path: layer.path(),
            problems: layer.problems(),
            presentations: layer.diagnostic_presentations(),
        }
        .into_items()
    }

    fn into_items(self) -> impl Iterator<Item = Diagnostic<'ctx>> {
        self.problems
            .iter()
            .enumerate()
            .map(move |(index, problem)| Diagnostic {
                path: self.path,
                problem,
                presentation: if let Some(presentation) = self.presentations.get(index) {
                    presentation.as_ref()
                } else {
                    None
                },
            })
    }
}
