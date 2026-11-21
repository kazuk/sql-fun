use crate::sem::{AnalysisError, traversal::SemanticVisitor};
mod ddl;
use crate::{
    ast::ObjectKind,
    names::{FullName, TableName},
    sem::{
        AlterFunctionParallel, ColumnCollection, FunctionOverloadCollection,
        ObjectPrivilegeDeclaration, OverloadVariant, OwnerSpecification, OwnershipIdentity,
        ParallelSafety, ParseContext, SourcePathWithSpan, type_system::ArgumentBindingCollection,
    },
};

/// Analyzed CREATE FUNCTION statement.
#[derive(Debug, Clone)]
pub struct CreateFunction {
    replace: bool,
    parallel_safety: Option<ParallelSafety>,
    parallel_changes: Vec<AlterFunctionParallel>,
    source: Option<SourcePathWithSpan>,

    owner: Option<OwnerSpecification>,
    object_kind: ObjectKind,
    name: FullName,
    overloads: FunctionOverloadCollection,
    returning_columdef: Option<ColumnCollection>,
    has_sql_body: bool,
    builtin_properties: Option<(bool, bool)>,
    privilege_declarations: Vec<ObjectPrivilegeDeclaration>,
}

impl CreateFunction {
    /// Object privilege declarations in statement order.
    #[must_use]
    pub fn privilege_declarations(&self) -> &[ObjectPrivilegeDeclaration] {
        &self.privilege_declarations
    }

    pub(crate) fn append_privilege(&mut self, declaration: &ObjectPrivilegeDeclaration) {
        self.privilege_declarations.push(declaration.clone());
    }

    pub(crate) const fn has_sql_body(&self) -> bool {
        self.has_sql_body
    }

    pub(crate) fn visit_references<V: SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for overload in &self.overloads {
            overload.visit_references(visitor)?;
        }
        if let Some(columns) = &self.returning_columdef {
            columns.visit_references(visitor)?;
        }
        Ok(())
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) -> bool {
        let previous = self.clone();
        self.overloads.rename_relation(old, new);
        if let Some(columns) = &mut self.returning_columdef {
            columns.rename_relation(old, new);
        }
        !self.overloads.same_signatures(&previous.overloads)
            || self.returning_columdef != previous.returning_columdef
    }

    /// Parallel safety recorded for this definition, if available.
    pub const fn parallel_safety(&self) -> Option<ParallelSafety> {
        self.parallel_safety
    }

    /// Applied ALTER FUNCTION PARALLEL statements in source order.
    pub fn parallel_changes(&self) -> &[AlterFunctionParallel] {
        &self.parallel_changes
    }

    pub(crate) fn change_parallel_safety(&mut self, statement: &AlterFunctionParallel) {
        self.parallel_safety = Some(statement.mode());
        self.parallel_changes.push(statement.clone());
    }

    /// Source of the declaration, when the catalog supplies one.
    pub const fn source(&self) -> Option<&SourcePathWithSpan> {
        self.source.as_ref()
    }

    pub(crate) const fn replace(&self) -> bool {
        self.replace
    }
    pub(crate) const fn with_replace(mut self, value: bool) -> Self {
        self.replace = value;
        self
    }

    /// Retained ownership; None means no ownership information was supplied.
    pub const fn owner(&self) -> Option<&OwnerSpecification> {
        self.owner.as_ref()
    }
    /// Commit an ownership change, leaving the rest of the definition intact.
    pub(crate) fn change_owner(&mut self, owner: &OwnerSpecification) {
        self.owner = Some(owner.clone());
    }

    /// create new instance
    #[must_use]
    pub fn new(
        name: &FullName,
        overloads: &FunctionOverloadCollection,
        returning_columns: &Option<ColumnCollection>,
    ) -> Self {
        Self {
            replace: false,
            parallel_safety: None,
            parallel_changes: Vec::new(),
            source: None,
            builtin_properties: None,
            owner: None,
            privilege_declarations: Vec::new(),
            object_kind: ObjectKind::from(20),
            name: name.clone(),
            overloads: overloads.clone(),
            returning_columdef: returning_columns.clone(),
            has_sql_body: false,
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
}

impl CreateFunction {
    pub(crate) fn previous_definition<'a>(
        &self,
        existing: &'a [Self],
        pending: &'a [Self],
    ) -> Option<&'a Self> {
        for definitions in [existing, pending] {
            for previous in definitions {
                if self.same_signature(previous) {
                    return Some(previous);
                }
            }
        }
        None
    }
    fn same_signature(&self, other: &Self) -> bool {
        self.overloads
            .get_at(0)
            .map(OverloadVariant::identity_arguments)
            == other
                .overloads
                .get_at(0)
                .map(OverloadVariant::identity_arguments)
    }
    pub(crate) fn can_replace(&self, other: &Self) -> bool {
        self.object_kind == other.object_kind
            && self.returning_columdef == other.returning_columdef
            && self.overloads.get_at(0).map(OverloadVariant::ret_type)
                == other.overloads.get_at(0).map(OverloadVariant::ret_type)
    }
    pub(crate) fn contains_identity(
        definitions: &[Self],
        name: &FullName,
        identity: &OwnershipIdentity,
    ) -> bool {
        for definition in definitions {
            if definition.ownership_identity(name) == *identity {
                return true;
            }
        }
        false
    }
    pub(crate) fn with_procedure_kind(mut self, procedure: bool) -> Self {
        if procedure {
            self.object_kind = ObjectKind::from(30);
        }
        self
    }
    /// Actual routine kind (FUNCTION or PROCEDURE).
    pub const fn object_kind(&self) -> ObjectKind {
        self.object_kind
    }
    pub(crate) fn retain_privilege_declarations(&mut self, previous: &Self) {
        self.privilege_declarations
            .clone_from(&previous.privilege_declarations);
    }

    pub(crate) const fn set_builtin_properties(&mut self, aggregate: bool, returns_set: bool) {
        self.builtin_properties = Some((aggregate, returns_set));
    }

    pub(crate) const fn catalog_aggregate(&self) -> bool {
        matches!(self.builtin_properties, Some((true, _)))
    }

    pub(crate) fn catalog_returns_set(&self) -> bool {
        self.builtin_properties
            .map_or_else(|| self.is_table_function(), |(_, returns_set)| returns_set)
    }

    pub(crate) fn split_definitions(&self) -> Vec<Self> {
        let mut definitions = Vec::new();
        for overload in &self.overloads {
            let mut definition = self.clone();
            definition.overloads = FunctionOverloadCollection::new(std::slice::from_ref(overload));
            definitions.push(definition);
        }
        definitions
    }
    pub(crate) fn ownership_identity(&self, key: &FullName) -> OwnershipIdentity {
        let arguments = self
            .overloads
            .get_at(0)
            .map_or_else(Vec::new, OverloadVariant::identity_arguments);
        OwnershipIdentity::with_arguments(self.object_kind, key, &arguments)
    }
}
