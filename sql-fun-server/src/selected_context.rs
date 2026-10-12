use ::sql_fun_core::{SqlFunArgs, SqlFunMetadata};
use ::sql_fun_sqlast::sem::{
    BaseParseContext, FrozenBaseContext, FullName, TypeDefinition, TypeInfoRead, TypeReference,
};
use std::path::Path;

/// Request-local configuration over a shared, immutable catalog.
#[derive(Debug)]
pub(crate) struct SelectedContext {
    base: FrozenBaseContext,
    args: SqlFunArgs,
    metadata: SqlFunMetadata,
}

impl SelectedContext {
    pub(crate) fn freeze(
        base: &FrozenBaseContext,
        args: &SqlFunArgs,
        metadata: &SqlFunMetadata,
    ) -> FrozenBaseContext {
        FrozenBaseContext::new(Self {
            base: base.clone(),
            args: args.clone(),
            metadata: metadata.clone(),
        })
    }
    const fn base(&self) -> &FrozenBaseContext {
        &self.base
    }
}

impl ::sql_fun_sqlast::sem::CastInfoRead for SelectedContext {
    fn get_explicit_cast(
        &self,
        source: &::sql_fun_sqlast::sem::TypeReference,
        target: &::sql_fun_sqlast::sem::TypeReference,
    ) -> Option<::sql_fun_sqlast::sem::CastDefinition> {
        ::sql_fun_sqlast::sem::CastInfoRead::get_explicit_cast(self.base(), source, target)
    }

    fn get_implicit_cast(
        &self,
        source: &::sql_fun_sqlast::sem::TypeReference,
        target: &::sql_fun_sqlast::sem::TypeReference,
    ) -> Option<::sql_fun_sqlast::sem::CastDefinition> {
        ::sql_fun_sqlast::sem::CastInfoRead::get_implicit_cast(self.base(), source, target)
    }

    fn get_implicit_castable(
        &self,
        source: &::sql_fun_sqlast::sem::TypeReference,
    ) -> std::collections::HashSet<::sql_fun_sqlast::sem::TypeReference> {
        ::sql_fun_sqlast::sem::CastInfoRead::get_implicit_castable(self.base(), source)
    }
}

impl ::sql_fun_sqlast::sem::FunctionInfoRead for SelectedContext {
    fn get_function_impl(
        &self,
        name: &::sql_fun_sqlast::sem::FullName,
    ) -> Option<::sql_fun_sqlast::sem::FunctionOverloadCollection> {
        ::sql_fun_sqlast::sem::FunctionInfoRead::get_function_impl(self.base(), name)
    }
}

impl ::sql_fun_sqlast::sem::IndexInfoRead for SelectedContext {
    fn get_index(&self, name: &str) -> Option<&::sql_fun_sqlast::sem::CreateIndex> {
        ::sql_fun_sqlast::sem::IndexInfoRead::get_index(self.base(), name)
    }
}

impl ::sql_fun_sqlast::sem::OperatorInfoRead for SelectedContext {
    fn get_operator_op(
        &self,
        name: &::sql_fun_sqlast::sem::OperatorName,
        left: bool,
        right: bool,
    ) -> Option<&::sql_fun_sqlast::sem::OperatorDefinition> {
        ::sql_fun_sqlast::sem::OperatorInfoRead::get_operator_op(self.base(), name, left, right)
    }
}

impl ::sql_fun_sqlast::sem::SequenceInfoRead for SelectedContext {
    fn get_sequence_impl(
        &self,
        name: &::sql_fun_sqlast::sem::FullName,
    ) -> Option<&::sql_fun_sqlast::sem::CreateSequence> {
        ::sql_fun_sqlast::sem::SequenceInfoRead::get_sequence_impl(self.base(), name)
    }
}

impl ::sql_fun_sqlast::sem::TableInfoRead for SelectedContext {
    fn get_table_impl(
        &self,
        name: &::sql_fun_sqlast::sem::FullName,
    ) -> Option<&::sql_fun_sqlast::sem::CreateTable> {
        ::sql_fun_sqlast::sem::TableInfoRead::get_table_impl(self.base(), name)
    }
}

impl ::sql_fun_sqlast::sem::ViewInfoRead for SelectedContext {
    fn get_view_impl(
        &self,
        name: &::sql_fun_sqlast::sem::FullName,
    ) -> Option<&::sql_fun_sqlast::sem::ViewLike> {
        ::sql_fun_sqlast::sem::ViewInfoRead::get_view_impl(self.base(), name)
    }
}

impl ::sql_fun_sqlast::sem::SearchPathRead for SelectedContext {
    fn get_search_path(&self) -> &[::sql_fun_sqlast::sem::SchemaName] {
        ::sql_fun_sqlast::sem::SearchPathRead::get_search_path(self.base())
    }

    fn get_default_schema(&self) -> Option<::sql_fun_sqlast::sem::SchemaName> {
        ::sql_fun_sqlast::sem::SearchPathRead::get_default_schema(self.base())
    }
}

impl ::sql_fun_sqlast::sem::GetSourcePathWithSpanOpt for SelectedContext {
    fn get_source_path_with_span(&self) -> Option<&::sql_fun_sqlast::sem::SourcePathWithSpan> {
        ::sql_fun_sqlast::sem::GetSourcePathWithSpanOpt::get_source_path_with_span(self.base())
    }
}
impl TypeInfoRead for SelectedContext {
    fn get_type_impl(&self, key: &FullName) -> Option<&TypeDefinition> {
        self.base.get_type_impl(key)
    }
    fn get_array_item_type(&self, name: &TypeReference) -> Option<TypeReference> {
        self.base.get_array_item_type(name)
    }
}

impl BaseParseContext for SelectedContext {
    fn metadata(&self) -> &SqlFunMetadata {
        &self.metadata
    }
    fn get_args(&self) -> &SqlFunArgs {
        &self.args
    }
    fn get_source_file_path(&self) -> Option<&Path> {
        self.base.get_source_file_path()
    }
}
