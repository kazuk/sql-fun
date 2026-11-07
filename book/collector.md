# sql-fun-collector

`sql-fun-collector` is an optional library for collecting and navigating ordered
rows. It can be used independently of generated query functions.

For `sql_query_many`, your application supplies the collector and row handler;
you can use your own collection logic without this library. See
[Function signatures](sql_functions/signatures.md#collecting-multiple-rows)
for that contract and [Collecting ordered rows](sql_fun_collector.md) for the
library's collection and navigation features. The guide also shows an
application-defined Arena and capability traits; these are patterns for your
application, not APIs provided by the library.
