# Diagnostic and lexical presentation boundaries

`sql-fun-core::analysis_problem` owns structured semantic diagnostics. Its
payloads do not depend on SQLAST, miette, or a terminal. SQLAST constructs these
values through local conversions when reporting a problem through `ParseContext`.
The existing semantic AST, catalog, token consumers, and span lookup algorithms
remain responsible for semantic analysis.

## Diagnostic values

Diagnostic fields retain their domain types while carrying text rather than
owning semantic objects:

| SQLAST input | Core diagnostic representation |
| --- | --- |
| Qualified names, table/column/view/alias names | Distinct string-backed name wrappers |
| Type references and definitions | Distinct diagnostic type wrappers |
| Expressions and argument bindings | Diagnostic text wrappers, without a duplicate AST |
| Rule, trigger, and ownership identities | Typed identity text |
| Existing reliable source locations | Statement-relative diagnostic byte spans |

The conversion preserves retained identity text, including values originally
formed with Rust debug formatting. These values are diagnostic evidence; they
must not be parsed back into semantic facts to drive resolution or catalog
application. Issue #717 tracks the missing structured facts.

`AnalysisProblem::code` and `severity` are available independently of rendering.
`render_metadata` supplies the currently implemented message and labels without
invoking a renderer. An absent value denotes an unimplemented detailed renderer
tracked by issue #522. `DiagnosticMessage::diagnostic_message` is a separate,
exhaustive contract for explanatory text, so CLI presentation can report every
problem even when detailed metadata is absent.

## Source and transport

Schema loaders provide the statement text they actually analyze to
`SchemaFileContext`. Diagnostic collection retains `DiagnosticPresentation`
alongside each problem when statement source was supplied. It neither reloads
source nor reads a theme nor writes terminal output. The frozen catalog does not
retain this presentation context.

`DiagnosticPresentation` carries a core problem, the analyzed statement text,
its explicit dialect, and the statement's byte offset in its file. Diagnostic
label spans remain relative to that statement. Unknown positions remain absent;
zero is a valid byte offset.

The server's retained layer diagnostics and diagnostic collection payloads carry
this presentation data through the client. Existing summary fields retain their
meaning. A renderer uses the supplied source, so later file edits cannot change
the SQL under a retained diagnostic. The JSON Lines API remains a structured
transport and does not implicitly render reports.

## Lexing and rendering

Core `TokenStream` owns original SQL text and validated ordered UTF-8 byte
ranges, including whitespace and comments. Token categories are presentation
categories; raw spelling and the selected dialect preserve lexical details.
They are unrelated to Rust procedural-macro token streams.

`sql-fun-sqlast::tokenize_sql` adapts existing dialect lexers to this public
representation for highlighting. It does not replace semantic scans or
`TryFromWithTokens`. Future semantic adoption requires its own scope decision.

The server exposes `tokenize_sql`, adapting the existing SQLAST lexer to a
lossless wire token collection. The client fetches and releases the collection,
then validates its UTF-8 byte ranges against the caller's exact source.
CLI `DiagnosticRenderer` owns miette integration, theme selection/loading,
terminal detection, and styling. It uses those server-produced tokens for
retained statement presentations; the highlighter clips them to miette's
snippet and line ranges. `check` pairs each report with the extracted Rust
function/source identity and complete original SQL. A semantic-only problem
uses the same renderer without invented source labels.
