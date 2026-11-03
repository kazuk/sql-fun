use sql_fun_core::IVec;

use crate::{
    ast::TryFromWithTokens,
    names::FullName,
    sem::{
        AnalysisError, AstAndContextPair, ColumnCollection, FunctionOverloadCollection,
        OverloadVariant, ParseContext, type_system::ArgumentBindingCollection,
    },
    syn::ScanToken,
};

/// analyzed [`crate::syn::CreateFunctionStmt`]
#[derive(Debug, Clone)]
pub struct CreateFunction {
    replace: bool,

    owner: Option<crate::sem::OwnerSpecification>,
    object_kind: crate::sem::ObjectKind,
    name: FullName,
    overloads: FunctionOverloadCollection,
    returning_columdef: Option<ColumnCollection>,
}

impl CreateFunction {
    pub(crate) fn rename_relation(
        &mut self,
        old: &crate::names::TableName,
        new: &crate::names::TableName,
    ) -> bool {
        let previous = self.clone();
        self.overloads.rename_relation(old, new);
        if let Some(columns) = &mut self.returning_columdef {
            columns.rename_relation(old, new);
        }
        !self.overloads.same_signatures(&previous.overloads)
            || self.returning_columdef != previous.returning_columdef
    }

    pub(crate) const fn replace(&self) -> bool {
        self.replace
    }
    pub(crate) const fn with_replace(mut self, value: bool) -> Self {
        self.replace = value;
        self
    }

    /// Retained ownership; None means no ownership information was supplied.
    pub const fn owner(&self) -> Option<&crate::sem::OwnerSpecification> {
        self.owner.as_ref()
    }
    /// Commit an ownership change, leaving the rest of the definition intact.
    pub(crate) fn change_owner(&mut self, owner: &crate::sem::OwnerSpecification) {
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
            owner: None,
            object_kind: crate::sem::ObjectKind::from(20),
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
        crate::ast::Function::try_from_with_tokens(&syn, tokens)?.resolve_and_apply(context)
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
        identity: &crate::sem::OwnershipIdentity,
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
            self.object_kind = crate::sem::ObjectKind::from(30);
        }
        self
    }
    /// Actual routine kind (FUNCTION or PROCEDURE).
    pub const fn object_kind(&self) -> crate::sem::ObjectKind {
        self.object_kind
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
    pub(crate) fn ownership_identity(&self, key: &FullName) -> crate::sem::OwnershipIdentity {
        let arguments = self
            .overloads
            .get_at(0)
            .map_or_else(Vec::new, OverloadVariant::identity_arguments);
        crate::sem::OwnershipIdentity::with_arguments(self.object_kind, key, &arguments)
    }
}
