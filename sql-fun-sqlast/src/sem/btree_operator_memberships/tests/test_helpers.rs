use super::super::BtreeOperatorMemberships;

pub(super) struct CatalogFixture(tempfile::TempDir);
impl CatalogFixture {
    pub(super) fn memberships(values: &[serde_json::Value]) -> Self {
        Self::with_payload(
            &serde_json::json!({"format_version":1,"memberships":values}).to_string(),
        )
    }
    pub(super) fn legacy() -> Self {
        Self(tempfile::tempdir().unwrap())
    }
    pub(super) fn with_payload(content: &str) -> Self {
        let fixture = Self::legacy();
        std::fs::write(fixture.0.path().join("memberships.json"), content).unwrap();
        fixture
    }
    pub(super) fn load(
        &self,
    ) -> Result<Option<BtreeOperatorMemberships>, crate::sem::AnalysisError> {
        BtreeOperatorMemberships::load(&self.0.path().join("memberships.json"))
    }
}

pub(super) fn member(family: &str, strategy: u16) -> serde_json::Value {
    serde_json::json!({
        "operator":{"schema":"Custom 雪","name":"<~>"},
        "negator":{"schema":"Custom 雪","name":"different reverse edge"},
        "negated_by":[{"schema":"Custom 雪","name":"!~>"}],
        "left":{"name":{"schema":"pg_catalog","name":"int4"},"array":false},
        "right":{"name":{"schema":"pg_catalog","name":"int8"},"array":false},
        "family":{"schema":"Elsewhere","name":family,"access_method":"btree"},
        "family_left":{"schema":"pg_catalog","name":"int4"},
        "family_right":{"schema":"pg_catalog","name":"int8"},
        "strategy":strategy,"purpose":"s","sort_family":null
    })
}
