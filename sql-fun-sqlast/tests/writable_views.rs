//! View-write semantics using the maintained sql-fun-home catalogs.
//! All cases run in the standard workspace flow.
use ::sql_fun_sqlast::context::RuleInfoRead;
use sql_fun_core::analysis_problem::InsertProblemKind;
use sql_fun_core::analysis_problem::UpdateProblemKind;
use sql_fun_sqlast::context::BaseParseContext;
use sql_fun_sqlast::context::ParseContext;
use sql_fun_sqlast::sem::RuleIdentity;
use sql_fun_sqlast::sem::ViewName;
use sql_fun_sqlast::sem::ViewWriteEvaluation;

mod test_helpers {
    pub mod heavy {
        use ::clap::Parser;
        use ::sql_fun_core::{SqlFunArgs, SqlFunMetadata};
        use ::sql_fun_sqlast::{
            ParsedSchemaDump, PostgresDialect,
            sem::{BaseContext, FrozenBaseContext, SchemaFileContext, SemAst},
        };
        use sql_fun_sqlast::context::ParseContext;
        use std::path::Path;

        pub struct ViewSchema {
            args: SqlFunArgs,
            metadata: SqlFunMetadata,
            parsed: ParsedSchemaDump,
        }
        impl ViewSchema {
            /// Load the maintained `PostgreSQL` 17 catalog for view-write analysis.
            /// Each call reloads the full catalog without sharing across test processes.
            pub fn postgres(sql: &str) -> Result<Self, Box<dyn std::error::Error>> {
                Self::load_from_catalog(sql, false, "17", Path::new(env!("SQL_FUN_HOME")))
            }
            /// Load the maintained catalog with SQLite metadata for view-write analysis.
            /// Each call reloads the full catalog without sharing across test processes.
            #[cfg(feature = "sqlite")]
            pub fn sqlite(sql: &str) -> Result<Self, Box<dyn std::error::Error>> {
                Self::load_from_catalog(sql, true, "17", Path::new(env!("SQL_FUN_HOME")))
            }
            fn load_from_catalog(
                sql: &str,
                sqlite: bool,
                version: &str,
                builtin_home: &Path,
            ) -> Result<Self, Box<dyn std::error::Error>> {
                let args = SqlFunArgs::try_parse_from([
                    "sql-fun",
                    "--sql-fun-home",
                    builtin_home.to_str().unwrap(),
                    "--postgres-version",
                    version,
                ])?;
                let selected = if sqlite {
                    ::serde_json::from_value(::serde_json::json!({
                        "engine": "SQLite", "dialect": "SQLite", "version": "3",
                        "search_path": ["main"]
                    }))?
                } else {
                    ::serde_json::from_value(::serde_json::json!({
                        "engine": "PostgreSQL", "dialect": "postgres", "version": "17",
                        "search_path": ["public", "pg_catalog"]
                    }))?
                };
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
                self.layer_at(self.context(), sql)
            }
            pub fn mask_trigger(
                &self,
                identity: &::sql_fun_sqlast::sem::TriggerIdentity,
            ) -> Result<FrozenBaseContext, String> {
                use ::sql_fun_sqlast::context::TriggerInfoWrite;
                SchemaFileContext::new(
                    Path::new("mask.sql"),
                    self.context(),
                    &self.args,
                    &self.metadata,
                )
                .cancel_trigger(identity)
                .map(ParseContext::freeze)
                .map_err(|error| error.to_string())
            }
            pub fn mask_rule(
                &self,
                identity: &::sql_fun_sqlast::sem::RuleIdentity,
            ) -> Result<FrozenBaseContext, String> {
                use ::sql_fun_sqlast::context::RuleInfoWrite;
                SchemaFileContext::new(
                    Path::new("mask.sql"),
                    self.context(),
                    &self.args,
                    &self.metadata,
                )
                .cancel_rule(identity)
                .map(ParseContext::freeze)
                .map_err(|error| error.to_string())
            }
            pub fn layer_at(
                &self,
                base: &FrozenBaseContext,
                sql: &str,
            ) -> Result<FrozenBaseContext, String> {
                let mut context = SchemaFileContext::new(
                    Path::new("layer.sql"),
                    base,
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
                Ok(context.freeze())
            }
            pub const fn context(&self) -> &FrozenBaseContext {
                self.parsed.base_context()
            }
            pub fn query_problems(
                &self,
                sql: &str,
            ) -> Result<Vec<::sql_fun_sqlast::sem::AnalysisProblem>, String> {
                self.query_problems_at(self.context(), sql)
            }
            pub fn without_search_path(&self) -> FrozenBaseContext {
                let mut context = SchemaFileContext::new(
                    Path::new("empty-path.sql"),
                    self.context(),
                    &self.args,
                    &self.metadata,
                );
                context.set_search_path(&[]);
                context.freeze()
            }
            pub fn detached_view_catalog(
                &self,
                name: &::sql_fun_sqlast::sem::FullName,
            ) -> Result<FrozenBaseContext, String> {
                use ::sql_fun_sqlast::context::BaseParseContext;
                use sql_fun_sqlast::context::ViewInfoWrite;

                let view = self
                    .context()
                    .get_view(&::sql_fun_sqlast::sem::ViewName::from_full_name(name))
                    .unwrap()
                    .ordinary_definition()
                    .unwrap();
                let base = FrozenBaseContext::from(
                    BaseContext::new(&self.args, &self.metadata)
                        .map_err(|error| error.to_string())?,
                );
                SchemaFileContext::new(Path::new("detached.sql"), &base, &self.args, &self.metadata)
                    .apply_create_view(view)
                    .map(ParseContext::freeze)
                    .map_err(|error| error.to_string())
            }
            pub fn incomplete_catalog(&self) -> Result<FrozenBaseContext, String> {
                use sql_fun_sqlast::context::ViewInfoWrite;

                let SemAst::CreateView(view) =
                    self.analyze("CREATE VIEW public.incomplete AS SELECT id FROM public.items")?
                else {
                    panic!("expected view")
                };
                let context = SchemaFileContext::new(
                    Path::new("older.sql"),
                    &FrozenBaseContext::from(
                        BaseContext::new(&self.args, &self.metadata)
                            .map_err(|error| error.to_string())?,
                    ),
                    &self.args,
                    &self.metadata,
                );
                let context = context
                    .apply_create_view(&view)
                    .map_err(|error| error.to_string())?;
                let statement = ::sql_fun_sqlast::parse_statements(
                    "CREATE VIEW public.incomplete_nested AS SELECT id FROM public.incomplete",
                    ::sql_fun_sqlast::ast::Dialect::from(&self.metadata),
                )
                .next()
                .unwrap()
                .map_err(|error| error.to_string())?;
                Ok(statement
                    .analyze(context)
                    .map_err(|error| error.to_string())?
                    .into_context()
                    .freeze())
            }
            pub fn query_problems_at(
                &self,
                base: &FrozenBaseContext,
                sql: &str,
            ) -> Result<Vec<::sql_fun_sqlast::sem::AnalysisProblem>, String> {
                Ok(self.analyze_statements_at(base, sql)?.problems().to_vec())
            }
            pub fn analyze_statements_at(
                &self,
                base: &FrozenBaseContext,
                sql: &str,
            ) -> Result<SchemaFileContext, String> {
                let mut context = SchemaFileContext::new(
                    Path::new("query.sql"),
                    base,
                    &self.args,
                    &self.metadata,
                );
                for statement in ::sql_fun_sqlast::parse_statements(
                    sql,
                    ::sql_fun_sqlast::ast::Dialect::from(&self.metadata),
                ) {
                    let statement = statement.map_err(|error| error.to_string())?;
                    context.set_statement_span(*statement.span());
                    context = statement
                        .analyze(context)
                        .map_err(|error| error.to_string())?
                        .into_context();
                }
                Ok(context)
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

        impl ViewSchema {
            pub fn analysis_context_at(&self, base: &FrozenBaseContext) -> SchemaFileContext {
                SchemaFileContext::new(
                    Path::new("view-analysis.sql"),
                    base,
                    &self.args,
                    &self.metadata,
                )
            }
            pub fn analyze_insert_at(
                &self,
                base: &FrozenBaseContext,
                view: &::sql_fun_sqlast::sem::FullName,
            ) -> ::sql_fun_sqlast::sem::ViewInsertSupport {
                ::sql_fun_sqlast::sem::ViewWriteEvaluation::new(&mut self.analysis_context_at(base))
                    .insert(view)
                    .expect("analysis engine succeeds")
                    .expect("complete view write facts")
            }
            pub fn analyze_update_at(
                &self,
                base: &FrozenBaseContext,
                view: &::sql_fun_sqlast::sem::FullName,
            ) -> ::sql_fun_sqlast::sem::ViewUpdateSupport {
                ::sql_fun_sqlast::sem::ViewWriteEvaluation::new(&mut self.analysis_context_at(base))
                    .update(view)
                    .expect("analysis engine succeeds")
                    .expect("complete view write facts")
            }
            pub fn analyze_insert(
                &self,
                view: &::sql_fun_sqlast::sem::FullName,
            ) -> ::sql_fun_sqlast::sem::ViewInsertSupport {
                self.analyze_insert_at(self.context(), view)
            }
            pub fn analyze_update(
                &self,
                view: &::sql_fun_sqlast::sem::FullName,
            ) -> ::sql_fun_sqlast::sem::ViewUpdateSupport {
                self.analyze_update_at(self.context(), view)
            }
        }

        /// Read emitted `pg_settings` and load its full production builtin catalog.
        /// Each call loads the selected version independently, without caching.
        pub fn builtin_settings(version: &str) -> Result<ViewSchema, Box<dyn std::error::Error>> {
            let path = std::path::Path::new(env!("SQL_FUN_HOME"))
                .join(format!("postgres/{version}/schema.sql"));
            let schema = std::fs::read_to_string(path)?;
            let start = schema
                .find("CREATE VIEW \"pg_catalog\".\"pg_settings\"")
                .expect("emitted builtin view");
            let remainder = &schema[start..];
            let end = remainder[1..]
                .find("CREATE VIEW ")
                .map_or(remainder.len(), |end| end + 1);
            ViewSchema::load_from_catalog(
                &remainder[..end],
                false,
                version,
                std::path::Path::new(env!("SQL_FUN_HOME")),
            )
        }
    }
}

use self::test_helpers::heavy;
use ::sql_fun_sqlast::sem::FullName;

const AUTOMATIC: &str = r"
CREATE TABLE public.items (id integer PRIMARY KEY, value integer DEFAULT 7);
CREATE VIEW public.item_view AS SELECT id AS key, value, value + 1 AS computed FROM public.items;
CREATE VIEW public.nested (renamed, value) AS SELECT key, value FROM public.item_view;
";

#[rstest::rstest]
#[case::distinct("SELECT DISTINCT id FROM public.items")]
#[case::limit("SELECT id FROM public.items LIMIT 1")]
#[case::join("SELECT a.id FROM public.items a JOIN public.items b ON a.id=b.id")]
#[case::expression_only("SELECT id + 1 AS id FROM public.items")]
#[case::constant("SELECT 1 AS id")]
#[case::grouping("SELECT id, GROUPING(id) AS grouped FROM public.items GROUP BY id")]
fn read_only_shapes(#[case] query: &str) -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(&format!(
        "CREATE TABLE public.items(id integer); CREATE VIEW public.v AS {query};"
    ))?;
    let name = FullName::with_schema("public", "v");
    assert!(!schema.analyze_insert(&name).is_writable());
    assert!(!schema.analyze_update(&name).is_writable());
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
    let support = schema.analyze_update(&name);
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
    Ok(())
}

#[rstest::rstest]
#[case::insert_trigger(
    "CREATE TRIGGER ti INSTEAD OF INSERT ON public.v FOR EACH ROW EXECUTE FUNCTION public.noop();",
    true,
    false,
    true,
    true
)]
#[case::update_trigger(
    "CREATE TRIGGER tu INSTEAD OF UPDATE ON public.v FOR EACH ROW EXECUTE FUNCTION public.noop();",
    false,
    true,
    true,
    false
)]
#[case::both_triggers(
    "CREATE TRIGGER ti INSTEAD OF INSERT OR UPDATE ON public.v FOR EACH ROW EXECUTE FUNCTION public.noop();",
    true,
    true,
    true,
    true
)]
#[case::insert_rule(
    "CREATE RULE ri AS ON INSERT TO public.v DO INSTEAD NOTHING;",
    true,
    false,
    false,
    true
)]
#[case::update_rule(
    "CREATE RULE ru AS ON UPDATE TO public.v DO INSTEAD NOTHING;",
    false,
    true,
    false,
    false
)]
#[case::rule_returning(
    "CREATE RULE ri AS ON INSERT TO public.v DO INSTEAD INSERT INTO public.items VALUES(NEW.id) RETURNING id;",
    true,
    false,
    true,
    false
)]
#[case::conditional_rule(
    "CREATE RULE ri AS ON INSERT TO public.v WHERE NEW.id > 0 DO INSTEAD NOTHING;",
    false,
    false,
    false,
    false
)]
#[case::additional_rule(
    "CREATE RULE ri AS ON INSERT TO public.v DO ALSO INSERT INTO public.items VALUES(NEW.id);",
    false,
    false,
    false,
    false
)]
fn independent_write_mechanisms(
    #[case] declaration: &str,
    #[case] insert: bool,
    #[case] update: bool,
    #[case] returning: bool,
    #[case] conflict: bool,
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
    let i = schema.analyze_insert(&name);
    let u = schema.analyze_update(&name);
    assert_eq!(i.is_writable(), insert);
    assert_eq!(u.is_writable(), update);
    if insert {
        assert_eq!(i.supports_returning(), returning);
    }
    if update {
        assert_eq!(u.supports_returning(), returning);
    }
    assert_eq!(i.supports_on_conflict(), conflict);
    Ok(())
}

#[test]
fn view_default_overrides_base_default() -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(&format!(
        "{AUTOMATIC} ALTER VIEW public.item_view ALTER COLUMN value SET DEFAULT 42;"
    ))?;
    let support = schema.analyze_insert(&FullName::with_schema("public", "item_view"));
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
    let support = schema.analyze_update(&FullName::with_schema("public", "v"));
    assert_eq!(support.is_writable(), writable);
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
    assert!(schema.analyze_insert_at(&child, &name).is_writable());
    assert!(!schema.analyze_update_at(&child, &name).is_writable());
    assert!(!schema.analyze_insert(&name).is_writable());
    let replaced =
        schema.layer("CREATE OR REPLACE VIEW public.v AS SELECT id FROM public.items;")?;
    assert!(schema.analyze_update_at(&replaced, &name).is_writable());
    assert!(!schema.analyze_update(&name).is_writable());
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
    assert!(schema.analyze_insert(&name).is_writable());
    let dropped = schema.layer("DROP TRIGGER it ON public.v;")?;
    assert!(!schema.analyze_insert_at(&dropped, &name).is_writable());
    assert!(schema.analyze_insert(&name).is_writable());
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
#[case::additional_update(
    "CREATE RULE r AS ON UPDATE TO public.v DO ALSO INSERT INTO public.audit VALUES(NEW.id);",
    true,
    true,
    true
)]
#[case::instead(
    "CREATE RULE r AS ON INSERT TO public.v DO INSTEAD NOTHING;",
    true,
    false,
    true
)]
#[case::conditional_trigger(
    "CREATE RULE r AS ON INSERT TO public.v WHERE NEW.id>0 DO INSTEAD NOTHING; CREATE TRIGGER it INSTEAD OF INSERT ON public.v FOR EACH ROW EXECUTE FUNCTION public.noop();",
    true,
    false,
    true
)]
#[case::additional_trigger(
    "CREATE RULE r AS ON INSERT TO public.v DO ALSO INSERT INTO public.audit VALUES(NEW.id); CREATE TRIGGER it INSTEAD OF INSERT ON public.v FOR EACH ROW EXECUTE FUNCTION public.noop();",
    true,
    true,
    false
)]
fn rule_precedence_support_and_bindings(
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
    let name = FullName::with_schema("public", "v");
    let support = schema.analyze_insert(&name);
    assert_eq!(support.is_writable(), insert);
    assert_eq!(support.supports_returning(), insert && returning);
    assert_eq!(support.supports_on_conflict(), insert && conflict);
    let sql = "INSERT INTO public.v(id) VALUES(1) ON CONFLICT DO NOTHING";
    if insert && conflict {
        let statement = schema.analyze(sql)?;
        let statement = statement.as_insert().unwrap();
        assert_eq!(statement.target().full_name(), &name);
        assert!(statement.view_support().unwrap().supports_on_conflict());
        assert_eq!(statement.conflicts().len(), 1);
    } else {
        use ::sql_fun_sqlast::sem::problem::InsertProblemKind;
        let problems = schema.query_problems(sql)?;
        assert_eq!(problems.len(), 1);
        let expected = if insert {
            InsertProblemKind::view_operation()
        } else {
            InsertProblemKind::read_only_target()
        };
        assert_eq!(problems[0].insert_problem().unwrap().kind(), expected);
    }
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
            .analyze_update(&FullName::with_schema("other", "v"))
            .is_writable()
    );
    assert!(
        schema
            .analyze_update(&FullName::with_schema("public", "v"))
            .is_writable()
    );
    Ok(())
}

#[test]
fn materialized_views_are_not_write_destinations() -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(
        "CREATE TABLE public.items(id int); CREATE MATERIALIZED VIEW public.v AS SELECT id FROM public.items;",
    )?;
    let name = FullName::with_schema("public", "v");
    let support = ::sql_fun_sqlast::sem::ViewWriteEvaluation::new(
        &mut schema.analysis_context_at(schema.context()),
    )
    .evaluate(&name)
    .unwrap()
    .unwrap();
    assert!(!support.insert().is_writable());
    assert!(!support.update().is_writable());
    Ok(())
}

#[test]
fn defaults_and_rule_drops_change_only_the_child() -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(
        "CREATE TABLE public.items(id int DEFAULT 7); CREATE VIEW public.v AS SELECT DISTINCT id FROM public.items; CREATE RULE r AS ON INSERT TO public.v DO INSTEAD NOTHING;",
    )?;
    let name = FullName::with_schema("public", "v");
    let child = schema.layer("DROP RULE r ON public.v;")?;
    assert!(!schema.analyze_insert_at(&child, &name).is_writable());
    assert!(schema.analyze_insert(&name).is_writable());
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
    assert!(schema.analyze_insert(&name).is_writable());
    let child = schema.layer("DROP TRIGGER it;")?;
    assert!(!schema.analyze_insert_at(&child, &name).is_writable());
    assert!(schema.analyze_insert(&name).is_writable());
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
            .analyze_update(&FullName::with_schema("public", "v"))
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
    let schema = heavy::builtin_settings(version)?;
    let name = FullName::with_schema("pg_catalog", "pg_settings");
    assert!(!schema.analyze_insert(&name).is_writable());
    assert!(schema.analyze_update(&name).is_writable());
    assert!(!schema.analyze_update(&name).supports_returning());
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

#[test]
fn view_write_bindings_and_diagnostics_remain_structured() -> Result<(), Box<dyn std::error::Error>>
{
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

#[test]
fn polymorphic_function_properties_use_the_producer_signature()
-> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(
        "CREATE TABLE public.items(id int); CREATE FUNCTION public.custom(i anyelement) RETURNS anyelement LANGUAGE sql AS 'SELECT i'; CREATE FUNCTION public.custom(i text) RETURNS SETOF text LANGUAGE sql AS 'SELECT i'; CREATE VIEW public.v AS SELECT id,public.custom(id) AS expression FROM public.items;",
    )?;
    assert!(
        schema
            .analyze_update(&FullName::with_schema("public", "v"))
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

#[rstest::rstest]
#[case::missing_alter(
    "ALTER VIEW public.absent ALTER COLUMN id SET DEFAULT 2",
    "view_invalid_target"
)]
#[case::table_alter(
    "ALTER VIEW public.items ALTER COLUMN id SET DEFAULT 2",
    "view_invalid_target"
)]
#[case::default(
    "ALTER VIEW public.item_view ALTER COLUMN absent SET DEFAULT 2",
    "view_unknown_default_column"
)]
#[case::replace(
    "CREATE OR REPLACE VIEW public.item_view AS SELECT id FROM public.items",
    "conflicting_creation"
)]
#[case::rename(
    "CREATE OR REPLACE VIEW public.item_view AS SELECT id AS renamed, value, value+1 AS computed FROM public.items",
    "conflicting_creation"
)]
#[case::type_change(
    "CREATE OR REPLACE VIEW public.item_view AS SELECT id::bigint AS key, value, value+1 AS computed FROM public.items",
    "conflicting_creation"
)]
#[case::duplicate_view(
    "CREATE VIEW public.item_view AS SELECT id FROM public.items",
    "conflicting_creation"
)]
#[case::alter("ALTER VIEW public.item_view DISABLE RULE r", "view_invalid_alter")]
fn invalid_view_declarations_are_structured(
    #[case] sql: &str,
    #[case] code: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(AUTOMATIC)?;
    let problems = schema.query_problems(&format!("{sql}; SELECT 1"))?;
    assert!(
        problems.iter().any(|problem| problem.code() == code
            && (problem.view_problem().is_some() || problem.creation_problem().is_some())),
        "{problems:?}"
    );
    assert!(
        schema
            .analyze("UPDATE public.item_view SET value = 9")
            .is_ok()
    );
    Ok(())
}

#[test]
fn incomplete_view_metadata_is_explicit() -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(AUTOMATIC)?;
    let older = schema.incomplete_catalog()?;
    let view = FullName::with_schema("public", "incomplete");
    assert!(
        ::sql_fun_sqlast::sem::ViewInsertInfoRead::view_insert_support(&older, &view).is_none()
    );
    assert!(
        ::sql_fun_sqlast::sem::ViewUpdateInfoRead::view_update_support(&older, &view).is_none()
    );
    for nested in ["incomplete", "incomplete_nested"] {
        let nested = FullName::with_schema("public", nested);
        for insert in [true, false] {
            let mut context = schema.analysis_context_at(&older);
            let unresolved = if insert {
                ::sql_fun_sqlast::sem::ViewWriteEvaluation::new(&mut context)
                    .insert(&nested)?
                    .is_none()
            } else {
                ::sql_fun_sqlast::sem::ViewWriteEvaluation::new(&mut context)
                    .update(&nested)?
                    .is_none()
            };
            assert!(unresolved);
            assert_eq!(context.problems().len(), 1);
            assert_eq!(
                context.problems()[0]
                    .view_problem()
                    .unwrap()
                    .missing_metadata_view()
                    .map(sql_fun_core::analysis_problem::FullName::as_str),
                Some("public.items")
            );
        }
    }
    for sql in [
        "INSERT INTO public.incomplete_nested VALUES (1)",
        "UPDATE public.incomplete_nested SET id = 2",
        "INSERT INTO public.incomplete VALUES (1)",
        "UPDATE public.incomplete SET id = 2",
    ] {
        let problems = schema.query_problems_at(&older, sql)?;
        assert!(
            problems
                .iter()
                .filter(|problem| problem.code() == "view_missing_write_metadata")
                .count()
                == 1,
            "{problems:?}"
        );
    }
    let problems = schema.query_problems_at(
        &older,
        "INSERT INTO public.incomplete VALUES (1) ON CONFLICT(id) DO NOTHING",
    )?;
    assert_eq!(
        problems
            .iter()
            .filter(|problem| problem.code() == "view_missing_write_metadata")
            .count(),
        1
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_trigger_namespace_and_column_case_match_engine() -> Result<(), Box<dyn std::error::Error>>
{
    let ddl = r"
CREATE TABLE items(a);
CREATE VIEW v1 AS SELECT a FROM items;
CREATE VIEW v2 AS SELECT a FROM items;
CREATE TRIGGER Tr INSTEAD OF UPDATE OF A ON v1 BEGIN SELECT 1; END;
CREATE TRIGGER IF NOT EXISTS tr INSTEAD OF INSERT ON v2 BEGIN SELECT 1; END;
";
    let db = ::rusqlite::Connection::open_in_memory()?;
    db.execute_batch(ddl)?;
    assert!(db.prepare("UPDATE v1 SET a = 1").is_ok());
    assert!(db.prepare("INSERT INTO v2 VALUES (1)").is_err());
    let schema = heavy::ViewSchema::sqlite(ddl)?;
    assert!(schema.analyze("UPDATE v1 SET a = 1").is_ok());
    assert!(schema.analyze("INSERT INTO v2 VALUES (1)").is_err());
    assert!(
        schema
            .analyze("CREATE TRIGGER tr INSTEAD OF INSERT ON v2 BEGIN SELECT 1; END;")
            .is_err()
    );
    Ok(())
}

#[test]
fn view_declaration_requires_a_selected_schema() -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(AUTOMATIC)?;
    let base = schema.without_search_path();
    let problems = schema.query_problems_at(
        &base,
        "CREATE VIEW unqualified AS SELECT id FROM public.items; SELECT 1",
    )?;
    assert!(
        problems
            .iter()
            .any(|problem| problem.code() == "view_invalid_target")
    );
    Ok(())
}

#[test]
fn select_rule_replacement_refreshes_view_facts_in_the_child()
-> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(
        "CREATE TABLE public.items(id integer); CREATE VIEW public.v AS SELECT id FROM public.items;",
    )?;
    let name = FullName::with_schema("public", "v");
    assert!(schema.analyze_insert(&name).is_writable());
    let child = schema.layer(r#"CREATE OR REPLACE RULE "_RETURN" AS ON SELECT TO public.v DO INSTEAD SELECT DISTINCT id FROM public.items"#)?;
    assert!(!schema.analyze_insert_at(&child, &name).is_writable());
    assert!(!schema.analyze_update_at(&child, &name).is_writable());
    assert!(schema.analyze_insert(&name).is_writable());
    Ok(())
}

#[test]
fn replacing_a_view_preserves_named_rule_links_and_refreshes_its_select_rule()
-> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(
        r#"
CREATE TABLE public.items(id integer);
CREATE VIEW public.v AS SELECT id FROM public.items;
CREATE RULE r AS ON INSERT TO public.v DO INSTEAD NOTHING;
CREATE OR REPLACE RULE "_RETURN" AS ON SELECT TO public.v DO INSTEAD SELECT id FROM public.items;
"#,
    )?;
    let name = FullName::with_schema("public", "v");
    let child =
        schema.layer("CREATE OR REPLACE VIEW public.v AS SELECT DISTINCT id FROM public.items")?;
    let view = child.get_view(&ViewName::from_full_name(&name)).unwrap();
    assert_eq!(view.rules().unwrap().resolve(&child).len(), 2);
    let identity = RuleIdentity::new(&name, "_RETURN");
    let rule = child.rule(&identity).unwrap();
    assert!(
        rule.actions()[0]
            .select_statement()
            .unwrap()
            .query_clauses()
            .unwrap()
            .body()
            .select_clauses()
            .unwrap()
            .distinct()
            .is_some()
    );
    assert!(
        schema.context().rule(&identity).unwrap().actions()[0]
            .select_statement()
            .unwrap()
            .query_clauses()
            .unwrap()
            .body()
            .select_clauses()
            .unwrap()
            .distinct()
            .is_none()
    );
    assert!(schema.analyze_insert_at(&child, &name).is_writable());
    assert!(!schema.analyze_update_at(&child, &name).is_writable());
    Ok(())
}

#[test]
fn evaluation_service_reads_definition_options_and_retained_defaults()
-> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(
        "CREATE TABLE public.items(id integer); CREATE VIEW public.v WITH (security_barrier = true) AS SELECT id FROM public.items WITH LOCAL CHECK OPTION; ALTER VIEW public.v ALTER COLUMN id SET DEFAULT 7;",
    )?;
    let name = FullName::with_schema("public", "v");
    let definition = schema
        .context()
        .get_view(&ViewName::from_full_name(&name))
        .unwrap()
        .ordinary_definition()
        .unwrap();
    assert_eq!(definition.options()[0].name(), "security_barrier");
    assert!(matches!(
        definition.check_option(),
        ::sql_fun_sqlast::sem::ViewCheckOption::Local
    ));
    let mut context = schema.analysis_context_at(schema.context());
    let mut service = ViewWriteEvaluation::new(&mut context);
    let result = service.evaluate(&name).unwrap().unwrap();
    assert!(result.insert().is_writable());
    assert!(result.update().is_writable());
    let replacement =
        schema.layer("CREATE OR REPLACE VIEW public.v AS SELECT id FROM public.items;")?;
    let support = ViewWriteEvaluation::new(&mut schema.analysis_context_at(&replacement))
        .insert(&name)
        .unwrap()
        .unwrap();
    let column = support.column_definition(&"id".into()).unwrap();
    let defaults: Vec<_> = column
        .constraints()
        .iter()
        .filter_map(|constraint| constraint.as_default())
        .collect();
    assert_eq!(defaults.len(), 1);
    assert_eq!(
        defaults[0].expr().as_const(),
        Some(&::sql_fun_sqlast::sem::ScalarConstExpr::new_integer(7))
    );
    let problems = schema.query_problems_at(&replacement, "INSERT INTO public.v DEFAULT VALUES")?;
    assert!(problems.is_empty(), "{problems:?}");
    Ok(())
}

#[test]
fn evaluation_excludes_masked_rule_references_and_uses_recreated_definitions()
-> Result<(), Box<dyn std::error::Error>> {
    use sql_fun_sqlast::context::BaseParseContext;

    let schema = heavy::ViewSchema::postgres(
        "CREATE TABLE public.items(id integer); CREATE VIEW public.v AS SELECT DISTINCT id FROM public.items; CREATE RULE r AS ON INSERT TO public.v DO INSTEAD NOTHING;",
    )?;
    let name = FullName::with_schema("public", "v");
    let identity = RuleIdentity::new(&name, "r");
    let child = schema.mask_rule(&identity)?;
    let references = child
        .get_view(&ViewName::from_full_name(&name))
        .unwrap()
        .rules()
        .unwrap();
    assert_eq!(references.identities().len(), 1);
    assert!(references.resolve(&child).is_empty());
    assert!(
        !ViewWriteEvaluation::new(&mut schema.analysis_context_at(&child))
            .insert(&name)
            .unwrap()
            .unwrap()
            .is_writable()
    );
    let recreated = schema.layer_at(
        &child,
        "CREATE RULE r AS ON INSERT TO public.v DO INSTEAD NOTHING;",
    )?;
    assert!(
        ViewWriteEvaluation::new(&mut schema.analysis_context_at(&recreated))
            .insert(&name)
            .unwrap()
            .unwrap()
            .is_writable()
    );
    assert_eq!(
        recreated
            .get_view(&::sql_fun_sqlast::sem::ViewName::from_full_name(&name))
            .unwrap()
            .rules()
            .unwrap()
            .resolve(&recreated)
            .len(),
        1
    );
    assert!(
        ViewWriteEvaluation::new(&mut schema.analysis_context_at(schema.context()))
            .insert(&name)
            .unwrap()
            .unwrap()
            .is_writable()
    );
    assert!(
        !ViewWriteEvaluation::new(&mut schema.analysis_context_at(&child))
            .insert(&name)
            .unwrap()
            .unwrap()
            .is_writable()
    );
    Ok(())
}

#[rstest::rstest]
#[case::rule("CREATE RULE r AS ON INSERT TO public.v DO INSTEAD NOTHING", "r")]
#[case::trigger(
    "CREATE FUNCTION public.noop() RETURNS trigger LANGUAGE plpgsql AS 'BEGIN RETURN NEW; END'; CREATE TRIGGER t INSTEAD OF INSERT ON public.v FOR EACH ROW EXECUTE FUNCTION public.noop()",
    "t"
)]
fn missing_linked_definition_is_incomplete_not_a_dropped_capability(
    #[case] declaration: &str,
    #[case] missing: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(&format!(
        "CREATE TABLE public.items(id integer); CREATE VIEW public.v AS SELECT DISTINCT id FROM public.items; {declaration};"
    ))?;
    let name = FullName::with_schema("public", "v");
    let detached = schema.detached_view_catalog(&name)?;
    let mut context = schema.analysis_context_at(&detached);
    assert!(
        ::sql_fun_sqlast::sem::ViewWriteEvaluation::new(&mut context)
            .insert(&name)?
            .is_none()
    );
    assert_eq!(context.problems().len(), 1);
    assert_eq!(
        context.problems()[0]
            .view_problem()
            .unwrap()
            .missing_metadata_view()
            .map(sql_fun_core::analysis_problem::FullName::as_str),
        Some(
            FullName::with_schema("public", missing)
                .to_string()
                .as_str()
        )
    );
    Ok(())
}

#[test]
fn evaluation_excludes_cancelled_trigger_links_and_accepts_recreation()
-> Result<(), Box<dyn std::error::Error>> {
    use ::sql_fun_sqlast::{
        context::TriggerInfoRead,
        sem::{TriggerIdentity, ViewWriteEvaluation},
    };
    let declaration = "CREATE TRIGGER t INSTEAD OF INSERT ON public.v FOR EACH ROW EXECUTE FUNCTION public.noop()";
    let schema = heavy::ViewSchema::postgres(&format!(
        "CREATE TABLE public.items(id integer); CREATE VIEW public.v AS SELECT DISTINCT id FROM public.items; CREATE FUNCTION public.noop() RETURNS trigger LANGUAGE plpgsql AS 'BEGIN RETURN NEW; END'; {declaration};"
    ))?;
    let name = FullName::with_schema("public", "v");
    let identity = TriggerIdentity::postgres(&name, &FullName::with_schema("public", "t"));
    let child = schema.mask_trigger(&identity)?;
    assert!(child.trigger_is_cancelled(&identity));
    assert!(
        !ViewWriteEvaluation::new(&mut schema.analysis_context_at(&child))
            .insert(&name)
            .unwrap()
            .unwrap()
            .is_writable()
    );
    let recreated = schema.layer_at(&child, declaration)?;
    assert!(!recreated.trigger_is_cancelled(&identity));
    assert!(
        ViewWriteEvaluation::new(&mut schema.analysis_context_at(&recreated))
            .insert(&name)
            .unwrap()
            .unwrap()
            .is_writable()
    );
    assert!(
        ViewWriteEvaluation::new(&mut schema.analysis_context_at(schema.context()))
            .insert(&name)
            .unwrap()
            .unwrap()
            .is_writable()
    );
    assert!(
        !ViewWriteEvaluation::new(&mut schema.analysis_context_at(&child))
            .insert(&name)
            .unwrap()
            .unwrap()
            .is_writable()
    );
    Ok(())
}

#[rstest::rstest]
#[case::insert_cte(
    "WITH rejected AS (INSERT INTO public.typo VALUES (1) RETURNING *) SELECT 1",
    "relation_not_found"
)]
#[case::update_cte(
    "WITH rejected AS (UPDATE public.typo SET id = 1 RETURNING *) SELECT 1",
    "relation_not_found"
)]
#[case::insert_typo("INSERT INTO public.typo VALUES (1)", "relation_not_found")]
#[case::update_typo("UPDATE public.typo SET id = 1", "relation_not_found")]
#[case::insert_read_only("INSERT INTO public.read_only VALUES (1)", "insert_semantics")]
#[case::update_read_only("UPDATE public.read_only SET id = 1", "update_semantics")]
#[case::insert_column(
    "INSERT INTO public.writable(typo) VALUES (1)",
    "column_not_found_in_table"
)]
#[case::update_column("UPDATE public.writable SET typo = 1", "column_not_found_in_table")]
#[case::create_rule(
    "CREATE RULE invalid AS ON INSERT TO public.typo DO NOTHING",
    "rule_invalid_definition"
)]
#[case::drop_rule("DROP RULE missing ON public.items", "rule_invalid_definition")]
#[case::create_trigger(
    "CREATE TRIGGER invalid BEFORE INSERT ON public.typo EXECUTE FUNCTION public.absent()",
    "trigger_invalid_definition"
)]
#[case::drop_trigger("DROP TRIGGER missing ON public.items", "trigger_invalid_definition")]
#[case::view_default(
    "ALTER VIEW public.writable ALTER COLUMN typo SET DEFAULT 1",
    "view_unknown_default_column"
)]
#[case::view_rule_state(
    "ALTER VIEW public.writable DISABLE RULE missing",
    "view_invalid_alter"
)]
fn semantic_problems_allow_subsequent_statements(
    #[case] invalid: &str,
    #[case] expected_code: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(
        "CREATE TABLE public.items(id integer); CREATE VIEW public.writable AS SELECT id FROM public.items; CREATE VIEW public.read_only AS SELECT DISTINCT id FROM public.items;",
    )?;
    let problems = schema.query_problems_at(schema.context(), &format!(
        "{invalid}; CREATE TABLE public.recovered(id integer); INSERT INTO public.recovered VALUES (1); UPDATE public.recovered SET id = 2; SELECT id FROM public.recovered;"
    ))?;
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].code(), expected_code);
    Ok(())
}

#[test]
fn rejected_default_changes_leave_the_view_unchanged() -> Result<(), Box<dyn std::error::Error>> {
    let schema = heavy::ViewSchema::postgres(
        "CREATE TABLE public.items(id integer); CREATE VIEW public.v AS SELECT id FROM public.items; ALTER VIEW public.v ALTER COLUMN id SET DEFAULT 7;",
    )?;
    let context = schema.analyze_statements_at(schema.context(),
        "ALTER VIEW public.v ALTER COLUMN id SET DEFAULT 9, ALTER COLUMN typo SET DEFAULT 1; INSERT INTO public.items VALUES (1);"
    )?;
    assert_eq!(context.problems().len(), 1);
    assert_eq!(context.problems()[0].code(), "view_unknown_default_column");
    let support =
        schema.analyze_insert_at(&context.freeze(), &FullName::with_schema("public", "v"));
    let column = support.column_definition(&"id".into()).unwrap();
    let default = column
        .constraints()
        .iter()
        .find_map(|constraint| constraint.as_default())
        .unwrap();
    assert_eq!(
        default.expr().as_const(),
        Some(&::sql_fun_sqlast::sem::ScalarConstExpr::new_integer(7))
    );
    Ok(())
}
