# State Change Event Receiver

## Intent

A state-owning object receives externally determined state changes as events and
updates its own state and invariants.

External components do not directly modify the object's internal fields. Their
responsibility is limited to delivering the state change.

## Problem

In application code, an object's state is often determined by the result of an
external operation.

Examples:

- `INSERT ... RETURNING` determines an ID or `created_at` value.
- A payment service determines that a payment has been authorized, captured, or
  refunded.
- A shipping service changes the state of a shipment.
- A workflow result causes a domain object to transition to another state.

If an external component directly manages the target object's internal state,
knowledge about state transitions becomes scattered outside the object.

```rust
payment.status = PaymentStatus::Authorized;
payment.authorization_id = Some(authorization_id);
payment.authorized_at = Some(authorized_at);
```

If synchronization is then managed externally as well, the application tends to
need a change tracker that handles snapshots, dirty tracking, identity maps,
relationship fixup, generated values, and similar concerns.

## Forces

- The state-owning object has the best knowledge of its own invariants.
- Some values and transitions are determined by external systems.
- State-transition knowledge should not be scattered across adapters,
  repositories, ORMs, or application services.
- Shared mutable state races should be avoided.
- A state transition may need to propagate state to child objects.
- Invalid transition ordering should be represented in the type system when
  practical.

## Pattern

Implement state change event receivers on the object that owns the state.

External components pass the values representing the state change to the
receiver, and the receiver updates itself.

```rust
impl Payment {
    pub fn on_authorized(
        &mut self,
        authorization_id: AuthorizationId,
        authorized_at: OffsetDateTime,
    ) {
        self.status = PaymentStatus::Authorized;
        self.authorization_id = Some(authorization_id);
        self.authorized_at = Some(authorized_at);
    }
}
```

The caller does not manipulate internal state directly.

```rust
let authorized = payment_gateway.authorize(...).await?;

payment.on_authorized(
    authorized.authorization_id,
    authorized.authorized_at,
);
```

The basic structure is:

```text
external state change
        |
        v
state change event
        |
        v
state-owning object
        |
        v
receiver method
        |
        v
self state / invariant update
```

## Implementation in Rust

In Rust, using `&mut self` for the receiver gives exclusive mutable access while
the state change is being applied.

```rust
impl UserDto {
    pub fn on_insert(
        &mut self,
        id: UserId,
        created_at: OffsetDateTime,
        updated_at: OffsetDateTime,
    ) {
        self.id = Some(id);
        self.created_at = Some(created_at);
        self.updated_at = Some(updated_at);

        for address in &mut self.addresses {
            address.on_user_inserted(id);
        }
    }
}
```

State determined by the database can be received from `RETURNING` and passed
directly to the receiver.

```rust
let inserted = insert_user(client, &user).await?;
user.on_insert(inserted.id, inserted.created_at, inserted.updated_at);
```

Here, `insert_user` returns the values produced by `RETURNING`; every receiver
argument therefore has an explicit source.

Boilerplate receiver implementations may be generated with a derive macro.

```rust
#[derive(StateChangeEventReceiver)]
struct UserAddressDto {
    #[on_insert]
    id: Option<UserAddressId>,

    #[on_insert]
    created_at: Option<OffsetDateTime>,

    #[on_update]
    updated_at: Option<OffsetDateTime>,
}
```

The derive macro is only a convenience. The pattern itself does not require a
particular macro or trait.

## Composition with an Event Router

An Event Router is responsible for deciding which domain object receives a state
change. The domain object is responsible for applying the state transition and
maintaining its invariants.

```text
external event
    |
    v
event router
    |
    v
target domain object
    |
    +--> on_authorized(...)
    +--> on_shipped(...)
    +--> on_insert(...)
```

The router does not modify the domain object's internal state directly.

This lets the application layer focus on event delivery and orchestration while
domain objects manage their own domain state.

## Type-state Variant

When a transition should change which operations are available, the pattern can
be combined with the type-state pattern.

```rust
let user: User<PendingInsert> = ...;
let user_id = insert_user(client, &user).await?;
let user: User<Inserted> = user.on_insert(user_id);

insert_user_addresses(client, user.addresses()).await?;
```

If `User<PendingInsert>` does not expose `addresses()` as a persistence source,
and only `User<Inserted>` does, code that inserts child rows before the parent
insert cannot be written.

In this variant, the receiver method is not an in-place mutation through
`&mut self`; it is a transition constructor that returns a value in the next
state.

## Persistence Example

Even for DTOs with parent-child relationships, the persistence layer does not
need to understand the object graph.

```rust
let user_id = insert_user(client, &user_dto).await?;
user_dto.on_insert(user_id);

insert_user_address(
    client,
    user_dto.addresses(),
    UserAddressInsertCollector::new(...),
)
.await?;
```

The insert query only borrows `user_dto`, because the same DTO remains the state
owner that receives the resulting state change and is reused by the subsequent
child insert.

If `UserDto::on_insert` propagates `user_id` to its child `UserAddressDto`
values, the next query can obtain the required values through ordinary getters.

If the collection returned by `user_dto.addresses()` implements `ToSql` and
serializes itself as JSONB, the parent insert and the child bulk insert require
exactly two queries regardless of the number of child elements.

`UserAddressInsertCollector` does not construct the input JSONB. It can instead
deliver each `RETURNING` result to the corresponding `UserAddressDto` state
change event receiver.

```rust
address.on_insert(id, created_at, updated_at);
```

The responsibilities remain separate:

```text
schema / SQL        -> persistence semantics
ToSql               -> input encoding
collector           -> result delivery
state change event  -> state-transition fact
DTO / domain object -> state ownership and invariant maintenance
```

## Consequences

### Benefits

- State-transition knowledge is localized in the state owner.
- Application services, adapters, and repositories do not need to manipulate
  internal fields directly.
- The need for external change tracking is reduced.
- Database-generated values and externally determined state can be reflected
  naturally.
- State propagation to child objects can remain inside the domain object.
- In Rust, `&mut self` lets the type system support exclusive mutation.
- In combination with type-state, transition ordering can be constrained at
  compile time.

### Costs

- State-owning objects need an explicit receiver API.
- More event kinds imply more receiver methods.
- Atomicity across multiple events is not provided by this pattern itself.
- Duplicate or out-of-order events in distributed systems still require separate
  idempotency and ordering policies.

## Distinction from Observer

The Observer pattern focuses on delivering notifications. An observer does not
necessarily own the authoritative state being changed.

In State Change Event Receiver, the receiver is the state owner, and applying
the received event as its own state transition is the central responsibility.

The purpose is therefore not notification itself, but localization of state
ownership.

## Distinction from External Change Tracking

An external change tracker observes an object from the outside and manages
differences and synchronization state on its behalf.

State Change Event Receiver takes the opposite approach: the state change is
delivered to the state owner itself.

```text
external change tracking:
    tracker observes and manages object state

state change event receiver:
    object receives change and manages itself
```

If an external tracker is still required, it can be combined with receivers
while keeping object-specific state-transition knowledge inside the object.

## Applicability

Use this pattern when an external operation determines a state change that must
be reflected in an application object.

Typical examples include:

- database-generated state
- payment authorization, capture, and refund
- order lifecycle
- shipment lifecycle
- workflow state transition
- contract lifecycle
- inventory reservation
- asynchronous external service results

The pattern is not limited to DTOs. It applies to any state-owning object.

## Related Patterns

- Event Router
- State
- Observer
- Type-state
- Aggregate
- Event Sourcing

State Change Event Receiver does not require Event Sourcing. The event log does
not need to be the authoritative store; the pattern is also useful when an
externally determined state change is simply applied to the current state owner.
