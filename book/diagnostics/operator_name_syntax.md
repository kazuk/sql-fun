# operator_name_syntax

## Code and severity

Code: `operator_name_syntax`. Severity: Error.

## When it is reported

An operator identity has malformed components, such as a present empty name,
an empty component, or a syntax node of the wrong kind.

## What analysis knows and does not know

The supplied shape cannot identify a valid operator. Analysis retains an invalid
operator identity and does not resolve it against the catalog.

## How to address it

Supply a valid operator name. If a program constructs analysis inputs directly,
provide nonempty string components for the operator and optional schema.

## Example

Not applicable as a parser-accepted SQL reproducer: the PostgreSQL parser rejects
these malformed shapes before operator-name conversion. An API-supplied present
empty component list illustrates this condition; an absent optional name is
handled separately and does not itself report this diagnostic.

## Related diagnostics

[cross_database_operator_reference](cross_database_operator_reference.md)
identifies parser-accepted references with database qualification.
[query_parse](query_parse.md) reports SQL rejected during parsing.
