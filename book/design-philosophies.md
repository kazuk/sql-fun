# Design Philosophies

`sql-fun` is built on the belief that tools should help — not control — the
developer.

- It assumes you know SQL and prefer to write it directly.
- It does not attempt to abstract over SQL or model your database schema.
- It runs entirely at compile time, with no runtime dependencies.
- It provides static safety through validation, not indirection.

This design avoids bringing dynamic behavior into your application.
There is no runtime — and therefore, no runtime logic to manage.
Every query is statically defined, statically parsed, and statically validated.

> You write the SQL.  
> `sql-fun` helps Rust understand it.  
> That’s it.

## No-lockin Philosophy

Among many possible design philosophies, `sql-fun` adopts one — but does not
enforce it.

While it embraces compile-time validation and zero-runtime constraints,  
it deliberately avoids locking users into this way of thinking.

You are free to:

- use the generated Rust code however you like,
- ignore parts of it,
- post-process the generated TOML files,
- or switch to other tools at any time.

There is no lock-in — not even ideological.

## Just a Tool Philosophy

`sql-fun` does not try to be smart. It tries to be yours.

It doesn’t act on its own. It doesn’t try to improve your SQL.  
It doesn’t offer advice or fill in gaps. Because that’s not what tools do.

> Like a hammer or a saw,  
> it will never start working without you.  
> It never makes assumptions, never overrides intent, and never moves on its
> own.

In a world where many systems try to automate, predict, or control,  
`sql-fun` insists on being a tool — and only a tool.

This is not a limitation.  
It’s a stance.

### Explicit-choice Philosophy

In `sql-fun`, nothing happens unless you choose it.

Even if there is only one valid option, it is still considered "not chosen"  
until you, the user, explicitly make that decision.

> If you didn’t choose it,  
> and you didn’t want it,  
> then it must not happen.  
> Choosing for you — *is not what sql-fun does.*

This is not because `sql-fun` wants to be opinionated.  
It’s because it wants to be **just a tool** —  
like a saw or a hammer, it should do nothing on its own.  
It does only what you ask, and nothing more.

A choice can also be expressed in configuration: naming a datasource `default`
selects it for commands that use the common datasource when no other datasource
name is supplied. An explicit CLI or environment selection takes precedence.

This principle governs core policies such as:

- Refusing to assume a default `connector` or `engine`, even if only one exists,
- Requiring schema files to be explicitly provided,
- Treating omissions as deliberate silence — not as implicit consent.

`sql-fun` will never make decisions on your behalf.  
Not in configuration, not in code generation, not in philosophy.

### Tools, Not Taxis

Some tools are like taxis — you don't own them, but they take you to a
destination.  
They decide the route, and sometimes even drive themselves.

That’s fine — for taxis.

But `sql-fun` is not a taxi.  
It’s your car. Your hammer. Your saw.

If your car started driving itself without you asking,  
you wouldn’t call it helpful — you’d call it dangerous.

Likewise, `sql-fun` will never take action you didn’t request.  
Not automatically, not silently, and not presumptuously.

It does nothing unless you hold the wheel.

### Why This Matters

`sql-fun` is often embedded into larger systems —  
build pipelines, test suites, code generation workflows, and developer tooling.

If such a tool started making its own decisions —  
even well-intentioned ones — it could introduce inconsistencies, confusion, or
worse.

Silent assumptions become invisible bugs.  
Helpful guesses become unintentional lock-in.  
And implicit behavior becomes technical debt.

That’s why `sql-fun` chooses not to choose for you.  
It behaves as a tool — and only as a tool.

> It will never act on your behalf.  
> It will never guess what you meant.  
> It will never try to be smarter than you.

It simply does what you asked, and nothing more.  
And in complex systems — that's not a limitation.  
That’s a virtue.
