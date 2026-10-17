//! Production schema-provider coverage for view writes. Runs in the workspace flow.
mod test_helpers {
    pub mod heavy {
        use ::clap::Parser;
        use ::sql_fun_core::{SqlFunArgs, SqlFunMetadata};
        use ::sql_fun_sqlast::{
            ParsedSchemaDump, PostgresDialect,
            sem::{BaseContext, FrozenBaseContext, SchemaFileContext, SemAst},
        };
        use std::path::Path;

        pub struct ViewSchema {
            args: SqlFunArgs,
            metadata: SqlFunMetadata,
            parsed: ParsedSchemaDump,
        }
        impl ViewSchema {
            /// Loads production builtin JSON masters in each test process; the real
            /// provider/loader boundary is the behavior under test. No live server.
            pub fn postgres(sql: &str) -> Result<Self, Box<dyn std::error::Error>> {
                Self::load(sql, false)
            }
            #[cfg(feature = "sqlite")]
            pub fn sqlite(sql: &str) -> Result<Self, Box<dyn std::error::Error>> {
                Self::load(sql, true)
            }
            pub fn builtin_settings(version: &str) -> Result<Self, Box<dyn std::error::Error>> {
                let path =
                    Path::new(env!("SQL_FUN_HOME")).join(format!("postgres/{version}/schema.sql"));
                let schema = std::fs::read_to_string(path)?;
                let start = schema
                    .find("CREATE VIEW \"pg_catalog\".\"pg_settings\"")
                    .expect("emitted builtin view");
                let remainder = &schema[start..];
                let end = remainder[1..]
                    .find("CREATE VIEW ")
                    .map_or(remainder.len(), |end| end + 1);
                Self::load_version(&remainder[..end], false, version)
            }
            fn load(sql: &str, sqlite: bool) -> Result<Self, Box<dyn std::error::Error>> {
                Self::load_version(sql, sqlite, "17")
            }
            fn load_version(
                sql: &str,
                sqlite: bool,
                version: &str,
            ) -> Result<Self, Box<dyn std::error::Error>> {
                let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
                let metadata_file = if sqlite {
                    manifest.join("tests/fixtures/writable-sqlite.toml")
                } else {
                    manifest.join("tests/fixtures/writable-postgres.toml")
                };
                let args = SqlFunArgs::try_parse_from([
                    "sql-fun",
                    "--metadata-file",
                    metadata_file.to_str().unwrap(),
                    "--sql-fun-home",
                    env!("SQL_FUN_HOME"),
                    "--postgres-version",
                    version,
                ])?;
                let selected = args.load_metadata()?.get("default").unwrap().clone();
                let base = FrozenBaseContext::from(BaseContext::new(&args, &selected)?);
                let file = ::tempfile::NamedTempFile::new()?;
                std::fs::write(file.path(), sql)?;
                let parsed = {
                    #[cfg(feature = "sqlite")]
                    if sqlite {
                        ::sql_fun_sqlast::SqliteDialect::parse_schema_file(
                            file.path(),
                            &base,
                            &mut None,
                            &args,
                            &selected,
                        )?
                    } else {
                        PostgresDialect::parse_schema_file(
                            file.path(),
                            &base,
                            &mut None,
                            &args,
                            &selected,
                        )?
                    }
                    #[cfg(not(feature = "sqlite"))]
                    {
                        PostgresDialect::parse_schema_file(
                            file.path(),
                            &base,
                            &mut None,
                            &args,
                            &selected,
                        )?
                    }
                };
                assert!(parsed.problems().is_empty(), "{:?}", parsed.problems());
                Ok(Self {
                    args,
                    metadata: selected,
                    parsed,
                })
            }
            pub fn layer(&self, sql: &str) -> Result<FrozenBaseContext, String> {
                let mut context = SchemaFileContext::new(
                    Path::new("layer.sql"),
                    self.context(),
                    &self.args,
                    &self.metadata,
                );
                for statement in ::sql_fun_sqlast::parse_statements(
                    sql,
                    ::sql_fun_sqlast::ast::Dialect::from(&self.metadata),
                ) {
                    context = statement
                        .map_err(|error| error.to_string())?
                        .analyze(context)
                        .map_err(|error| error.to_string())?
                        .into_context();
                }
                if !context.problems().is_empty() {
                    return Err(format!("{:?}", context.problems()));
                }
                Ok(::sql_fun_sqlast::sem::ParseContext::freeze(context))
            }
            pub const fn context(&self) -> &FrozenBaseContext {
                self.parsed.base_context()
            }
            pub fn query_problems(
                &self,
                sql: &str,
            ) -> Result<Vec<::sql_fun_sqlast::sem::AnalysisProblem>, String> {
                let context = SchemaFileContext::new(
                    Path::new("query.sql"),
                    self.context(),
                    &self.args,
                    &self.metadata,
                );
                let statement = ::sql_fun_sqlast::parse_statements(
                    sql,
                    ::sql_fun_sqlast::ast::Dialect::from(&self.metadata),
                )
                .next()
                .unwrap()
                .map_err(|error| error.to_string())?;
                let result = statement
                    .analyze(context)
                    .map_err(|error| error.to_string())?;
                Ok(result.into_context().problems().to_vec())
            }
            pub fn analyze(&self, sql: &str) -> Result<SemAst, String> {
                let context = SchemaFileContext::new(
                    Path::new("query.sql"),
                    self.context(),
                    &self.args,
                    &self.metadata,
                );
                let statement = ::sql_fun_sqlast::parse_statements(
                    sql,
                    ::sql_fun_sqlast::ast::Dialect::from(&self.metadata),
                )
                .next()
                .unwrap()
                .map_err(|error| error.to_string())?;
                let result = statement
                    .analyze(context)
                    .map_err(|error| error.to_string())?;
                let ast = result.ast().clone();
                let context = result.into_context();
                if context.problems().is_empty() {
                    Ok(ast)
                } else {
                    Err(format!("{:?}", context.problems()))
                }
            }
        }
    }
}
use self::test_helpers::heavy;
use ::sql_fun_sqlast::sem::{FullName, ViewInsertInfoRead, ViewUpdateInfoRead};

const AUTOMATIC: &str = r"
CREATE TABLE public.items (id integer PRIMARY KEY, value integer DEFAULT 7);
CREATE VIEW public.item_view AS SELECT id AS key, value, value + 1 AS computed FROM public.items;
CREATE VIEW public.nested (renamed, value) AS SELECT key, value FROM public.item_view;
";

#[rstest::rstest]
#[case::insert(
    "INSERT INTO public.item_view(key) VALUES (1) RETURNING key, computed",
    true
)]
#[case::default_values("INSERT INTO public.item_view DEFAULT VALUES", true)]
#[case::update("UPDATE public.item_view SET value = 9 RETURNING key, computed", true)]
#[case::computed_insert("INSERT INTO public.item_view(computed) VALUES (2)", false)]
#[case::computed_default("INSERT INTO public.item_view(computed) VALUES (DEFAULT)", false)]
#[case::computed_update("UPDATE public.item_view SET computed = DEFAULT", false)]
#[case::nested_insert("INSERT INTO public.nested(renamed) VALUES (1)", true)]
#[case::nested_update("UPDATE public.nested SET value = 2", true)]
#[case::conflict(
    "INSERT INTO public.item_view(key) VALUES (1) ON CONFLICT(key) DO NOTHING",
    true
)]
#[case::nested_conflict(
    "INSERT INTO public.nested(renamed) VALUES (1) ON CONFLICT(renamed) DO NOTHING",
    true
)]
fn automatic_view_query(
    #[case] sql: &str,
    #[case] accepted: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(AUTOMATIC)?;
    let result = schema.analyze(sql);
    assert_eq!(result.is_ok(), accepted, "{sql}: {result:?}");
    Ok(())
}

#[rstest::rstest]
#[case::distinct("SELECT DISTINCT id FROM public.items")]
#[case::limit("SELECT id FROM public.items LIMIT 1")]
#[case::join("SELECT a.id FROM public.items a JOIN public.items b ON a.id=b.id")]
#[case::expression_only("SELECT id + 1 AS id FROM public.items")]
#[case::constant("SELECT 1 AS id")]
fn read_only_shapes(#[case] query: &str) -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(&format!(
        "CREATE TABLE public.items(id integer); CREATE VIEW public.v AS {query};"
    ))?;
    let name = FullName::with_schema("public", "v");
    assert!(!schema.context().view_insert_support(&name).is_writable());
    assert!(!schema.context().view_update_support(&name).is_writable());
    Ok(())
}

#[test]
fn writable_columns_preserve_effective_constraints() -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(
        r"
CREATE TABLE public.items(id integer GENERATED ALWAYS AS IDENTITY, value integer DEFAULT 7, calc integer GENERATED ALWAYS AS (42) STORED);
CREATE VIEW public.v AS SELECT id AS key, value, calc, value+1 AS computed FROM public.items;
",
    )?;
    let name = FullName::with_schema("public", "v");
    let support = schema.context().view_update_support(&name);
    let key = support.column_definition(&"key".into()).unwrap();
    assert!(
        key.constraints()
            .iter()
            .any(|constraint| constraint.as_identity().is_some())
    );
    assert!(
        support
            .column_definition(&"calc".into())
            .unwrap()
            .constraints()
            .iter()
            .any(|constraint| constraint.as_generated().is_some())
    );
    assert!(
        support
            .column_definition(&"value".into())
            .unwrap()
            .constraints()
            .iter()
            .any(|constraint| constraint.as_default().is_some())
    );
    assert!(support.column_definition(&"computed".into()).is_none());
    assert!(schema.analyze("UPDATE public.v SET key=1").is_err());
    assert!(
        schema
            .analyze("UPDATE public.v SET key=DEFAULT, calc=DEFAULT")
            .is_err()
    );
    Ok(())
}

#[rstest::rstest]
#[case::insert_trigger(
    "CREATE TRIGGER ti INSTEAD OF INSERT ON public.v FOR EACH ROW EXECUTE FUNCTION public.noop();",
    true,
    false,
    true
)]
#[case::update_trigger(
    "CREATE TRIGGER tu INSTEAD OF UPDATE ON public.v FOR EACH ROW EXECUTE FUNCTION public.noop();",
    false,
    true,
    true
)]
#[case::both_triggers(
    "CREATE TRIGGER ti INSTEAD OF INSERT OR UPDATE ON public.v FOR EACH ROW EXECUTE FUNCTION public.noop();",
    true,
    true,
    true
)]
#[case::insert_rule(
    "CREATE RULE ri AS ON INSERT TO public.v DO INSTEAD NOTHING;",
    true,
    false,
    false
)]
#[case::update_rule(
    "CREATE RULE ru AS ON UPDATE TO public.v DO INSTEAD NOTHING;",
    false,
    true,
    false
)]
#[case::rule_returning(
    "CREATE RULE ri AS ON INSERT TO public.v DO INSTEAD INSERT INTO public.items VALUES(NEW.id) RETURNING id;",
    true,
    false,
    true
)]
#[case::conditional_rule(
    "CREATE RULE ri AS ON INSERT TO public.v WHERE NEW.id > 0 DO INSTEAD NOTHING;",
    false,
    false,
    false
)]
#[case::additional_rule(
    "CREATE RULE ri AS ON INSERT TO public.v DO ALSO INSERT INTO public.items VALUES(NEW.id);",
    false,
    false,
    false
)]
fn independent_write_mechanisms(
    #[case] declaration: &str,
    #[case] insert: bool,
    #[case] update: bool,
    #[case] returning: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(&format!(
        r"
CREATE TABLE public.items(id integer);
CREATE VIEW public.v AS SELECT DISTINCT id FROM public.items;
CREATE FUNCTION public.noop() RETURNS trigger LANGUAGE plpgsql AS 'BEGIN RETURN NEW; END';
{declaration}
"
    ))?;
    let name = FullName::with_schema("public", "v");
    let i = schema.context().view_insert_support(&name);
    let u = schema.context().view_update_support(&name);
    assert_eq!(i.is_writable(), insert);
    assert_eq!(u.is_writable(), update);
    if insert {
        assert_eq!(i.supports_returning(), returning);
    }
    if update {
        assert_eq!(u.supports_returning(), returning);
    }
    if declaration.contains("RULE") {
        assert!(!i.supports_on_conflict());
    }
    assert_eq!(
        schema.analyze("INSERT INTO public.v(id) VALUES(1)").is_ok(),
        insert
    );
    assert_eq!(schema.analyze("UPDATE public.v SET id=1").is_ok(), update);
    if insert {
        assert_eq!(
            schema
                .analyze("INSERT INTO public.v(id) VALUES(1) RETURNING id")
                .is_ok(),
            returning
        );
    }
    Ok(())
}

#[test]
fn view_default_overrides_base_default() -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(&format!(
        "{AUTOMATIC} ALTER VIEW public.item_view ALTER COLUMN value SET DEFAULT 42;"
    ))?;
    let support = schema
        .context()
        .view_insert_support(&FullName::with_schema("public", "item_view"));
    let column = support.column_definition(&"value".into()).unwrap();
    let defaults: Vec<_> = column
        .constraints()
        .iter()
        .filter_map(|constraint| constraint.as_default())
        .collect();
    assert_eq!(defaults.len(), 1);
    assert_eq!(
        defaults[0].expr().as_const(),
        Some(&::sql_fun_sqlast::sem::ScalarConstExpr::new_integer(42))
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::insert(
    "CREATE TRIGGER ti INSTEAD OF INSERT ON v BEGIN SELECT 1; END;",
    "INSERT INTO v(a,b) VALUES(1,2) RETURNING a",
    true
)]
#[case::update(
    "CREATE TRIGGER tu INSTEAD OF UPDATE ON v BEGIN SELECT 1; END;",
    "UPDATE v SET b=2 RETURNING a",
    true
)]
#[case::wrong_event(
    "CREATE TRIGGER ti INSTEAD OF INSERT ON v BEGIN SELECT 1; END;",
    "UPDATE v SET a=2",
    false
)]
#[case::update_of_miss(
    "CREATE TRIGGER tu INSTEAD OF UPDATE OF a ON v BEGIN SELECT 1; END;",
    "UPDATE v SET b=2",
    false
)]
#[case::update_of_hit(
    "CREATE TRIGGER tu INSTEAD OF UPDATE OF a ON v BEGIN SELECT 1; END;",
    "UPDATE v SET a=2,b=3",
    true
)]
#[case::nonexistent_update_of(
    "CREATE TRIGGER tu INSTEAD OF UPDATE OF absent ON v BEGIN SELECT 1; END;",
    "UPDATE v SET a=2",
    false
)]
#[case::upsert(
    "CREATE TRIGGER ti INSTEAD OF INSERT ON v BEGIN SELECT 1; END;",
    "INSERT INTO v(a) VALUES(1) ON CONFLICT(a) DO NOTHING",
    false
)]
fn sqlite_trigger_oracle(
    #[case] trigger: &str,
    #[case] query: &str,
    #[case] accepted: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let ddl = format!(
        "CREATE TABLE items(a PRIMARY KEY,b); CREATE VIEW v AS SELECT a,b FROM items; {trigger}"
    );
    let oracle = ::rusqlite::Connection::open_in_memory()?;
    oracle.execute_batch(&ddl)?;
    eprintln!("bundled SQLite {}", ::rusqlite::version());
    assert_eq!(oracle.prepare(query).is_ok(), accepted, "{query}");
    let schema = heavy::ViewSchema::sqlite(&ddl)?;
    let result = schema.analyze(query);
    assert_eq!(result.is_ok(), accepted, "{query}: {result:?}");
    Ok(())
}

#[rstest::rstest]
#[case::scalar("SELECT id, lower(id::text) AS computed FROM public.items", true)]
#[case::set_returning("SELECT id, pg_ls_dir('unused') AS expanded FROM public.items", false)]
#[case::window("SELECT id, row_number() OVER () AS position FROM public.items", false)]
#[case::aggregate("SELECT bool_and(id > 0) AS total FROM public.items", false)]
fn function_row_properties(
    #[case] query: &str,
    #[case] writable: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(&format!(
        "CREATE TABLE public.items(id integer); CREATE VIEW public.v AS {query};"
    ))?;
    let support = schema
        .context()
        .view_update_support(&FullName::with_schema("public", "v"));
    assert_eq!(support.is_writable(), writable);
    Ok(())
}

// PostgreSQL 15.19/16.15/17.6/18.1: rollback-scoped probes agree on this matrix.
#[rstest::rstest]
#[case::explicit_insert("INSERT INTO public.v(x,y) VALUES(1,2)", false)]
#[case::implicit_insert("INSERT INTO public.v VALUES(1,2)", false)]
#[case::first_implicit("INSERT INTO public.v VALUES(1)", true)]
#[case::default_values("INSERT INTO public.v DEFAULT VALUES", true)]
#[case::duplicate_update("UPDATE public.v SET x=1,y=2", false)]
#[case::subscripts("UPDATE public.v SET p[1]=1,q[2]=2", true)]
fn duplicate_projection_assignments(
    #[case] query: &str,
    #[case] accepted: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(
        "CREATE TABLE public.items(a int,b int[]); CREATE VIEW public.v AS SELECT a AS x,a AS y,b AS p,b AS q FROM public.items;",
    )?;
    let result = schema.analyze(query);
    assert_eq!(result.is_ok(), accepted, "{query}: {result:?}");
    Ok(())
}

#[test]
fn nested_arbiters_retain_the_actual_source() -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(AUTOMATIC)?;
    let statement = schema
        .analyze("INSERT INTO public.nested(renamed) VALUES(1) ON CONFLICT(renamed) DO NOTHING")?;
    let arbiters = statement.as_insert().unwrap().conflicts()[0]
        .arbiter()
        .arbiters()
        .unwrap();
    assert_eq!(arbiters.len(), 1);
    assert_eq!(
        arbiters[0].source_relation().unwrap().full_name(),
        &FullName::with_schema("public", "items")
    );
    assert_eq!(arbiters[0].column().unwrap().as_str(), "id");
    Ok(())
}

#[test]
fn later_layers_and_replace_preserve_frozen_parents() -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(
        r"
CREATE TABLE public.items(id int);
CREATE VIEW public.v AS SELECT DISTINCT id FROM public.items;
CREATE FUNCTION public.noop() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN RETURN NEW; END$$;
",
    )?;
    let name = FullName::with_schema("public", "v");
    let child = schema.layer("CREATE TRIGGER it INSTEAD OF INSERT ON public.v FOR EACH ROW EXECUTE FUNCTION public.noop();")?;
    assert!(child.view_insert_support(&name).is_writable());
    assert!(!child.view_update_support(&name).is_writable());
    assert!(!schema.context().view_insert_support(&name).is_writable());
    let replaced =
        schema.layer("CREATE OR REPLACE VIEW public.v AS SELECT id FROM public.items;")?;
    assert!(replaced.view_update_support(&name).is_writable());
    assert!(!schema.context().view_update_support(&name).is_writable());
    let schema = heavy::ViewSchema::postgres(
        r"
CREATE TABLE public.items(id int);
CREATE VIEW public.v AS SELECT DISTINCT id FROM public.items;
CREATE FUNCTION public.noop() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN RETURN NEW; END$$;
CREATE TRIGGER it INSTEAD OF INSERT ON public.v FOR EACH ROW EXECUTE FUNCTION public.noop();
ALTER VIEW public.v ALTER COLUMN id SET DEFAULT 42;
CREATE OR REPLACE VIEW public.v AS SELECT DISTINCT id FROM public.items;
",
    )?;
    assert!(schema.context().view_insert_support(&name).is_writable());
    let dropped = schema.layer("DROP TRIGGER it ON public.v;")?;
    assert!(!dropped.view_insert_support(&name).is_writable());
    assert!(schema.context().view_insert_support(&name).is_writable());
    Ok(())
}

#[rstest::rstest]
#[case::conditional(
    "CREATE RULE r AS ON INSERT TO public.v WHERE NEW.id>0 DO INSTEAD NOTHING;",
    false,
    false,
    false
)]
#[case::additional(
    "CREATE RULE r AS ON INSERT TO public.v DO ALSO INSERT INTO public.audit VALUES(NEW.id);",
    true,
    true,
    true
)]
#[case::instead(
    "CREATE RULE r AS ON INSERT TO public.v DO INSTEAD NOTHING;",
    true,
    false,
    false
)]
#[case::conditional_trigger(
    "CREATE RULE r AS ON INSERT TO public.v WHERE NEW.id>0 DO INSTEAD NOTHING; CREATE TRIGGER it INSTEAD OF INSERT ON public.v FOR EACH ROW EXECUTE FUNCTION public.noop();",
    true,
    false,
    false
)]
#[case::additional_trigger(
    "CREATE RULE r AS ON INSERT TO public.v DO ALSO INSERT INTO public.audit VALUES(NEW.id); CREATE TRIGGER it INSTEAD OF INSERT ON public.v FOR EACH ROW EXECUTE FUNCTION public.noop();",
    true,
    true,
    false
)]
fn automatic_rule_precedence(
    #[case] declaration: &str,
    #[case] insert: bool,
    #[case] returning: bool,
    #[case] conflict: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(&format!(
        r"
CREATE TABLE public.items(id int PRIMARY KEY);
CREATE TABLE public.audit(id int);
CREATE VIEW public.v AS SELECT id FROM public.items;
CREATE FUNCTION public.noop() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN RETURN NEW; END$$;
{declaration}
"
    ))?;
    let result = schema.analyze("INSERT INTO public.v(id) VALUES(1)");
    assert_eq!(result.is_ok(), insert, "{result:?}");
    assert_eq!(
        schema
            .analyze("INSERT INTO public.v(id) VALUES(1) RETURNING id")
            .is_ok(),
        insert && returning
    );
    assert_eq!(
        schema
            .analyze("INSERT INTO public.v(id) VALUES(1) ON CONFLICT DO NOTHING")
            .is_ok(),
        insert && conflict
    );
    assert!(
        schema
            .analyze("UPDATE public.v SET id=1 RETURNING id")
            .is_ok()
    );
    Ok(())
}

#[test]
fn bound_names_do_not_reselect_shadowed_views() -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(
        "CREATE SCHEMA other; CREATE TABLE public.items(id int); CREATE VIEW public.v AS SELECT id FROM public.items; CREATE VIEW other.v AS SELECT DISTINCT id FROM public.items;",
    )?;
    assert!(
        schema
            .analyze("INSERT INTO v VALUES($1) RETURNING id")
            .is_ok()
    );
    assert!(schema.analyze("UPDATE v SET id=$1 RETURNING id").is_ok());
    assert!(schema.analyze("INSERT INTO other.v VALUES(1)").is_err());
    assert!(
        !schema
            .context()
            .view_update_support(&FullName::with_schema("other", "v"))
            .is_writable()
    );
    assert!(
        schema
            .context()
            .view_update_support(&FullName::with_schema("public", "v"))
            .is_writable()
    );
    Ok(())
}

#[test]
fn materialized_views_are_not_write_destinations() -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(
        "CREATE TABLE public.items(id int); CREATE MATERIALIZED VIEW public.v AS SELECT id FROM public.items;",
    )?;
    assert!(schema.analyze("INSERT INTO public.v VALUES(1)").is_err());
    assert!(schema.analyze("UPDATE public.v SET id=1").is_err());
    assert!(schema.analyze("SELECT id FROM public.v").is_ok());
    assert!(schema.analyze("SELECT * FROM public.v").is_ok());
    Ok(())
}

#[test]
fn defaults_and_rule_drops_change_only_the_child() -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(
        "CREATE TABLE public.items(id int DEFAULT 7); CREATE VIEW public.v AS SELECT DISTINCT id FROM public.items; CREATE RULE r AS ON INSERT TO public.v DO INSTEAD NOTHING;",
    )?;
    let name = FullName::with_schema("public", "v");
    let child = schema.layer("DROP RULE r ON public.v;")?;
    assert!(!child.view_insert_support(&name).is_writable());
    assert!(schema.context().view_insert_support(&name).is_writable());
    assert!(
        schema
            .layer("ALTER VIEW public.v ALTER COLUMN missing SET DEFAULT 42;")
            .is_err()
    );
    assert!(
        schema
            .layer("CREATE RULE r AS ON INSERT TO public.v DO INSTEAD NOTHING;")
            .is_err()
    );
    assert!(
        schema
            .layer("CREATE OR REPLACE RULE r AS ON INSERT TO public.v DO INSTEAD NOTHING;")
            .is_ok()
    );
    assert!(
        schema
            .layer("DROP RULE IF EXISTS absent ON public.v;")
            .is_ok()
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_drop_masks_inherited_trigger() -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::sqlite(
        "CREATE TABLE items(a); CREATE VIEW v AS SELECT a FROM items; CREATE TRIGGER it INSTEAD OF INSERT ON v BEGIN SELECT 1; END;",
    )?;
    let name = FullName::with_schema("main", "v");
    assert!(schema.context().view_insert_support(&name).is_writable());
    let child = schema.layer("DROP TRIGGER it;")?;
    assert!(!child.view_insert_support(&name).is_writable());
    assert!(schema.context().view_insert_support(&name).is_writable());
    assert!(
        schema
            .layer("CREATE TRIGGER IF NOT EXISTS it INSTEAD OF INSERT ON v BEGIN SELECT 1; END;")
            .is_ok()
    );
    assert!(schema.layer("DROP TRIGGER IF EXISTS absent;").is_ok());
    Ok(())
}

#[rstest::rstest]
#[case::scalar("integer", true)]
#[case::set_returning("SETOF integer", false)]
fn declared_function_properties(
    #[case] result: &str,
    #[case] writable: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(&format!(
        "CREATE TABLE public.items(id int); CREATE FUNCTION public.custom(i int) RETURNS {result} LANGUAGE sql AS 'SELECT i'; CREATE VIEW public.v AS SELECT id,public.custom(id) AS expression FROM public.items;"
    ))?;
    assert_eq!(
        schema
            .context()
            .view_update_support(&FullName::with_schema("public", "v"))
            .is_writable(),
        writable
    );
    Ok(())
}

#[rstest::rstest]
#[case::postgres17("17")]
#[case::postgres18("18")]
fn extracted_builtin_rules_reach_production_analysis(
    #[case] version: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::builtin_settings(version)?;
    let name = FullName::with_schema("pg_catalog", "pg_settings");
    assert!(!schema.context().view_insert_support(&name).is_writable());
    assert!(schema.context().view_update_support(&name).is_writable());
    assert!(
        !schema
            .context()
            .view_update_support(&name)
            .supports_returning()
    );
    assert!(
        schema
            .analyze("UPDATE pg_catalog.pg_settings SET setting='on' WHERE name='enable_seqscan'")
            .is_ok()
    );
    assert!(
        schema
            .analyze("UPDATE pg_catalog.pg_settings SET setting='on' RETURNING setting")
            .is_err()
    );
    Ok(())
}

#[rstest::rstest]
#[case::same_type("SELECT id::int AS key FROM public.items", "key", true)]
#[case::different_type("SELECT id::bigint AS key FROM public.items", "key", false)]
#[case::star_then_cast("SELECT *, id::int AS key FROM public.items", "key", true)]
#[case::typmod("SELECT value::varchar(5) AS key FROM public.items", "key", false)]
fn view_cast_lineage(
    #[case] query: &str,
    #[case] column: &str,
    #[case] writable: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(&format!(
        "CREATE TABLE public.items(id int,value varchar); CREATE VIEW public.v AS {query};"
    ))?;
    let result = schema.analyze(&format!("UPDATE public.v SET {column}=DEFAULT"));
    assert_eq!(result.is_ok(), writable, "{result:?}");
    Ok(())
}

#[test]
fn view_write_bindings_and_diagnostics_remain_structured() -> Result<(), Box<dyn std::error::Error>>
{
    use ::sql_fun_sqlast::sem::problem::{InsertProblemKind, UpdateProblemKind};
    let schema = heavy::ViewSchema::postgres(AUTOMATIC)?;
    let insert = schema.analyze("INSERT INTO item_view(key) VALUES($1) RETURNING computed")?;
    let insert = insert.as_insert().unwrap();
    assert_eq!(
        insert.target().full_name(),
        &FullName::with_schema("public", "item_view")
    );
    assert!(insert.view_support().unwrap().is_writable());
    let update = schema.analyze("UPDATE item_view SET value=$1 RETURNING computed")?;
    let update = update.as_update().unwrap();
    assert_eq!(
        update.target().full_name(),
        &FullName::with_schema("public", "item_view")
    );
    assert!(update.view_support().unwrap().is_writable());
    let problems = schema.query_problems("UPDATE item_view SET value=1,value=2")?;
    assert!(problems.iter().any(|problem| {
        problem
            .update_problem()
            .is_some_and(|problem| problem.kind() == UpdateProblemKind::DuplicateTarget)
    }));
    let problems = schema.query_problems("INSERT INTO item_view(key,key) VALUES(1,2)")?;
    assert!(problems.iter().any(|problem| {
        problem
            .insert_problem()
            .is_some_and(|problem| problem.kind() == InsertProblemKind::DuplicateTarget)
    }));
    Ok(())
}

// All four PostgreSQL development versions reject view UPDATE DEFAULT for
// GENERATED/ALWAYS identity, but accept the corresponding INSERT DEFAULT.
#[rstest::rstest]
#[case::identity_insert("INSERT INTO public.v(key) VALUES(DEFAULT)", true)]
#[case::generated_insert("INSERT INTO public.v(calc) VALUES(DEFAULT)", true)]
#[case::identity_update("UPDATE public.v SET key=DEFAULT", false)]
#[case::generated_update("UPDATE public.v SET calc=DEFAULT", false)]
#[case::by_default_update("UPDATE public.v SET other=DEFAULT", true)]
fn view_default_rewrite(
    #[case] query: &str,
    #[case] accepted: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(
        "CREATE TABLE public.items(id int GENERATED ALWAYS AS IDENTITY, calc int GENERATED ALWAYS AS(42) STORED, other int GENERATED BY DEFAULT AS IDENTITY); CREATE VIEW public.v AS SELECT id AS key,calc,other FROM public.items;",
    )?;
    let result = schema.analyze(query);
    assert_eq!(result.is_ok(), accepted, "{result:?}");
    Ok(())
}

#[test]
fn polymorphic_function_properties_use_the_producer_signature()
-> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(
        "CREATE TABLE public.items(id int); CREATE FUNCTION public.custom(i anyelement) RETURNS anyelement LANGUAGE sql AS 'SELECT i'; CREATE FUNCTION public.custom(i text) RETURNS SETOF text LANGUAGE sql AS 'SELECT i'; CREATE VIEW public.v AS SELECT id,public.custom(id) AS expression FROM public.items;",
    )?;
    assert!(
        schema
            .context()
            .view_update_support(&FullName::with_schema("public", "v"))
            .is_writable()
    );
    Ok(())
}

#[rstest::rstest]
#[case::computed("INSERT INTO public.v(id) VALUES(1) ON CONFLICT(computed) DO NOTHING")]
#[case::nested("INSERT INTO public.nested(id) VALUES(1) ON CONFLICT(renamed) DO NOTHING")]
#[case::expression("INSERT INTO public.v(id) VALUES(1) ON CONFLICT((id+1)) DO NOTHING")]
fn view_inference_expands_projection_expressions(
    #[case] query: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(
        "CREATE TABLE public.items(a int); CREATE UNIQUE INDEX expression_key ON public.items((a+1)); CREATE VIEW public.v AS SELECT a AS id,a+1 AS computed FROM public.items; CREATE VIEW public.nested(id,renamed) AS SELECT id,computed FROM public.v;",
    )?;
    let statement = schema.analyze(query)?;
    let arbiters = statement.as_insert().unwrap().conflicts()[0]
        .arbiter()
        .arbiters()
        .unwrap();
    assert_eq!(arbiters.len(), 1);
    assert_eq!(
        arbiters[0].index_name(),
        Some(&FullName::with_schema("public", "expression_key"))
    );
    assert_eq!(
        arbiters[0].source_relation().unwrap().full_name(),
        &FullName::with_schema("public", "items")
    );
    Ok(())
}
