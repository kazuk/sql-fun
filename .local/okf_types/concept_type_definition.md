---
type: Concept Type Definition
defined_type: Concept Type Definition
---

## Type Description

`Concept Type Definition` defines a concept type.

## Frontmatter Fields

- `defined_type`: REQUIRED Text, Type name for this definition
- `tools`: OPTIONAL List, tools that operate on concepts of `defined_type`.
  - `kind`: REQUIRED Text, kind of tool, such as validator or exporter.
  - `description`: OPTIONAL Text, short description of the tool.
  - `command`: REQUIRED Text, command line used to invoke the tool.
  - `link`: OPTIONAL Text, link to detailed documentation for the tool.

## Markdown Content

- `Type Description` heading: REQUIRED, describes the concept type.
- `Frontmatter Fields` heading: REQUIRED, describes frontmatter fields.
- `Markdown Content` heading: REQUIRED, describes the expected Markdown content.
- `Relationships` heading: OPTIONAL, describes link relationships.
- `Placement` heading: OPTIONAL, describes path semantics.
- `Naming` heading: OPTIONAL, describes file naming rules.
- `Examples` heading: OPTIONAL, provides examples of type usage.
- `Lifecycle` heading: OPTIONAL, describes the concept lifecycle.
- `Agent Skills` heading: OPTIONAL, describes agent skills for this　concept type.

## Examples

- A complete example is available in [this file](./concept_type_definition.md)

## Placement

- Valid and active `Concept Type Definition` concepts MUST be placed under `/.local/okf_types/`.
- A `Concept Type Definition` placed outside `/.local/okf_types/` is inactive and MUST NOT be used as an active type definition.

## Naming

- SHOUD snake cased defined type name.
