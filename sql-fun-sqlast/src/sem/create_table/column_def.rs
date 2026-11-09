mod ddl;
use crate::{
    ast::Dialect,
    names::{ColumnName, FullName as NamesFullName, TableName},
    sem::{
        AnalysisError, CatalogNameLookup, ColumnConstraint, ColumnDrop,
        ColumnNullabilityConstraint, ColumnRename, ConstraintNames, DeclarationCollation,
        DefaultColumnConstraint, DropDependencyKind, GeneratedColumnConstraint, ParseContext,
        SemScalarExpr, TypeReference, WithTypmod,
        traversal::{ColumnRenamer, SemanticVisitorMut},
    },
};

/// column definitions
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ColumnDefinition {
    name: Option<ColumnName>,
    type_ref: Option<TypeReference>,
    #[deprecated = "query from constraints"]
    is_not_null: Option<bool>,
    #[deprecated = "query from constraints"]
    default: Option<Box<SemScalarExpr>>,
    constraints: Vec<ColumnConstraint>,
    #[serde(default, skip_serializing_if = "ColumnAttributes::is_empty")]
    attributes: ColumnAttributes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    declaration: Option<Box<ColumnDeclaration>>,
}

// Keep declaration-only metadata out of the inline size of every column binding.
#[derive(Debug, Clone, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
struct ColumnDeclaration {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    rejected: bool,
    requested_type: Option<WithTypmod<TypeReference>>,
    collation: DeclarationCollation,
}

/// Persistent PostgreSQL per-column planning attributes.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
struct ColumnAttributes {
    statistics_target: Option<i32>,
    n_distinct: Option<String>,
    n_distinct_inherited: Option<String>,
}
impl ColumnAttributes {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

impl ColumnDefinition {
    pub(crate) fn reserve_check_names(&self, names: &mut ConstraintNames) {
        for constraint in &self.constraints {
            if let Some(check) = constraint.as_check() {
                names.reserve(check.name().unwrap_or_default());
            }
        }
    }

    #[expect(deprecated)]
    pub(crate) fn default_expression(&self) -> Option<&SemScalarExpr> {
        self.constraints
            .iter()
            .find_map(|constraint| constraint.as_default().map(DefaultColumnConstraint::expr))
            .or(self.default.as_deref())
    }

    pub(crate) fn set_projection_name(&mut self, name: &ColumnName) {
        self.name = Some(name.clone());
    }
    pub(crate) fn rename_column_name(&mut self, rename: &ColumnRename) {
        self.rename_constraint_columns(rename, true);
        if let Some(name) = &mut self.name {
            rename.rename_name(name);
        }
    }
    pub(crate) fn rename_column_dependencies(
        &mut self,
        rename: &ColumnRename,
        owned: bool,
    ) -> Result<(), AnalysisError> {
        self.rename_constraint_columns(rename, owned);
        self.rename_expressions(&mut ColumnRenamer::new(rename))
    }
    fn rename_constraint_columns(&mut self, rename: &ColumnRename, owned: bool) {
        for constraint in &mut self.constraints {
            constraint.rename_column(rename, owned);
        }
    }

    /// Requested declaration type and modifiers, separate from canonical identity.
    pub fn requested_type(&self) -> Option<&WithTypmod<TypeReference>> {
        self.declaration.as_ref()?.requested_type.as_ref()
    }

    pub(crate) fn retain_requested_type(&mut self, value: &WithTypmod<TypeReference>) {
        let declaration = self.declaration.get_or_insert_default();
        declaration.requested_type = Some(value.clone());
    }

    /// Retained explicit declaration collation analysis.
    pub fn declaration_collation(&self) -> &DeclarationCollation {
        if let Some(declaration) = &self.declaration {
            &declaration.collation
        } else {
            DeclarationCollation::omitted()
        }
    }

    pub(crate) fn validate_collation(
        &mut self,
        context: &mut impl ParseContext,
        lookup: Option<&CatalogNameLookup>,
    ) -> Result<(), AnalysisError> {
        let declaration = self.declaration.get_or_insert_default();
        declaration.collation =
            DeclarationCollation::validate(context, self.type_ref.as_ref(), lookup);
        declaration
            .collation
            .report(context, self.type_ref.as_ref())
    }

    pub(crate) fn reject_declaration(&mut self) {
        self.declaration.get_or_insert_default().rejected = true;
    }

    pub(crate) fn has_check_name(&self, name: &str) -> bool {
        self.constraints.iter().any(|constraint| {
            constraint
                .as_check()
                .is_some_and(|check| check.name() == Some(name))
        })
    }

    pub(crate) fn declaration_can_apply(&self) -> bool {
        if self.constraints.iter().any(|constraint| {
            constraint
                .as_check()
                .is_some_and(|check| !check.can_apply())
        }) {
            return false;
        }
        let Some(declaration) = &self.declaration else {
            return true;
        };
        // An absent type is not itself a failed declaration (SQLite permits
        // typeless columns, and query-derived columns may remain unknown).
        let type_resolved = declaration.requested_type.is_none() || self.type_ref.is_some();
        !declaration.rejected && type_resolved && declaration.collation.can_apply()
    }

    pub(super) fn rename_expressions(
        &mut self,
        visitor: &mut impl SemanticVisitorMut,
    ) -> Result<(), AnalysisError> {
        for constraint in &mut self.constraints {
            constraint.rename_expression(visitor)?;
        }
        Ok(())
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        for constraint in &mut self.constraints {
            constraint.rename_relation(old, new);
        }
        if let Some(ty) = &mut self.type_ref {
            ty.rename_relation(old, new);
        }
    }

    /// Explicit statistics target; None uses PostgreSQL's configured default.
    pub const fn statistics_target(&self) -> Option<i32> {
        self.attributes.statistics_target
    }
    /// Explicit n_distinct estimate retained from ALTER COLUMN SET.
    pub fn distinct_estimate(&self) -> Option<f64> {
        if let Some(value) = &self.attributes.n_distinct {
            value.trim().parse().ok()
        } else {
            None
        }
    }
    /// Explicit n_distinct_inherited estimate retained from ALTER COLUMN SET.
    pub fn inherited_distinct_estimate(&self) -> Option<f64> {
        if let Some(value) = &self.attributes.n_distinct_inherited {
            value.trim().parse().ok()
        } else {
            None
        }
    }
    pub(crate) const fn apply_statistics_target(&mut self, value: Option<i32>) {
        self.attributes.statistics_target = value;
    }
    pub(crate) fn apply_distinct_option(&mut self, name: &str, value: Option<&str>) {
        match name {
            "n_distinct" => self.attributes.n_distinct = value.map(str::to_owned),
            "n_distinct_inherited" => {
                self.attributes.n_distinct_inherited = value.map(str::to_owned)
            }
            _ => unreachable!("only validated column options are applied"),
        }
    }
    pub(crate) fn drop_generation_expression(&mut self) {
        self.constraints
            .retain(|constraint| constraint.as_generated().is_none());
    }
    pub(crate) fn apply_attribute_nullability(&mut self, table: &TableName, value: bool) {
        self.constraints
            .retain(|constraint| constraint.as_nullability().is_none());
        if let Some(column) = &self.name {
            self.constraints.push(ColumnConstraint::nullability(
                ColumnNullabilityConstraint::new(table, column, value, None, None),
            ));
        }
        // Keep the existing query metadata accessor in sync with the constraint.
        self.set_not_null(Some(value));
    }
    pub(crate) fn has_identity(&self) -> bool {
        self.constraints
            .iter()
            .any(|constraint| constraint.as_identity().is_some())
    }
    pub(crate) fn has_inline_primary_key(&self) -> bool {
        self.constraints
            .iter()
            .any(|constraint| constraint.as_primary_key().is_some())
    }
    pub(crate) fn has_generation(&self) -> bool {
        self.constraints
            .iter()
            .any(|constraint| constraint.as_generated().is_some())
    }
    pub(crate) fn has_stored_generation(&self) -> bool {
        self.constraints.iter().any(|constraint| {
            constraint
                .as_generated()
                .is_some_and(GeneratedColumnConstraint::is_stored)
        })
    }
    pub(crate) fn has_virtual_generation(&self) -> bool {
        self.constraints.iter().any(|constraint| {
            constraint
                .as_generated()
                .is_some_and(GeneratedColumnConstraint::is_virtual)
        })
    }
}

impl Clone for ColumnDefinition {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            type_ref: self.type_ref.clone(),
            #[expect(deprecated)]
            is_not_null: self.is_not_null,
            #[expect(deprecated)]
            default: self.default.clone(),
            constraints: self.constraints.clone(),
            attributes: self.attributes.clone(),
            declaration: self.declaration.clone(),
        }
    }
}

impl ColumnDefinition {
    pub(crate) fn replace_view_default(&mut self, view: &NamesFullName, value: &SemScalarExpr) {
        self.constraints
            .retain(|constraint| constraint.as_default().is_none());
        if let Some(name) = &self.name {
            self.constraints
                .push(ColumnConstraint::default(DefaultColumnConstraint::new(
                    &TableName::from(view.clone()),
                    name,
                    value.clone(),
                )));
        }
    }

    /// create instance
    #[must_use]
    pub fn new(
        name: &Option<ColumnName>,
        type_ref: Option<&TypeReference>,
        is_not_null: Option<bool>,
        constraints: &[ColumnConstraint],
    ) -> Self {
        Self {
            name: name.clone(),
            type_ref: type_ref.cloned(),
            #[expect(deprecated)]
            is_not_null,
            #[expect(deprecated)]
            default: Default::default(),
            constraints: constraints.to_vec(),
            attributes: ColumnAttributes::default(),
            declaration: None,
        }
    }
}

#[cfg(test)]
mod test_column_serialization {
    //! SQL-to-semantics and catalog-chain regressions; no external server is required.

    use crate::{
        sem::ColumnDefinition as SemColumnDefinition,
        test_helpers::column_attributes as test_helpers,
    };

    #[test]
    fn legacy_column_json_shape_remains_readable() -> testresult::TestResult {
        let context = test_helpers::seed("CREATE TABLE t(a integer)", "17")?;
        let original = test_helpers::column(&context, "a");
        let json = ::serde_json::to_value(original)?;
        assert!(json.get("attributes").is_none());
        let restored: SemColumnDefinition = ::serde_json::from_value(json)?;
        assert_eq!(&restored, original);
        Ok(())
    }
}

impl ColumnDefinition {
    /// get name of column
    #[must_use]
    pub const fn name(&self) -> &Option<ColumnName> {
        &self.name
    }

    /// get [`TypeReference`]
    #[must_use]
    pub fn get_type_cloned(&self) -> Option<TypeReference> {
        self.type_ref.clone()
    }

    /// get [`TypeReference`]
    #[must_use]
    pub const fn get_type(&self) -> Option<&TypeReference> {
        self.type_ref.as_ref()
    }

    /// column nullability
    #[must_use]
    #[expect(deprecated)]
    pub const fn is_not_null(&self) -> Option<bool> {
        self.is_not_null
    }

    /// apply default expression to column
    #[expect(deprecated)]
    pub fn set_default(&mut self, scalar_expr: &SemScalarExpr) {
        self.default = Some(Box::new(scalar_expr.clone()));
    }

    /// apply not null constraint to column
    #[expect(deprecated)]
    pub const fn set_not_null(&mut self, not_null: Option<bool>) {
        self.is_not_null = not_null;
    }

    pub(crate) fn apply_primary_key_nullability(&mut self) {
        if self
            .constraints
            .iter()
            .any(|constraint| constraint.as_primary_key().is_some())
        {
            self.set_not_null(Some(true));
        }
    }

    pub(super) fn has_unique_key(&self) -> bool {
        self.constraints.iter().any(|constraint| {
            constraint.as_primary_key().is_some() || constraint.as_unique().is_some()
        })
    }

    /// get column constraints
    #[must_use]
    pub const fn constraints(&self) -> &[ColumnConstraint] {
        self.constraints.as_slice()
    }

    /// add column constraint
    pub fn push_constraint(&mut self, constraint: ColumnConstraint) {
        self.constraints.push(constraint)
    }

    pub(crate) fn insert_inline_check(&mut self, index: usize, check: &ColumnConstraint) {
        self.constraints.insert(index, check.clone());
    }
}

impl ColumnDefinition {
    pub(crate) fn collect_drop_dependencies(
        &self,
        context: &impl ParseContext,
        change: &mut ColumnDrop,
        owner: &NamesFullName,
        owned: bool,
    ) -> Result<(), AnalysisError> {
        let sqlite = context.dialect() == Dialect::sqlite();
        let removed = owned
            && self
                .name()
                .as_ref()
                .is_some_and(|name| change.matches_name(context, name));
        for (index, constraint) in self.constraints.iter().enumerate() {
            let anonymous = format!(
                "{}#{index}",
                self.name().as_ref().map_or("", ColumnName::as_str)
            );
            if removed {
                if constraint.as_primary_key().is_some() || constraint.as_unique().is_some() {
                    change.dependency(
                        owner.clone(),
                        &anonymous,
                        DropDependencyKind::constraint(),
                        !sqlite,
                    );
                }
                continue;
            }
            if let Some(check) = constraint.as_check()
                && owned
                && change.expression_dependency(context, check.expr())?
            {
                change.dependency(
                    owner.clone(),
                    check.name().unwrap_or(&anonymous),
                    DropDependencyKind::constraint(),
                    !sqlite,
                );
            }
            if let Some(generated) = constraint.as_generated()
                && owned
                && change.expression_dependency(context, generated.expression())?
            {
                change.dependency(
                    owner.clone(),
                    self.name().as_ref().map_or("", ColumnName::as_str),
                    DropDependencyKind::generated_column(),
                    false,
                );
            }
            if let Some(key) = constraint.as_foreign_key()
                && !sqlite
                && key.primary_key_table() == Some(change.table())
                && change.matches_name(context, key.primary_key_column())
            {
                change.dependency(
                    owner.clone(),
                    &anonymous,
                    DropDependencyKind::constraint(),
                    owned,
                );
            }
        }
        Ok(())
    }
}
