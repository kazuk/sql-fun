# xml2

Functions for XPath queries and XSLT transformations.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.1 | Initializes. Example resolves. |
| 18.1 | 1.2 | Initializes. Example resolves. |
| 19 snapshot | 1.2 | Initializes. Example resolves. |

## SQL example

```sql
SELECT xpath_string('<x>value</x>', '/x');
```

## Analysis boundaries

Analysis resolves the XML function signature. PostgreSQL parses the input,
interprets the XPath, and executes transformations.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/xml2.html)
