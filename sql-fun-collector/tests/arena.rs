//! Demonstrates accessing related rows in an arena using sorted collections.
#![allow(dead_code)]

use sql_fun_collector::{ExtractKey, ExtractKeyWithContext, Sorted, SortedSlice, SortedSliceView};

struct OrderArena {
    users: SortedSlice<UserRow, UserId>,
    user_orders: SortedSlice<UserOrderRow, (UserId, OrderId)>,
    order_lines: SortedSlice<OrderLineRow, (UserId, OrderId, OrderLineId)>,

    // select distinct products.*
    // from users
    //   inner join user_orders
    //   inner join order_lines
    //   inner join products
    // order by products.id
    products: SortedSlice<ProductRow, ProductId>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct UserId(i64);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct OrderId(i64);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct OrderLineId(i64);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct ProductId(i64);

struct UserRow {
    id: UserId,
}

impl ExtractKey<UserId> for UserRow {
    fn get_key(&self) -> UserId {
        self.id
    }
}

struct UserOrderRow {
    id: OrderId,
    user_id: UserId,
}

struct ProductRow {
    id: ProductId,
}

impl ExtractKey<(UserId, OrderId)> for UserOrderRow {
    fn get_key(&self) -> (UserId, OrderId) {
        (self.user_id, self.id)
    }
}

impl ExtractKey<OrderId> for UserOrderRow {
    fn get_key(&self) -> OrderId {
        self.id
    }
}

struct OrderLineRow {
    id: OrderLineId,
    order_id: OrderId,
}

impl OrderArena {
    pub fn get_user_by_order(&self, order_id: OrderId) -> Option<&UserRow> {
        let user_id = self.user_orders.resolve_prefix_key(order_id)?;
        self.users.get_by(user_id)
    }
}

trait HasUserRows {
    fn users(&self) -> &SortedSlice<UserRow, UserId>;

    fn get_user(&self, user_id: UserId) -> Option<&UserRow> {
        self.users().get_by(user_id)
    }
}

impl HasUserRows for OrderArena {
    fn users(&self) -> &SortedSlice<UserRow, UserId> {
        &self.users
    }
}

trait HasUserOrderRows {
    fn user_orders(&self) -> &SortedSlice<UserOrderRow, (UserId, OrderId)>;

    fn get_order(&self, order_id: OrderId) -> Option<&UserOrderRow> {
        self.user_orders().iter().find(|item| item.id == order_id)
    }

    fn get_user_by_order(&self, order_id: OrderId) -> Option<&UserRow>
    where
        Self: HasUserRows,
    {
        let user_id = self.user_orders().resolve_prefix_key(order_id)?;
        self.users().get_by(user_id)
    }
}

impl HasUserOrderRows for OrderArena {
    fn user_orders(&self) -> &SortedSlice<UserOrderRow, (UserId, OrderId)> {
        &self.user_orders
    }
}

trait HasOrderLines {
    fn order_lines(&self) -> &SortedSlice<OrderLineRow, (UserId, OrderId, OrderLineId)>;
}

impl HasOrderLines for OrderArena {
    fn order_lines(&self) -> &SortedSlice<OrderLineRow, (UserId, OrderId, OrderLineId)> {
        &self.order_lines
    }
}

impl UserRow {
    pub fn get<A>(arena: &A, user_id: UserId) -> Option<&Self>
    where
        A: HasUserRows,
    {
        arena.get_user(user_id)
    }

    pub fn orders<'a, A>(&self, arena: &'a A) -> SortedSliceView<'a, UserOrderRow, OrderId>
    where
        A: HasUserOrderRows,
    {
        arena.user_orders().head(self.id)
    }
}

impl ExtractKey<(OrderId, OrderLineId)> for OrderLineRow {
    fn get_key(&self) -> (OrderId, OrderLineId) {
        (self.order_id, self.id)
    }
}

impl<A> ExtractKeyWithContext<A, (UserId, OrderId, OrderLineId)> for OrderLineRow
where
    A: HasUserOrderRows,
{
    fn get_key_with_context(&self, ctx: &A) -> (UserId, OrderId, OrderLineId) {
        let record_key = self.get_key();
        let order = ctx.get_order(record_key.0).unwrap();
        (order.user_id, record_key.0, record_key.1)
    }
}

impl UserOrderRow {
    pub fn get<A>(arena: &A, order_id: OrderId) -> Option<&Self>
    where
        A: HasUserOrderRows,
    {
        arena.get_order(order_id)
    }

    pub fn user<'a, A>(&self, arena: &'a A) -> Option<&'a UserRow>
    where
        A: HasUserRows,
    {
        arena.get_user(self.user_id)
    }

    pub fn order_lines<'a, A>(&self, arena: &'a A) -> SortedSliceView<'a, OrderLineRow, OrderLineId>
    where
        A: HasOrderLines + HasUserOrderRows,
    {
        arena
            .order_lines()
            .head_with_context(arena, self.user_id)
            .head(self.id)
    }
}
