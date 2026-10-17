# Writable-view semantic catalog fixture

These small PostgreSQL 17 catalog inputs cover the integer, text, array,
assignment, and conflict-inference behavior used by `writable_views.rs`.
They retain the relevant entries from the repository's PostgreSQL 17 masters;
functions and unary operators are intentionally empty. They are test data,
not a replacement builtin distribution or a generated artifact.

Routine semantic cases load this fixture independently, including under
nextest. Tests of extracted builtin function properties and `pg_settings`
rules explicitly use `test_helpers::heavy` and the full production masters.
All tests run in the standard workspace test flow.
