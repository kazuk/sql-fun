# dict_xsyn

An extended-synonym text-search dictionary template and dictionary.

## Bundled versions

| PostgreSQL catalog | Extension version | Offline verification |
| --- | --- | --- |
| 17.6 | 1.0 | Initializes. Example resolves. |
| 18.1 | 1.0 | Initializes. Example resolves. |
| 19 snapshot | 1.0 | Initializes. Example resolves. |

## SQL example

```sql
SELECT ts_lexize('xsyn'::regdictionary, 'example');
```

## Analysis boundaries

Analysis resolves the function and argument types without invoking
the dictionary or
reading a PostgreSQL synonym file.

sql-fun loads SQL declarations offline. PostgreSQL installs the extension
and runs its code. sql-fun does not load the extension binary or verify
its ABI compatibility with the server.

[PostgreSQL manual](https://www.postgresql.org/docs/19/dict-xsyn.html)
