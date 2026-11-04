use crate::{names::ColumnName, sem::ColumnDefinition};

/// INSTEAD OF row-trigger events available on a MERGE view target.
#[derive(
    Debug, Clone, Copy, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize,
)]
pub struct ViewMergeTriggers {
    insert: bool,
    update: bool,
    delete: bool,
}
impl ViewMergeTriggers {
    /// Record which action events have an INSTEAD OF row trigger.
    pub const fn new(insert: bool, update: bool, delete: bool) -> Self {
        Self {
            insert,
            update,
            delete,
        }
    }

    const fn any(self) -> bool {
        self.insert || self.update || self.delete
    }
}

/// MERGE-specific catalog decision for an ordinary view target.
///
/// This is intentionally independent of ordinary INSERT, UPDATE and DELETE
/// capabilities. PostgreSQL permits MERGE on a view with no rules when it is
/// automatically updatable and has no INSTEAD OF triggers, or when every
/// action named by the MERGE statement has a corresponding INSTEAD OF trigger.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ViewMergeSupport {
    automatically_updatable: bool,
    has_rules: bool,
    triggers: ViewMergeTriggers,
    insert_columns: Option<Vec<ColumnDefinition>>,
    update_columns: Option<Vec<ColumnDefinition>>,
}
impl ViewMergeSupport {
    /// Build explicit MERGE view metadata from catalog facts.
    /// `has_rules` refers to non-SELECT action rules, excluding the defining
    /// SELECT rule that every PostgreSQL view has.
    ///
    /// `None` column lists mean that action cannot assign through the view;
    /// `Some(&[])` permits the action with no explicit assignments, such as
    /// `INSERT DEFAULT VALUES`.
    pub fn new(
        automatically_updatable: bool,
        has_rules: bool,
        triggers: ViewMergeTriggers,
        insert_columns: Option<&[ColumnDefinition]>,
        update_columns: Option<&[ColumnDefinition]>,
    ) -> Self {
        Self {
            automatically_updatable,
            has_rules,
            triggers,
            insert_columns: insert_columns.map(<[ColumnDefinition]>::to_vec),
            update_columns: update_columns.map(<[ColumnDefinition]>::to_vec),
        }
    }
}

impl ViewMergeSupport {
    /// Whether the view can be a target for all action types present in MERGE.
    pub const fn supports_actions(&self, insert: bool, update: bool, delete: bool) -> bool {
        if self.has_rules {
            return false;
        }
        if self.automatically_updatable && !self.triggers.any() {
            return true;
        }
        (!insert || self.triggers.insert)
            && (!update || self.triggers.update)
            && (!delete || self.triggers.delete)
    }
}

#[cfg(test)]
mod test_supports_actions {

    use crate::sem::merge_statement::view_support::{ViewMergeSupport, ViewMergeTriggers};

    #[test]
    fn automatically_updatable_view_without_triggers_supports_all_actions() {
        let support = ViewMergeSupport::new(true, false, ViewMergeTriggers::default(), None, None);

        assert!(support.supports_actions(true, true, true));
        assert!(support.supports_actions(false, true, false));
    }

    #[test]
    fn trigger_backed_view_requires_each_action_trigger() {
        let support = ViewMergeSupport::new(
            false,
            false,
            ViewMergeTriggers::new(true, false, true),
            None,
            None,
        );

        assert!(support.supports_actions(true, false, true));
        assert!(!support.supports_actions(true, true, true));
        assert!(!support.supports_actions(false, true, false));
    }

    #[test]
    fn rules_reject_merge_even_with_automatic_updates_and_all_triggers() {
        let support = ViewMergeSupport::new(
            true,
            true,
            ViewMergeTriggers::new(true, true, true),
            None,
            None,
        );

        assert!(!support.supports_actions(true, true, true));
    }

    #[test]
    fn automatic_update_with_any_trigger_uses_per_action_trigger_rules() {
        let support = ViewMergeSupport::new(
            true,
            false,
            ViewMergeTriggers::new(false, true, false),
            None,
            None,
        );

        assert!(support.supports_actions(false, true, false));
        assert!(!support.supports_actions(true, false, false));
    }
}

impl ViewMergeSupport {
    /// Whether an INSERT action can use this view's effective column metadata.
    pub const fn supports_insert(&self) -> bool {
        self.insert_columns.is_some()
    }

    /// Whether an UPDATE action can use this view's effective column metadata.
    pub const fn supports_update(&self) -> bool {
        self.update_columns.is_some()
    }

    /// Effective assignment metadata for a MERGE INSERT target column.
    pub fn insert_column_definition(&self, name: &ColumnName) -> Option<&ColumnDefinition> {
        self.insert_columns
            .as_ref()?
            .iter()
            .find(|column| column.name().as_ref() == Some(name))
    }

    /// Effective assignment metadata for a MERGE UPDATE target column.
    pub fn update_column_definition(&self, name: &ColumnName) -> Option<&ColumnDefinition> {
        self.update_columns
            .as_ref()?
            .iter()
            .find(|column| column.name().as_ref() == Some(name))
    }
}

#[cfg(test)]
mod tests {
    use crate::names::ColumnName;
    use crate::test_helpers::merge_views::column_definition as definition;

    use crate::sem::merge_statement::view_support::{ViewMergeSupport, ViewMergeTriggers};

    #[test]
    fn action_columns_distinguish_default_values_and_assignable_columns() {
        let insert_column = definition("inserted");
        let update_column = definition("updated");
        let support = ViewMergeSupport::new(
            true,
            false,
            ViewMergeTriggers::default(),
            Some(std::slice::from_ref(&insert_column)),
            Some(std::slice::from_ref(&update_column)),
        );

        assert!(support.supports_insert());
        assert!(support.supports_update());
        assert!(
            support
                .insert_column_definition(&ColumnName::from("inserted"))
                .is_some()
        );
        assert!(
            support
                .insert_column_definition(&ColumnName::from("updated"))
                .is_none()
        );
        assert!(
            support
                .update_column_definition(&ColumnName::from("updated"))
                .is_some()
        );

        let default_only =
            ViewMergeSupport::new(true, false, ViewMergeTriggers::default(), Some(&[]), None);
        assert!(default_only.supports_insert());
        assert!(!default_only.supports_update());
    }
}
