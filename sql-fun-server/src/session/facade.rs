use super::{
    SessionState, context_handle_registry::ContextHandleRegistry, initialization::Initialization,
};
use crate::{Api, InitializedSchema, JsonRpcError, json_rpc::JsonRpcRequest};
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
        let context_handle = self.register_context(&schema)?;
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

    pub(super) async fn process_request(
        &mut self,
        api: &Api,
        request: &JsonRpcRequest,
    ) -> Result<::serde_json::Value, JsonRpcError> {
        let method = request.method();
        match method {
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
            _ => crate::process_request(request),
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
    layers: &'ctx [Arc<crate::LayerDiagnostics>],
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
}

impl Diagnostics<'_> {
    const fn span(span: ::sql_fun_sqlast::StringSpan) -> StatementByteRange {
        StatementByteRange::new(span.offset(), span.len())
    }

    fn map(item: Diagnostic<'_>) -> ::serde_json::Value {
        let span = item.problem.statement_relative_span().map(Self::span);
        let severity = match item.problem.severity() {
            ::miette::Severity::Error => DiagnosticSeverity::error(),
            ::miette::Severity::Warning => DiagnosticSeverity::warning(),
            ::miette::Severity::Advice => DiagnosticSeverity::advice(),
        };
        ::serde_json::to_value(AnalysisProbrem::new(
            item.problem.code(),
            severity,
            item.path,
            span,
        ))
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
}

impl<'ctx> LayerProblems<'ctx> {
    fn items(layer: &'ctx Arc<crate::LayerDiagnostics>) -> impl Iterator<Item = Diagnostic<'ctx>> {
        Self {
            path: layer.path(),
            problems: layer.problems(),
        }
        .into_items()
    }

    fn into_items(self) -> impl Iterator<Item = Diagnostic<'ctx>> {
        self.problems.iter().map(move |problem| Diagnostic {
            path: self.path,
            problem,
        })
    }
}
