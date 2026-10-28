use super::PgBuiltinCasts;
use crate::sem::CastContext;

#[rstest::rstest]
#[case::preferred_implicit("i", Some(true), CastContext::ImplicitPrefeerd)]
#[case::ordinary_implicit("i", Some(false), CastContext::Implicit)]
#[case::legacy_implicit("i", None, CastContext::Implicit)]
#[case::preferred_assignment("a", Some(true), CastContext::Assignment)]
#[case::preferred_explicit("e", Some(true), CastContext::Explicit)]
fn loader_preserves_preference_only_for_implicit_casts(
    #[case] context: &str,
    #[case] preferred: Option<bool>,
    #[case] expected: CastContext,
) -> testresult::TestResult {
    let catalog = test_helpers::cast_catalog(context, preferred)?;
    let (casts, _) = PgBuiltinCasts::load(catalog.path(), &crate::sem::ConflictMasters::sqlite())?;
    let cast = casts.get("source").unwrap().get("target").unwrap();
    assert_eq!(cast.context(), expected);
    assert_eq!(cast.use_in_implicit(), expected.use_in_implicit());
    assert_eq!(
        cast.context().overload_priority_score(),
        expected.overload_priority_score()
    );
    Ok(())
}

mod test_helpers {
    pub(super) fn cast_catalog(
        context: &str,
        preferred: Option<bool>,
    ) -> Result<::tempfile::NamedTempFile, Box<dyn std::error::Error>> {
        let mut record = ::serde_json::json!({"func": "0", "method": "b", "context": context});
        if let Some(preferred) = preferred {
            record["is_prefferd"] = ::serde_json::json!(preferred);
        }
        let file = ::tempfile::NamedTempFile::new()?;
        ::serde_json::to_writer(
            file.as_file(),
            &::serde_json::json!({"source": {"target": record}}),
        )?;
        Ok(file)
    }
}
