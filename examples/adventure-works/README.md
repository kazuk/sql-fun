# 🧪 `sql-fun` Example: AdventureWorks Sample Database

This example demonstrates how to use
[`sql-fun`](https://github.com/your-org/sql-fun) with the classic
[AdventureWorks](https://github.com/lorint/AdventureWorks-for-Postgres) database
for PostgreSQL.

> ✅ Realistic schema. ✅ Real-world SQL. ✅ Fully compile-time checked.

---

## 📦 Setting Up the AdventureWorks Database

### 1. Clone and Launch the Database

<!-- Preserve the original terminal transcript and output. -->
<!-- rumdl-disable MD013 -->
```bash
git clone https://github.com/lorint/AdventureWorks-for-Postgres.git
cd AdventureWorks-for-Postgres

wget https://github.com/Microsoft/sql-server-samples/releases/download/adventureworks/AdventureWorks-oltp-install-script.zip \
  -O adventure_works_2014_OLTP_script.zip

docker compose up -d
```

<!-- rumdl-enable MD013 -->
### 2. Configure Environment Variables

- Setting up for database connection environment variables.

```bash
cat <<EOF > .env
export PGHOST=localhost
export PGUSER=postgres
export PGPORT=5432
export PGDATABASE=Adventureworks
EOF

echo "localhost:5432:Adventureworks:postgres:postgres" >> ~/.pgpass
chmod 600 ~/.pgpass
```

### 3. Testing connection

```bash
(
   source .env
   psql -c "\\dt *.*" | head -n 10
)

```

Example output:

<!-- Preserve the original terminal transcript and output. -->
<!-- rumdl-disable MD013 -->
```text
                                  List of relations
       Schema       |                 Name                  |    Type     |  Owner   
--------------------+---------------------------------------+-------------+----------
 humanresources     | department                            | table       | postgres
 humanresources     | employee                              | table       | postgres
 humanresources     | employeedepartmenthistory             | table       | postgres
 humanresources     | employeepayhistory                    | table       | postgres
 humanresources     | jobcandidate                          | table       | postgres
 humanresources     | shift                                 | table       | postgres
 information_schema | sql_features                          | table       | postgres
```

<!-- rumdl-enable MD013 -->
### 4. Dump the Schema

The `adventureworks-for-postgres-db` container runs `PostgreSQL` version 14, but
your local `pg_dump` might be a different version (e.g., 15, 16, or 17). Because
`pg_dump` must exactly match the server version, we run it inside the container
to avoid version mismatch errors.

Run the following to dump the schema:

<!-- Preserve the original terminal transcript and output. -->
<!-- rumdl-disable MD013 -->
```bash
CONTAINER_NAME=$(docker container list --format json | \
  jq -r 'select(.Image=="adventureworks-for-postgres-db") | .Names')

(
  source .env
  PG_ENV=$(env | grep -E '^PG' | cut -d '=' -f 1 | xargs -I{} bash -c 'echo -n "-e {}=${} "')

  docker exec $PG_ENV "$CONTAINER_NAME" pg_dump -s > schema.develop.sql
)
```

<!-- rumdl-enable MD013 -->
### 5. Shut Down the Container (Optional)

```bash
docker compose down
```

## Implementing Application

### 1. First requirement List Customers

The requirement is just three words: **“Show list customers.”**

Let's start by exploring the raw data.

<!-- Preserve the original terminal transcript and output. -->
<!-- rumdl-disable MD013 -->
```console
kazuk:~/sql-fun/examples/adventure-works$ psql
psql (14.17 (Ubuntu 14.17-0ubuntu0.22.04.1), server 17.4 (Debian 17.4-1.pgdg120+2))
WARNING: psql major version 14, server major version 17.
         Some psql features might not work.
Type "help" for help.

Adventureworks=# select * from sales.customer;
 customerid | personid | storeid | territoryid |               rowguid                |      modifieddate       
------------+----------+---------+-------------+--------------------------------------+-------------------------
          1 |          |     934 |           1 | 3f5ae95e-b87d-4aed-95b4-c3797afcb74f | 2014-09-12 11:15:07.263
          2 |          |    1028 |           1 | e552f657-a9af-4a7d-a645-c429d6e02491 | 2014-09-12 11:15:07.263
          3 |          |     642 |           4 | 130774b1-db21-4ef3-98c8-c104bcd6ed6d | 2014-09-12 11:15:07.263
...
```

<!-- rumdl-enable MD013 -->
Oh no. These columns don’t contain very user-friendly information.

Let’s check the schema. Open `schema.develop.sql` in your editor and search for
`sales.customer`.

```sql
CREATE TABLE sales.customer (
    customerid integer NOT NULL,
    personid integer,
    storeid integer,
    territoryid integer,
    rowguid uuid DEFAULT public.uuid_generate_v1() NOT NULL,
    modifieddate timestamp without time zone DEFAULT now() NOT NULL
);
```

That’s... not much help either.
Keep searching, and you’ll eventually find something more interesting:

<!-- Preserve the original terminal transcript and output. -->
<!-- rumdl-disable MD013 -->
```sql
CREATE VIEW sales.vindividualcustomer AS
 SELECT p.businessentityid,
    p.title,
    p.firstname,
    p.middlename, ...
   FROM (((((((((person.person p
     JOIN person.businessentityaddress bea ON ((bea.businessentityid = p.businessentityid)))
     JOIN person.address a ON ((a.addressid = bea.addressid)))
     JOIN person.stateprovince sp ON ((sp.stateprovinceid = a.stateprovinceid)))
     JOIN person.countryregion cr ON (((cr.countryregioncode)::text = (sp.countryregioncode)::text)))
     JOIN person.addresstype at ON ((at.addresstypeid = bea.addresstypeid)))
     JOIN sales.customer c ON ((c.personid = p.businessentityid)))   <------ joins sales.customer ! and join to person.person 
     LEFT JOIN person.emailaddress ea ON ((ea.businessentityid = p.businessentityid)))
     LEFT JOIN person.personphone pp ON ((pp.businessentityid = p.businessentityid)))
     LEFT JOIN person.phonenumbertype pnt ON ((pnt.phonenumbertypeid = pp.phonenumbertypeid)))
  WHERE (c.storeid IS NULL); <----- and selects storeid is null
```

<!-- rumdl-enable MD013 -->
Ok, check the this view.

> 🔍 Pro Tip: Pretty JSON Output You can use `psql -t -c` along with
> `row_to_json()` and `jq` to see clean, structured output:

<!-- Preserve the original terminal transcript and output. -->
<!-- rumdl-disable MD013 -->
```console
:~/sql-fun/examples/adventure-works$ psql -t -c "select row_to_json(v) from sales.vindividualcustomer v limit 2 offset 0" | jq
{
  "businessentityid": 13531,
  "title": null,
  "firstname": "Jon",
  "middlename": "V",
  "lastname": "Yang",
  "suffix": null,
  "phonenumber": "1 (11) 500 555-0162",
  "phonenumbertype": "Home",
  "emailaddress": "jon24@adventure-works.com",
  "emailpromotion": 1,
  "addresstype": "Home",
  "addressline1": "3761 N. 14th St",
  "addressline2": null,
  …
}
{
  "businessentityid": 5454,
  "title": null,
  "firstname": "Eugene",
  "middlename": "L",
  "lastname": "Huang",
  "suffix": null,
  "phonenumber": "1 (11) 500 555-0110",
  "phonenumbertype": "Cell",
  "emailaddress": "eugene10@adventure-works.com",
  "emailpromotion": 0,
  "addresstype": "Home",
  "addressline1": "2243 W St.",
  "addressline2": null,
  …
}
```

<!-- rumdl-enable MD013 -->
Ok lets find `customer.storeid` relationship target.

<!-- Preserve the original terminal transcript and output. -->
<!-- rumdl-disable MD013 -->
```console
kazuk:~/sql-fun/examples/adventure-works$ grep 'customer.storeid' schema.develop.sql 
-- Name: COLUMN customer.storeid; Type: COMMENT; Schema: sales; Owner: postgres
COMMENT ON COLUMN sales.customer.storeid IS 'Foreign key to Store.BusinessEntityID';
```

<!-- rumdl-enable MD013 -->
What?! It’s just a comment, not an actual foreign key?

...but maybe we can still join it.

Let’s try:

<!-- Preserve the original terminal transcript and output. -->
<!-- rumdl-disable MD013 -->
```console
kazuk:~/sql-fun/examples/adventure-works$ psql -t -c "select row_to_json(v) from (select c.*, s.* from sales.customer c inner join sales.store s on c.storeid = s.businessentityid limit 2 offset 0) v " | jq
{
  "customerid": 1,
  "personid": null,
  "storeid": 934,
  "territoryid": 1,
  "rowguid": "80b15702-df20-4df9-876c-2cf1c3978d39",
  "modifieddate": "2014-09-12T11:15:07.497",
  "businessentityid": 934,
  "name": "A Bike Store",
  "salespersonid": 280,
  "demographics": "<StoreSurvey xmlns=\"http://schemas.microsoft.com/sqlserver/2004/07/adventure-works/StoreSurvey\"><AnnualSales>300000</AnnualSales><AnnualRevenue>30000</AnnualRevenue><BankName>International Bank</BankName><BusinessType>BM</BusinessType><YearOpened>1970</YearOpened><Specialty>Road</Specialty><SquareFeet>7000</SquareFeet><Brands>AW</Brands><Internet>T1</Internet><NumberEmployees>2</NumberEmployees></StoreSurvey>"
}
...
```

<!-- rumdl-enable MD013 -->
#### Now We know

> ⚠️ Table and view names often lie. Because names are costly to change, they
> can become outdated, misleading, or simply wrong over time.

- The `sales.customer` table appears to include both `person.person` and
  `sales.store` customers.
- When `storeid` is NULL, the customer may be an individual.
  - This is suggested by the `sales.vindividualcustomer` view, which joins on
    `personid` and filters for `storeid` IS NULL.
  - The view name hints at "individual customers", but that naming isn’t a
    guarantee.
- When `storeid` is present, it likely represents a store.
  - You can join it with `sales.store` for more details.

> The absence of results can be treated as a bug,
> but an excess of results becomes a data leak incident.

So we ask the PM:  
> "Which type of customer should be listed — those linked to `person`, or
> `store`, or both?"

### 🗣️ Developer–PM Conversation (Customer Listing)

**Dev:** To clarify — which type of customer should we support in this
feature?  
Customers linked to `person.person`, `sales.store`, or both?

**PM:** Yeah, let’s start with just the individual customers.  
The store-based ones are corporate accounts — they follow a different workflow
and will be handled on a separate screen later.

**Dev:** Got it.  
What fields do you need? I already know how to retrieve name, phone, email, and
address.

**PM:** Just the name and email address.  
This is for a list to send new product announcements via email.

**Dev:** Understood.  
There’s a view called `sales.vindividualcustomer`. I’ve confirmed that it
includes all the info we need.  
Should I just filter down the result to name and email from that?

**PM:** That should be sufficient.

**Dev:** One more thing — the view seems to only contain individual customers,
so would you be okay with an endpoint named `/individual_customers`?  
Or do you prefer a more specific name, since the fields are limited to name and
email?

**PM:** Hmm… with an endpoint like `/individual_customers`, it kind of implies
the full entity.  
Would that be misleading?

**Dev:** I’m fine with returning all the fields if that’s more consistent.

**PM:** Then let’s go with that.  
Just return everything — we’ll consume only what we need on the client side.

### ✅ Refined Requirement: Our Defined Goal

The original requirement was vague:

> "Show list customers."

After clarification with the PM. we refined each term in the requirement:

Breakdown:

Term      | Clarified Meaning
-- | --
Show      | Provide an HTTP API (RESTful) to serve the data.
list      | Return the result as a JSON array.
customers | Refer specifically to individual customers, not stores — based on the view `sales.vindividualcustomer`.

The PM has confirmed:

- Only person-based customers are in scope.
- Full view output is acceptable (even if some fields are unused).
- The endpoint name `/individual_customers` is appropriate and clear.

Based on this, we define our implementation goal as follows:

> Implement a REST API endpoint at `/individual_customers` that returns a JSON
> array of all records from the `sales.vindividualcustomer` view, including all
> fields.

### Result set definition

First option , most faster way.

`sql_fun` has a `from_table` derive macro, this works for view.

```rust
#[derive(sql_fun::from_table)]
#[derive(Debug, derive_builder::Builder)]
#[sql_fun::source("sales.vindividualcustomer")]
struct IndividualCustomer {}
```

this struct fits `select ic.* from sales.vindividualcustomer ic`.

## 🛠 Troubleshooting

- **Database container won’t start?**  
  Make sure the `.zip` file was downloaded and unzipped correctly before running
  `docker compose up`.

- **Can't connect via `psql`?**  
  Double-check your `.env` and make sure you’ve run `source .env`.

- **No tables shown with `\dt`?**  
  Use `\dt *.*` to show tables across all schemas, or run `\dn` to check which
  schemas exist.
