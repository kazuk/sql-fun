use sql_fun_core::IVec;

use crate::{
    sem::unresolved::TryFromWithTokens,
    sem::{
        AnalysisError, AstAndContextPair, ColumnCollection, FullName, FunctionOverloadCollection,
        OverloadVariant, ParseContext, type_system::ArgumentBindingCollection,
    },
    syn::ScanToken,
};

/// analyzed [`crate::syn::CreateFunctionStmt`]
#[derive(Debug, Clone)]
pub struct CreateFunction {
    name: FullName,
    overloads: FunctionOverloadCollection,
    returning_columdef: Option<ColumnCollection>,
}

impl CreateFunction {
    /// create new instance
    #[must_use]
    pub fn new(
        name: &FullName,
        overloads: &FunctionOverloadCollection,
        returning_columns: &Option<ColumnCollection>,
    ) -> Self {
        Self {
            name: name.clone(),
            overloads: overloads.clone(),
            returning_columdef: returning_columns.clone(),
        }
    }

    /// list function overload (Using in builtin / extensions only)
    #[must_use]
    pub const fn overloads(&self) -> &FunctionOverloadCollection {
        &self.overloads
    }

    /// resolve overload with function argument types
    pub fn resolve_overload<TParseContext>(
        &self,
        arg_types: &ArgumentBindingCollection,
        context: &mut TParseContext,
    ) -> Option<OverloadVariant>
    where
        TParseContext: ParseContext,
    {
        if self.overloads.len() == 1 {
            self.overloads.get_at(0).cloned()
        } else {
            self.overloads.resolve_overload(context, arg_types)
        }
    }

    /// return trure if function is returns set
    #[must_use]
    pub const fn is_table_function(&self) -> bool {
        self.returning_columdef.is_some()
    }
}

impl CreateFunction {
    /// get function name
    #[must_use]
    pub const fn name(&self) -> &FullName {
        &self.name
    }

    /// analyze [`crate::syn::CreateFunctionStmt`]
    pub fn analyze_create_function<TParseContext>(
        context: TParseContext,
        _parent_schema: &Option<String>,
        syn: crate::syn::CreateFunctionStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        crate::sem::unresolved::Function::try_from_with_tokens(&syn, tokens)?
            .resolve_and_apply(context, crate::sem::unresolved::Dialect::postgres())
    }
}
