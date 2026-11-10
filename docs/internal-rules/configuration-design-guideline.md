# Configuration File Design Guidelines

Apply these principles when designing, implementing, or reviewing configuration
formats and their validation. A configuration file may be used with different
versions of sql-fun as users upgrade or downgrade the tool.

## Consider upgrades and downgrades

- For each proposed validation rule, consider whether a configuration accepted
  by a newer or older version would be rejected by the current version.
- Do not reject configuration solely because it contains fields the current
  version does not recognize. Prefer ordinary deserialization behavior that
  ignores unknown fields.
- Avoid restrictions on structure or combinations of fields unless they are
  necessary for the component's actual responsibility.
- Do not assume the configuration file was written specifically for the version
  currently reading it.

## Justify validation

Validation is not inherently beneficial. Before adding a check, identify:

1. **Responsibility:** Is this component responsible for the constraint?
2. **Necessity:** What concrete failure does the check prevent?
3. **Compatibility:** Could it reject a configuration accepted by another
   version of sql-fun, particularly after an upgrade or downgrade?
4. **Impact:** Would rejection prevent otherwise usable functionality from
   operating?

Do not introduce a check merely because a condition can be tested. When
validation has a real consumer requirement, keep it limited to that requirement.

## Separate decoding from interpretation

- Decoding reads configuration syntax and represents the fields it understands.
- Interpretation decides how those fields apply to the relevant functionality.
- Keep semantic constraints in the component that interprets and applies the
  configuration. Do not duplicate them in a shared configuration loader.
- Preserve supported data without enforcing downstream field combinations
  during decoding. This prevents changes in consumers from forcing unrelated
  changes to the loading layer.

## Errors

Distinguish failures to read or decode configuration from questions of its
meaning. Invalid syntax, I/O failures, and values that cannot be decoded into
known field types may be errors. Unknown fields and semantically inapplicable
data alone do not justify rejecting an entire configuration.

Prefer the smallest validation surface that protects the consuming operation
without unnecessarily limiting configuration compatibility.
