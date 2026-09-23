# Database engine compatibility 

## Context and Rationale

`sql-fun` requires a 100% engine-compatible SQL parser as a prerequisite for
supporting a database engine. Applications must be able to use the engine's SQL
syntax, including its extensions, rather than a subset chosen by a generic parser.

The parser choices are:

- **PostgreSQL:** [`pg_query`](https://docs.rs/crate/pg_query/6.2.0) uses the actual
  PostgreSQL server source and returns its parse tree. This preserves full syntax
  acceptance for the PostgreSQL version represented by the parser, including
  engine-specific tuning options.
- **SQLite:** [`turso_parser`](https://github.com/tursodatabase/turso/tree/v0.6.1/parser)
  is the parser used by Turso. We select it for SQLite under the same requirement
  of full engine compatibility; valid SQLite syntax is not out of scope merely
  because an application rarely uses it.

Parser compatibility and `sql-fun`'s own analysis are separate concerns. An
engine-compatible parser does not mean that every analysis or code generation
feature in `sql-fun` is already implemented. SQLite support is under development
and requires the `sqlite` feature.

This design choice is rooted in a simple but unavoidable reality:  
in production environments, performance tuning and operational control almost always require engine-specific syntax.  
Whether it’s index hints, optimizer directives, locking strategies like `SELECT FOR UPDATE`, or even subtle transaction semantics — such constructs are not optional; they are essential.

Without a parser that can interpret such syntax exactly as the engine does,  
no amount of abstraction can compensate — and any code generation tool becomes inherently incomplete.

To address this, `sql-fun` takes a static and fully analyzable approach to SQL.  
While it provides compile-time transformations to support developer productivity,  
**all SQL statements are finalized in the early stages of the build process** — there is no runtime SQL generation.  
As a result, every query can be enumerated before tuning begins, making tuning efforts measurable and predictable.  
Query omissions are impossible unless the user fails to include a required function.

The SQL extensions introduced by database engines — often viewed as deviations from the standard — are, in practice, necessary adaptations to real-world demands.


## Design Commitment and Practical Conclusions

For PostgreSQL, the parser is derived directly from the database engine itself,
so full syntax acceptance includes engine-specific tuning options by construction.
For SQLite, full engine compatibility is likewise the parser selection requirement.

Rather than resisting these extensions, `sql-fun` accepts them as an unavoidable part of working with production-grade systems.

These deviations are not signs of non-compliance; they are the price of practicality.  
No real-world database survives purely on theoretical purity — and tools meant to work at that level must reflect that reality.

Production-grade database engines do not merely tolerate the complexities of real-world usage — they embody them.

Over time, these engines have evolved in close symbiosis with real-world operational demands, incorporating engine-specific extensions, tuning mechanisms, and behavioral nuances that are not optional but essential.

This tight coupling between engine and workload is not an anomaly; it is the norm.  
In fact, it is precisely what makes most real-world databases viable.

`sql-fun` embraces this reality as a foundational design assumption.  
Supporting production-level SQL usage requires not just syntactic compatibility, but **deep alignment with how the engine itself parses and interprets SQL**.

That is why `sql-fun` depends on a 100% engine-compatible SQL parser.  
Without it, no tool can reliably accept or process the full range of constructs that make real-world databases operationally effective.
