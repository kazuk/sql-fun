use crate::{
    context::{ConflictMasters, base_context::builtin_casts::PgBuiltinCasts},
    sem::CastContext,
};

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
    let (casts, _) = PgBuiltinCasts::load(catalog.path(), &ConflictMasters::sqlite())?;
    let cast = casts.get("source").unwrap().get("target").unwrap();
    assert_eq!(cast.context(), expected);
    assert_eq!(cast.use_in_implicit(), expected.use_in_implicit());
    assert_eq!(
        cast.context().overload_priority_score(),
        expected.overload_priority_score()
    );
    Ok(())
}

use crate::test_helpers::cast_catalog as test_helpers;
