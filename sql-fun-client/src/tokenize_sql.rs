use crate::payloads::{SqlToken, TokenizeSqlArgs};
use crate::{Collection, TcpJsonRpcClientAsync, TypedResponseError};
use ::sql_fun_core::{SqlDialect, SqlToken as CoreToken, TokenStream};

impl TcpJsonRpcClientAsync {
    /// Fetch all presentation tokens and release their collection before building
    /// a lossless stream from the caller's original source and explicit dialect.
    /// Uses the existing connection, without reading files or initializing schemas.
    ///
    /// # Errors
    /// Returns lexical, transport, completion, release, or invalid token-range errors.
    /// Cancellation follows the same guarantees as `invoke_typed`.
    pub async fn tokenize_sql(
        &mut self,
        source: &str,
        dialect: SqlDialect,
    ) -> Result<TokenStream, TypedResponseError> {
        let params = ::serde_json::to_value(TokenizeSqlArgs::new(source, dialect))?;
        let (sender, receiver) = ::tokio::sync::oneshot::channel();
        self.invoke_typed(
            "tokenize_sql",
            &params,
            move |response: Result<Collection<SqlToken>, TypedResponseError>| async move {
                let _ = sender.send(response);
                Ok(())
            },
        )
        .await?;
        let tokens = receiver
            .await
            .map_err(|error| TypedResponseError::message(&error.to_string()))??;
        tokens.token_stream(source, dialect)
    }
}

impl Collection<SqlToken> {
    fn token_stream(
        &self,
        source: &str,
        dialect: SqlDialect,
    ) -> Result<TokenStream, TypedResponseError> {
        if !self.is_completed() {
            return Err(TypedResponseError::message(
                "token collection has not completed and released",
            ));
        }
        let mut tokens = Vec::new();
        let mut end = 0;
        for token in self.items() {
            let token = CoreToken::from(token);
            let span = token.span();
            if span.start != end || span.start >= span.end || source.get(span.clone()).is_none() {
                return Err(TypedResponseError::message(
                    "invalid or incomplete SQL token ranges",
                ));
            }
            end = span.end;
            tokens.push(token);
        }
        if end != source.len() {
            return Err(TypedResponseError::message("incomplete SQL token coverage"));
        }
        TokenStream::new(source, dialect, tokens)
            .map_err(|error| TypedResponseError::message(&error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use crate::{Collection, payloads::SqlToken};
    use ::sql_fun_core::SqlDialect;

    #[rstest::rstest]
    #[case::gap(" a", r#"[{"kind":"identifier","start":1,"end":2}]"#)]
    #[case::trailing_whitespace("a ", r#"[{"kind":"identifier","start":0,"end":1}]"#)]
    #[case::overlap(
        "aa",
        r#"[{"kind":"identifier","start":0,"end":2},{"kind":"identifier","start":1,"end":2}]"#
    )]
    #[case::empty_range("a", r#"[{"kind":"identifier","start":0,"end":0}]"#)]
    #[case::split_utf8("名", r#"[{"kind":"identifier","start":0,"end":1}]"#)]
    #[case::out_of_bounds("a", r#"[{"kind":"identifier","start":0,"end":9}]"#)]
    #[case::reversed("a", r#"[{"kind":"identifier","start":1,"end":0}]"#)]
    fn rejects_invalid_coverage(#[case] source: &str, #[case] json: &str) {
        let tokens: Vec<SqlToken> = ::serde_json::from_str(json).unwrap();
        assert!(
            Collection::completed(tokens)
                .token_stream(source, SqlDialect::PostgreSQL)
                .is_err()
        );
    }

    #[test]
    fn refuses_unreleased_collection() {
        let collection: Collection<SqlToken> = ::serde_json::from_value(::serde_json::json!({
            "handle":"retained", "total_length":0,"offset":0,"require_release":true,"items":[]
        }))
        .unwrap();
        assert!(collection.token_stream("", SqlDialect::PostgreSQL).is_err());
    }
}

#[cfg(test)]
mod test_helpers;

#[cfg(test)]
mod transport_tests {
    use super::test_helpers::{TokenPeer, continuation};
    use ::sql_fun_core::SqlDialect;

    #[tokio::test]
    async fn collects_and_releases_before_returning_original_utf8_stream() {
        let page = continuation(&::serde_json::json!([
            {"kind":"trivia","start":6,"end":7},
            {"kind":"identifier","start":7,"end":10}
        ]));
        let mut peer = TokenPeer::start(page, true).await;
        let stream = peer
            .client()
            .tokenize_sql("SELECT 名", SqlDialect::PostgreSQL)
            .await
            .unwrap();
        assert_eq!(stream.source(), "SELECT 名");
        assert_eq!(stream.dialect(), SqlDialect::PostgreSQL);
        assert_eq!(stream.tokens().count(), 3);
        let reconstructed: String = stream
            .tokens()
            .map(|t| &stream.source()[t.span()])
            .collect();
        assert_eq!(reconstructed, stream.source());
        assert_eq!(peer.finish().await, 1);
    }

    #[rstest::rstest]
    #[case::no_progress(r"[]", true)]
    #[case::decoding(
        r#"[{"kind":"unknown","start":6,"end":7},{"kind":"identifier","start":7,"end":10}]"#,
        true
    )]
    #[case::invalid_utf8(
        r#"[{"kind":"trivia","start":6,"end":7},{"kind":"identifier","start":7,"end":9}]"#,
        true
    )]
    #[case::missing_whitespace(
        r#"[{"kind":"identifier","start":7,"end":8},{"kind":"identifier","start":8,"end":10}]"#,
        true
    )]
    #[case::release_failure(
        r#"[{"kind":"trivia","start":6,"end":7},{"kind":"identifier","start":7,"end":10}]"#,
        false
    )]
    #[tokio::test]
    async fn does_not_return_partial_or_unreleased_stream(
        #[case] items: &str,
        #[case] release_ok: bool,
    ) {
        let page = continuation(&::serde_json::from_str(items).unwrap());
        let mut peer = TokenPeer::start(page, release_ok).await;
        assert!(
            peer.client()
                .tokenize_sql("SELECT 名", SqlDialect::PostgreSQL)
                .await
                .is_err()
        );
        assert_eq!(peer.finish().await, 1);
    }
}
