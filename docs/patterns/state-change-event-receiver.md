# State Change Event Receiver

## Intent

状態を所有するオブジェクト自身が、外部で確定した状態変化を event として受け取り、自身の状態と invariant を更新する。

外部コンポーネントはオブジェクト内部のフィールドを直接更新せず、状態変化を通知することだけを責務とする。

## Problem

アプリケーションでは、オブジェクトの状態が外部処理の結果によって確定することがある。

例:

- DB の `INSERT ... RETURNING` によって ID や `created_at` が確定する。
- 決済サービスによって payment が authorized / captured / refunded になる。
- 配送サービスによって shipment の状態が変わる。
- workflow の実行結果によって domain object の状態が遷移する。

このとき外部コンポーネントが対象オブジェクトの内部状態を直接管理すると、状態遷移の知識がオブジェクトの外へ分散する。

```rust
payment.status = PaymentStatus::Authorized;
payment.authorization_id = Some(authorization_id);
payment.authorized_at = Some(authorized_at);
```

さらに状態同期を外部で管理しようとすると、snapshot、dirty tracking、identity map、relationship fixup、generated value の反映などを扱う change tracker が必要になりやすい。

## Forces

- 状態を所有するオブジェクト自身が、その invariant を最もよく知っている。
- 外部システムで確定する値や状態遷移は存在する。
- 状態遷移の知識を adapter、repository、ORM、application service へ分散させたくない。
- shared mutable state の競合を避けたい。
- 状態遷移後に子オブジェクトなどへ状態を伝播する必要がある場合がある。
- 不正な遷移順序を可能であれば型で表現したい。

## Pattern

状態を所有するオブジェクトに state change event receiver を実装する。

外部コンポーネントは state change を表す値を receiver に渡し、receiver が自身の状態を更新する。

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

呼び出し側は内部状態を操作しない。

```rust
let authorized = payment_gateway.authorize(...).await?;

payment.on_authorized(
    authorized.authorization_id,
    authorized.authorized_at,
);
```

基本構造は次の通り。

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

## Rust での実装

Rust では receiver を `&mut self` にすることで、状態変化適用中の mutable access を排他的にできる。

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

DB で確定した state を `RETURNING` から受け取り、そのまま receiver に渡せる。

```rust
let user_id = insert_user(client, &user).await?;
user.on_insert(user_id, created_at, updated_at);
```

receiver の定型実装は derive macro で生成してもよい。

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

derive macro は convenience であり、パターン自体が特定の macro や trait を要求するわけではない。

## Event Router との組み合わせ

Event Router は「どの state change をどの domain object に届けるか」を担当し、domain object は受け取った event に対する state transition と invariant の維持を担当する。

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

router は domain object の内部状態を直接変更しない。

これにより application layer は event delivery と orchestration に集中し、domain object は自身の domain state を自己管理できる。

## Type-state Variant

状態遷移によって利用可能な操作そのものを変えたい場合は type-state pattern と組み合わせられる。

```rust
let user: User<PendingInsert> = ...;
let user_id = insert_user(client, &user).await?;
let user: User<Inserted> = user.on_insert(user_id);

insert_user_addresses(client, user.addresses()).await?;
```

`User<PendingInsert>` では `addresses()` を persistence 用 source として提供せず、`User<Inserted>` でのみ提供すれば、親 INSERT 前に子 INSERT を記述できないようにできる。

この variant では receiver method は `&mut self` による mutation ではなく、新しい state type を返す transition constructor になる。

## Persistence Example

親子関係を持つ DTO でも、persistence layer が object graph を理解する必要はない。

```rust
let user_id = insert_user(client, user_dto).await?;
user_dto.on_insert(user_id);

insert_user_address(
    client,
    user_dto.addresses(),
    UserAddressInsertCollector::new(...),
)
.await?;
```

`UserDto::on_insert` が `user_id` を子の `UserAddressDto` へ伝播するなら、次の query は通常の getter を通じて必要な値を取得できる。

`user_dto.addresses()` の collection object が `ToSql` を実装して JSONB に serialize する場合、子要素数に依存せず、親 INSERT と子 bulk INSERT の 2 query で処理できる。

`UserAddressInsertCollector` は入力 JSONB の生成を担当しない。`RETURNING` の結果を対応する `UserAddressDto` の state change event receiver に渡す役割を持てる。

```rust
address.on_insert(id, created_at, updated_at);
```

このとき責務は次のように分離される。

```text
schema / SQL       -> persistence semantics
ToSql              -> input encoding
collector          -> result delivery
state change event -> state transition fact
DTO / domain object-> state ownership and invariant maintenance
```

## Consequences

### Benefits

- 状態遷移の知識を状態所有者へ局所化できる。
- application service、adapter、repository が内部フィールドを直接管理しなくてよい。
- external change tracker の必要性を減らせる。
- DB-generated values や外部サービスで確定した state を自然に反映できる。
- 子オブジェクトへの state propagation を domain object 内に閉じ込められる。
- Rust では `&mut self` によって mutation の排他性を型システムで支援できる。
- type-state と組み合わせることで遷移順序を compile time に制約できる。

### Costs

- state-owning object が receiver API を設計する必要がある。
- event が増えると receiver method も増える。
- 複数 event を跨ぐ atomicity は、このパターン単体では保証しない。
- distributed system における duplicate / out-of-order event は、別途 idempotency や ordering policy が必要になる。

## Distinction from Observer

Observer pattern の中心は変更通知の配送であり、observer が必ず自身の authoritative state を管理するとは限らない。

State Change Event Receiver では、receiver が state owner であり、受け取った event を自身の状態遷移として適用することが中心となる。

そのため目的は notification ではなく state ownership の局所化である。

## Distinction from External Change Tracking

external change tracker は対象オブジェクトの外部から状態を観測し、差分や同期状態を管理する。

State Change Event Receiver は反対に、state owner 自身へ state change を渡す。

```text
external change tracking:
    tracker observes and manages object state

state change event receiver:
    object receives change and manages itself
```

外部 tracker が必要な場合でも、receiver と併用し、tracker が object 内部の state transition knowledge を持たないようにできる。

## Applicability

このパターンは、外部処理によって状態変化が確定し、その状態をアプリケーションオブジェクトへ反映する必要がある場合に適用できる。

代表例:

- database generated state
- payment authorization / capture / refund
- order lifecycle
- shipment lifecycle
- workflow state transition
- contract lifecycle
- inventory reservation
- asynchronous external service result

DTO に限定されるパターンではない。state-owning object 全般が対象となる。

## Related Patterns

- Event Router
- State
- Observer
- Type-state
- Aggregate
- Event Sourcing

State Change Event Receiver は Event Sourcing を要求しない。event log を authoritative store とせず、外部で確定した state change を現在の state owner に適用するだけでも利用できる。
