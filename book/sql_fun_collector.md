# Collecting ordered rows with sql-fun-collector

## Purpose and when to use it

`sql-fun` follows a **zero-runtime** design. The generated query functions do
not require you to adopt a runtime framework or a particular collection type.
For `sql_query_many`, you choose the collector and the handler that receives
each row.

`sql-fun-collector` is an **optional library** for implementing those
collectors. Use it to collect ordered SQL results, finish collection as
immutable row sets, and navigate related rows through borrowed views. You can
also use your own containers and collection logic without adding this
dependency. The library adds ordinary collection and lookup operations when you
choose to use it; it does not change the zero-runtime foundation of `sql-fun`.
It can also be used independently of generated query functions.

## From a result set to an immutable row set

Database result sets are normally ordered for this purpose with `ORDER BY` in
the `SELECT` statement. `SortedVec` accepts those rows sequentially from your
collector, with a lightweight check against the previous key. It does not sort
the rows itself: the database has already performed that work.

```text
SELECT ... ORDER BY ...
        |
        v
collector receives each row
        |
        v
SortedVec: append rows and check their order
        |
        | into_sorted_slice()
        v
SortedSlice: immutable row set
        |
        | select related rows
        v
SortedSliceView: borrowed view of part of the row set
```

`into_sorted_slice` consumes the `SortedVec` and transfers its rows into a
`SortedSlice`. The conversion uses `Vec::into_boxed_slice`, which discards the
vector's excess capacity. Spare capacity reserved while accumulating rows is
therefore released rather than retained in the finished row set.

The resulting row set is immutable: its collection API exposes no operations to
append, reorder, or mutate rows. Views borrow the stored rows, so selecting a
view does not copy them.

As with other shared Rust references, row types with interior mutability still
control their own internal state. Ordering keys must remain stable while you
use the collection's searches and views.

## A complete example without a database

Suppose orders are selected with this query:

```sql
SELECT user_id, id AS order_id
FROM user_orders
ORDER BY user_id ASC, id ASC;
```

The ordering key is `(user_id, order_id)`. The following runnable example uses
literal rows in that same order so you can try the collection operations without
a database connection.

```rust
use sql_fun_collector::{ExtractKey, OrderError, Sorted, SortedVec};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct UserId(i64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct OrderId(i64);

struct OrderRow {
    user_id: UserId,
    order_id: OrderId,
}

impl ExtractKey<(UserId, OrderId)> for OrderRow {
    fn get_key(&self) -> (UserId, OrderId) {
        (self.user_id, self.order_id)
    }
}

// After selecting a user, the remaining ordering key is the order ID.
impl ExtractKey<OrderId> for OrderRow {
    fn get_key(&self) -> OrderId {
        self.order_id
    }
}

fn main() -> Result<(), OrderError<(UserId, OrderId)>> {
    let mut collecting = SortedVec::default();

    for (user_id, order_id) in [(1, 10), (1, 11), (2, 20)] {
        collecting.push(OrderRow {
            user_id: UserId(user_id),
            order_id: OrderId(order_id),
        })?;
    }

    let orders = collecting.into_sorted_slice();

    // Borrow the orders belonging to user 1.
    let user_orders = orders.head(UserId(1));
    for order in user_orders.iter() {
        println!("Order {:?} belongs to user {:?}", order.order_id, order.user_id);
    }

    Ok(())
}
```

Import the `Sorted` trait to use access and lookup methods. `ExtractKey` tells
the collection how to obtain a key from a row. A row can implement it for more
than one key type, as above.

Prefer newtypes for IDs so each key identifies its relationship in the type
system. With `i64` aliases, `(UserId, OrderId)` and `(OrderId, OrderLineId)`
both become `(i64, i64)`: an `ExtractKey<(i64, i64)>` bound cannot distinguish
them, and a row cannot implement the trait separately for those two meanings.
Distinct newtypes allow both implementations and make navigation requirements
explicit. Deriving `Ord` preserves the underlying integer order; `Copy` makes
these small IDs convenient to return by value.

In a `sql_query_many` handler, the append step is the same: receive the
collector and a row, call `push(row)?` on its `SortedVec`, and return the
collector. Once the query completes, convert the accumulated rows with
`into_sorted_slice`. Your error type must be able to propagate `OrderError`
along with query and row-construction errors. See
[SQL Functions](./sql_functions/signatures.md#collector-type) for the collector and handler
contract.

## Keep SQL ordering and Rust keys aligned

For the example above, SQL orders by `user_id` first and `order_id` second.
Rust compares the tuple key in that same order. Each new key must be greater
than or equal to the previous one.

If a new key is smaller, `push` returns `OrderError::InvalidOrder` and leaves
the accumulated rows unchanged. Equal keys are accepted: this container does
not enforce uniqueness or remove duplicates. A lookup with `get_by` may return
any matching row when keys repeat.

Use an explicit `ORDER BY` on the query that returns the rows. Match its column
order and direction to the Rust key. For text or nullable columns, also account
for database collation and null ordering rather than assuming they match Rust's
`Ord`. The integer keys in this example keep that agreement straightforward.

`push_with_key` lets the collector supply a key explicitly, for example when it
has already resolved information from another row set. The supplied key must
agree with the keys used for later lookups. It checks append order but does not
store a separate lookup index for every row.

### Descending columns with `Reverse`

For `ORDER BY col DESC`, wrap the corresponding Rust key component in
[`std::cmp::Reverse`](https://doc.rust-lang.org/std/cmp/struct.Reverse.html).
It reverses that component's comparison order, so descending database values
form a nondecreasing sequence of Rust keys accepted by `SortedVec`.

For example, keep users ascending and order IDs descending:

```sql
SELECT user_id, id AS order_id
FROM user_orders
ORDER BY user_id ASC, id DESC;
```

Using the `OrderRow` type from the earlier example, provide these key
implementations for the descending ordering:

```rust
use std::cmp::Reverse;

impl ExtractKey<(UserId, Reverse<OrderId>)> for OrderRow {
    fn get_key(&self) -> (UserId, Reverse<OrderId>) {
        (self.user_id, Reverse(self.order_id))
    }
}

impl ExtractKey<Reverse<OrderId>> for OrderRow {
    fn get_key(&self) -> Reverse<OrderId> {
        Reverse(self.order_id)
    }
}

fn descending_orders() -> Result<(), OrderError<(UserId, Reverse<OrderId>)>> {
    let mut collecting = SortedVec::<OrderRow, (UserId, Reverse<OrderId>)>::default();
    for (user_id, order_id) in [(1, 11), (1, 10), (2, 20)] {
        collecting.push(OrderRow {
            user_id: UserId(user_id),
            order_id: OrderId(order_id),
        })?;
    }

    let orders = collecting.into_sorted_slice();
    let user_orders = orders.head(UserId(1));
    if let Some(order) = user_orders.get_by(Reverse(OrderId(11))) {
        println!("Order {:?} belongs to user {:?}", order.order_id, order.user_id);
    }
    Ok(())
}
```

Wrap only the components corresponding to descending columns. The key here is
`(UserId, Reverse<OrderId>)`, so the user's ordering stays ascending. Lookups
must use the same key types: `orders.get_by((UserId(1), Reverse(OrderId(11))))`
searches the full key, while `user_orders.get_by(Reverse(OrderId(11)))` searches
the tail after selecting a user. The stored row values remain unchanged, and no
additional sorting takes place.

## Composite keys and related rows

Composite keys express the hierarchy used to select related rows. For an order
line keyed by `(UserId, OrderId, OrderLineId)`, `SortOrder` defines:

| Component | Value | Meaning |
| --- | --- | --- |
| Head | `UserId` | First component |
| Tail | `(OrderId, OrderLineId)` | Everything after the first component |
| Parent | `(UserId, OrderId)` | Everything before the last component |
| Least | `OrderLineId` | Last component |

Tuple implementations are provided for two through ten components. For a pair,
the parent and tail are individual values rather than one-element tuples.

`head(user_id)` selects that user's rows and returns a view whose key type is
the tail. For order lines, that view is ordered by `(OrderId, OrderLineId)`.
Calling `head(order_id)` on that view selects one order's lines, leaving
`OrderLineId` as the key. Each step borrows the same underlying rows. The row
must support key extraction for the ordering used at each step.

`parent_range` and `iter_range` select rows by the complete parent key.
For order lines, pass `(user_id, order_id)` and provide an `ExtractKey`
implementation for that parent key. `group_by_parent` instead visits every
consecutive parent group and preserves the full key type in each view.

An empty selection produces an empty range or view. An unsuccessful individual
lookup produces `None`.

### Selecting a head-key interval

Use `head_between(start..end)` to select an interval of head keys. For example,
given an `events` collection ordered by `(Timestamp, EventId)` and bounds of
type `Timestamp`, a time-series reader can borrow the events in a period:

```rust
let period_events = events.head_between(start..end);
for event in period_events.iter() {
    // Render or process the borrowed event.
}
```

The interval includes `start` and excludes `end`. All rows with matching
timestamps are included, even when several events share a timestamp. Empty,
reversed, or unmatched intervals return an empty view.

Unlike `head`, `head_between` retains the full key type: the selected events
remain ordered by `(Timestamp, EventId)`. It finds the boundaries using
partition searches and borrows the rows between them without copying them.

## Building a read model with an Arena

An **Arena** is an application-defined read model for a particular operation.
It bundles the result sets that operation needs and provides navigation between
their rows. For example, an order-summary screen might need users, their
orders, and the lines in those orders. The library does not prescribe an Arena
type or require one model for the entire application.

An Arena keeps query-result row sets in ordered storage and provides
navigation through borrowed rows and views, without constructing a separate
object graph.

Prefer small Arenas tailored to an operation or feature. Reusing a broader
Arena can introduce queries and row decoding that the operation does not need,
and retain unused result sets for its entire lifetime. Share an Arena where
the required data fits; otherwise, reuse
[capability traits and navigation logic](#composing-read-models-with-capability-traits)
in a smaller read model, as described below.

### Relationship to DDD aggregates

In domain-driven design (DDD), an aggregate groups domain entities within a
consistency boundary. An Arena holds query-result row sets rather than domain
entities. Its boundary is the data needed by a particular read operation; it
does not define transactional consistency or enforce domain invariants.

The two concepts share a useful capability: navigation between related data.
An aggregate can provide navigation between its entities, while an Arena can
provide navigation between rows in its result sets. This similarity makes
aggregate-style navigation useful when designing an Arena's read API, but does
not make the Arena a DDD aggregate. An Arena may combine query results drawn
from several domain aggregates when the read operation needs them.

### Shared lifetime and navigation

An Arena owns multiple `SortedSlice` values. Your collector first accumulates
each query's rows in a separate `SortedVec`, then converts the containers into
`SortedSlice` values and places them in the Arena when collection is complete.
Keeping the result sets under one owner gives them a shared lifetime: the
Arena retains them together, and references or views returned by its methods
borrow from that owner. Dropping the Arena releases the result sets together.

The following examples extend the earlier `OrderRow`, `UserId`, and `OrderId`
definitions with users and order lines. `OrderLineId` is another distinct
newtype. All three row types represent query results.

```rust
use sql_fun_collector::{SortedSlice, SortedSliceView};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct OrderLineId(i64);

struct UserRow {
    user_id: UserId,
}

struct OrderLineRow {
    user_id: UserId,
    order_id: OrderId,
    order_line_id: OrderLineId,
}

impl ExtractKey<UserId> for UserRow {
    fn get_key(&self) -> UserId {
        self.user_id
    }
}

// The parent key identifies the order whose lines we want to enumerate.
impl ExtractKey<(UserId, OrderId)> for OrderLineRow {
    fn get_key(&self) -> (UserId, OrderId) {
        (self.user_id, self.order_id)
    }
}

struct OrderSummaryArena {
    users: SortedSlice<UserRow, UserId>,
    orders: SortedSlice<OrderRow, (UserId, OrderId)>,
    order_lines: SortedSlice<OrderLineRow, (UserId, OrderId, OrderLineId)>,
}
```

Each field owns one query's collected rows, ordered by its declared key:
users by `UserId`, orders by `(UserId, OrderId)`, and lines by
`(UserId, OrderId, OrderLineId)`. The fields stay private; navigation methods
return borrowed rows or views. Once collection is complete, these methods
work within existing storage without additional queries, allocations for
selected rows, or row copies.

See the
[aggregate fetch example](./examples/aggregate-fetch.md)
for collecting related data from multiple queries.

### Composing read models with capability traits

Define a **capability trait** for access to each result set an Arena can hold.
A capability describes what data a caller can access, without tying that
caller to a particular Arena struct. An Arena implements the traits matching
its fields, and a read operation requests the combination it needs through
trait bounds.

For users and orders, define accessors with reusable navigation methods:

```rust
trait HasUsers {
    fn users(&self) -> &SortedSlice<UserRow, UserId>;

    fn user(&self, id: UserId) -> Option<&UserRow> {
        self.users().get_by(id)
    }
}

trait HasOrders {
    fn orders(&self) -> &SortedSlice<OrderRow, (UserId, OrderId)>;

    fn orders_for_user(
        &self, id: UserId,
    ) -> SortedSliceView<'_, OrderRow, OrderId> {
        self.orders().head(id)
    }
}

impl HasUsers for OrderSummaryArena {
    fn users(&self) -> &SortedSlice<UserRow, UserId> {
        &self.users
    }
}

impl HasOrders for OrderSummaryArena {
    fn orders(&self) -> &SortedSlice<OrderRow, (UserId, OrderId)> {
        &self.orders
    }
}
```

The accessors connect each capability to the Arena's storage, while default
methods share navigation logic. A function requiring only orders accepts
`A: HasOrders`; one requiring users and orders accepts
`A: HasUsers + HasOrders`. Neither depends on `OrderSummaryArena` or requires
its order-line data.

Another Arena can implement the same traits for a different combination of
result sets and reuse those functions. Building a read model for an individual
operation therefore requires only its owning struct and the appropriate
accessors: capability traits supply the shared access and navigation logic.

### Navigating from one row to another

Capability traits also determine which navigation methods a read model can
offer from its rows. A user can enumerate orders when the supplied Arena
implements `HasOrders`; an order can enumerate its lines when that Arena
implements `HasOrderLines`. The row methods express these requirements as
trait bounds, so the compiler checks that the Arena provides the required
result sets.

Add the order-line capability, then expose navigation on the row types:

```rust
trait HasOrderLines {
    fn order_lines(&self)
        -> &SortedSlice<OrderLineRow, (UserId, OrderId, OrderLineId)>;
}

impl HasOrderLines for OrderSummaryArena {
    fn order_lines(&self)
        -> &SortedSlice<OrderLineRow, (UserId, OrderId, OrderLineId)>
    {
        &self.order_lines
    }
}

impl UserRow {
    fn orders<'a, A>(&self, arena: &'a A) -> SortedSliceView<'a, OrderRow, OrderId>
    where
        A: HasOrders,
    {
        arena.orders_for_user(self.user_id)
    }
}

impl OrderRow {
    fn lines<'a, A>(&self, arena: &'a A) -> impl Iterator<Item = &'a OrderLineRow>
    where
        A: HasOrderLines,
    {
        arena
            .order_lines()
            .iter_range((self.user_id, self.order_id))
    }
}

fn show_user_orders<A>(user: &UserRow, arena: &A)
where
    A: HasOrders + HasOrderLines,
{
    for order in user.orders(arena).iter() {
        println!("Order {:?}", order.order_id);
        for line in order.lines(arena) {
            println!("  Line {:?}", line.order_line_id);
        }
    }
}
```

The caller follows the relationships directly: user to orders, then order to
lines. The row methods use the Arena to resolve those relationships, and the
returned references borrow from the Arena rather than from the starting row.
The order-line lookup uses both user and order IDs to select only that order's
lines.

An Arena containing only users and orders can support `user.orders(&arena)`.
An Arena that also implements `HasOrderLines` can support the complete traversal
shown above. Reusing the row types and capability traits gives each read model
the navigation supported by its available result sets.

### Using navigation in the application

Navigating from a user to their orders and from an order to its lines lets
application code follow the relationships it needs directly. When building
domain objects, this API makes it straightforward to gather related rows and
pass their data to domain constructors. Domain rules and invariants remain
the responsibility of those domain objects; the Arena supplies the read data
and navigation used to construct them.

For a sufficiently simple application without a separate domain layer, the
same navigation can be used directly to render a response. For example, a
renderer can visit a user's orders and their lines to produce an HTML page or
an API response. There is no requirement to introduce an intermediate domain
model just to use the Arena. Choose the approach that fits the application's
complexity and responsibilities.

In either approach, navigating the collected rows uses existing storage and
does not issue additional queries. Constructing domain objects or rendering
the response may allocate as required by the application's output types.

## Resolving keys using other row sets

Sometimes a row stores its immediate parent's ID but not the full ordering
prefix. An order line might contain `OrderId` and `OrderLineId`, while its
`UserId` must be obtained from the orders collection.

`ExtractKeyWithContext<Ctx, O>` supports this case. Its
`get_key_with_context` method receives your context, such as an arena, and
returns the full key. `head_with_context` and `head_range_with_context` use
that key to select rows. The reconstructed keys must agree with the original
SQL result order.

Context-based extraction returns a key directly, not a `Result` or `Option`.
When reconstructing a key requires a parent row, that row must already be
loaded into the Arena. Its absence is a violation of the Arena's construction
invariants, rather than an ordinary lookup miss: the read model lacks data
required for its navigation to work. Validate these dependencies when building
the Arena and handle failures according to the application's error policy.
Keep the referenced keys stable afterward. The library does not load missing
relations or validate referential integrity for you.

For the opposite direction, `resolve_prefix_key` scans rows for a matching tail
and returns the head of the first match. Its main use is reverse lookup when
the tail uniquely identifies a row, such as an `OrderId` that is the primary
key of the orders table. Given that ID, the orders collection can locate its
`UserId`.

This is a linear search; it does not build a reverse index or check uniqueness.
If order IDs are unique only within each user, the same tail can occur under
multiple heads, and the first match in stored order wins. In that case, an
order ID alone cannot identify the intended user; use the full key or an
application-defined lookup with the required disambiguating information.

## Ownership and lookup costs

`SortedVec`, `SortedSlice`, and `SortedSliceView` preserve the ordering used
for lookup. With suitable keys and inexpensive key extraction, their ordered
searches are intended to provide adequate navigation performance for typical
application needs without additional indexes.

| Operation | How it uses the stored rows |
| --- | --- |
| `push` | Compares the new key with the previous key before appending |
| `get_by` | Uses binary search for a matching key |
| Parent and head range selection | Uses partition searches to find the range boundaries |
| `group_by_parent` | Visits consecutive rows to identify groups |
| `resolve_prefix_key` | Scans for the first matching tail key |

Choose key order to fit the relationships you navigate. Context-based key
extraction can perform additional lookups, which contribute to search cost.
Evaluate performance with the operations and data sizes your application uses.

If the application's requirements are more demanding, you can design and
implement additional index structures in application code. For example, an
Arena can own a map from an order ID to its position in the orders row set,
avoiding repeated linear reverse lookups. Build the index for the finalized
row set and keep it alongside that set in the Arena. Its navigation methods
can use the index internally while continuing to return borrowed rows.

The choice of index, its construction cost, and its memory use belong to the
application. The optional library's collection types do not prevent an Arena
from combining ordered row sets with indexes tailored to its read operations.
