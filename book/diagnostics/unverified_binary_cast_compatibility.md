# unverified_binary_cast_compatibility

sql-fun registers the binary cast for analysis without verifying ABI or binary
representation compatibility. PostgreSQL checks physical compatibility when
executing the declaration. Successful analysis does not guarantee runtime
compatibility.
