# Virtual Foreign Key Constraint

*This is a planning document outlining a potential future feature of
`sql-fun`.  
It is not yet implemented, and no timeline or commitment has been established.*

## Problem

Applications often need to enforce foreign key relationships *without* relying
on database-level constraints.

Consider log records that reference a user. If you use real foreign key
constraints, deleting a user becomes impossible while related logs exist.

This leads to introducing a `deleted` flag instead of physical deletion.

However, `deleted` flags can interfere with unique constraints. For example:

- A user accidentally marks themselves as deleted.
- They try to sign up again using the same email.
- The unique constraint fails: "Email already in use."
- So you drop the unique constraint.
- Now the application must check for existing records before inserting — adding
  a query and business logic.
- Eventually, the database becomes a loose, constraint-free storage layer, with
  all constraints moved into the application code.

You could use `(email, deleted_at)` as a unique constraint to resolve this more
cleanly.

But the pattern remains: real foreign keys → `deleted` flags → dropped unique
constraints.

## Solution

A **virtual foreign key** behaves like:

```text
log.user_id references users(id)
```

But enforced entirely at the application layer.

Given an SQL like:

```sql
INSERT INTO log (user_id,event_description) values ($1, $2)
```

`sql-fun` could rewrite this to:

```sql
INSERT INTO log (user_id, event_description) 
SELECT $1, $2 FROM users WHERE id=$1
```

Then check that exactly one row was inserted.

This ensures the referenced user exists at insert time,
without preventing deletion of that user later.
No deleted flag needed. No broken constraints.

> 🔧 This feature is not yet implemented.  
> We're currently exploring feasibility and potential use cases.  
> Community interest or sponsorship may influence its development priority.
