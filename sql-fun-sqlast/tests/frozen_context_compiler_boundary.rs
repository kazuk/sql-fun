//! Compile-time API boundary coverage for frozen schema contexts.

#[test]
fn frozen_context_compiler_boundary() {
    let cases = trybuild::TestCases::new();

    cases.pass("tests/ui/frozen_context/freeze_read_and_build_next.rs");
    cases.compile_fail("tests/ui/frozen_context/builder_is_consumed.rs");
    cases.compile_fail("tests/ui/frozen_context/frozen_context_has_no_mutators.rs");
    cases.compile_fail("tests/ui/frozen_context/mutable_context_cannot_be_a_frozen_base.rs");
    cases.compile_fail("tests/ui/frozen_context/frozen_handle_is_opaque.rs");
}
