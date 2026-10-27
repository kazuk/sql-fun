use crate::sem::{AnalysisError, CreateFunction, FullName, FunctionOverloadCollection};

/// get function definition
pub trait FunctionInfoRead {
    /// Declared routine identities that can depend on schema-defined row types.
    /// Immutable builtin-only catalogs have no such declarations.
    fn declared_routine_names(&self) -> Vec<FullName> {
        Vec::new()
    }

    /// Whether an exactly qualified routine is an aggregate. Ordinary routines
    /// are the default; catalogs retaining aggregates must override this method.
    /// Callers must first resolve the routine through the selected catalog.
    fn is_aggregate_impl(&self, _name: &FullName) -> bool {
        false
    }
    /// Whether this exact function overload is scalar and preserves source rows
    /// in a view target list. Aggregate/window/set-returning calls return false.
    /// A provider lacking function-kind metadata must remain explicitly unimplemented.
    fn function_preserves_view_rows(
        &self,
        name: &FullName,
        overload: &crate::sem::OverloadVariant,
    ) -> bool;

    /// get `CREATE FUNCTION` statement by function name
    fn get_function_impl(&self, name: &FullName) -> Option<FunctionOverloadCollection>;
}

/// store and update function info
pub trait FunctionInfoWrite
where
    Self: Sized,
{
    /// Record source function properties separately from its readable signature.
    fn apply_function_view_rows(
        self,
        function: &CreateFunction,
        preserves_rows: bool,
    ) -> Result<Self, AnalysisError>;

    /// apply `create function` statement (implementation)
    fn apply_create_function_impl(
        self,
        name: &FullName,
        create_function: &CreateFunction,
    ) -> Result<Self, AnalysisError>;
}
