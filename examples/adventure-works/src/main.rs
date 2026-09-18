//!  adventure-works schema load

#[sql_fun::from_table("sales.vindividualcustomer")]
#[derive(Debug, derive_builder::Builder)]
struct IndividualCustomer {}

fn main() {
    println!("Hello, world!");
}
