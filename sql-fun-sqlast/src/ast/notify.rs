//! Owns PostgreSQL NOTIFY syntax and frontend normalization without catalog binding.

use crate::{error::AnalysisError, syn};

/// Unresolved notification syntax; a channel is one identifier, not a relation.
///
/// Strings contain parser-normalized values. Payload omission remains distinct
/// from an explicitly empty string. Frontend offsets are not retained.
#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Notify {
    channel: String,
    payload: Option<String>,
}

impl TryFrom<&syn::NotifyStmt> for Notify {
    type Error = AnalysisError;

    fn try_from(value: &syn::NotifyStmt) -> Result<Self, Self::Error> {
        let channel = value.get_conditionname();
        if channel.is_empty() {
            return AnalysisError::raise_unexpected_input("NOTIFY channel");
        }
        let payload = value.get_payload();
        // pg_query 18 leaves the zero-initialized offset when the grammar omits
        // the payload; an explicit literal always follows NOTIFY and its channel.
        let payload = match value.get_payload_location() {
            0 if payload.is_empty() => None,
            location if location > 0 => Some(payload),
            _ => return AnalysisError::raise_unexpected_input("NOTIFY payload presence"),
        };
        Ok(Self { channel, payload })
    }
}

#[cfg(test)]
mod test_conversion {
    use crate::{ast::Notify, syn, test_helpers::notify::postgres_notify_syntax};

    #[rstest::rstest]
    #[case::unquoted("NOTIFY MiXeD", "mixed", None)]
    #[case::quoted(r#"NOTIFY "MiXeD.Channel""#, "MiXeD.Channel", None)]
    #[case::escaped_identifier(r#"NOTIFY "a""b""#, "a\"b", None)]
    #[case::empty("NOTIFY channel, ''", "channel", Some(""))]
    #[case::nonempty("NOTIFY channel, 'payload'", "channel", Some("payload"))]
    #[case::quoted_payload("NOTIFY channel, 'it''s retained'", "channel", Some("it's retained"))]
    #[case::escaped_payload(
        r#"NOTIFY channel, E'line\npath\\file'"#,
        "channel",
        Some("line\npath\\file")
    )]
    #[case::dollar_quoted(
        r#"NOTIFY channel, $p$it's \literal$p$"#,
        "channel",
        Some(r#"it's \literal"#)
    )]
    #[case::unicode(r#"NOTIFY "通知", 'é'"#, "通知", Some("é"))]
    fn preserves_normalized_frontend_facts(
        #[case] sql: &str,
        #[case] channel: &str,
        #[case] payload: Option<&str>,
    ) {
        let frontend = postgres_notify_syntax(sql);
        assert_eq!(frontend.conditionname, channel);
        assert_eq!(frontend.payload, payload.unwrap_or_default());
        // This assertion audits presence, not a diagnostic source span.
        assert_eq!(frontend.payload_location > 0, payload.is_some());
        if payload.is_none() {
            assert_eq!(frontend.payload_location, 0);
        }
        let common = Notify::try_from(&syn::NotifyStmt::from(frontend)).expect("NOTIFY syntax");
        assert_eq!(common.channel(), channel);
        assert_eq!(common.payload().as_deref(), payload);
    }

    #[test]
    #[should_panic(expected = "NOTIFY channel")]
    fn rejects_missing_frontend_channel() {
        let mut frontend = postgres_notify_syntax("NOTIFY channel");
        frontend.conditionname.clear();
        let _ = Notify::try_from(&syn::NotifyStmt::from(frontend));
    }

    #[rstest::rstest]
    #[case::missing_literal_location("NOTIFY channel, 'payload'", 0)]
    #[case::negative_location("NOTIFY channel, ''", -1)]
    #[should_panic(expected = "NOTIFY payload presence")]
    fn rejects_inconsistent_frontend_presence(#[case] sql: &str, #[case] location: i32) {
        let mut frontend = postgres_notify_syntax(sql);
        frontend.payload_location = location;
        let _ = Notify::try_from(&syn::NotifyStmt::from(frontend));
    }
}
