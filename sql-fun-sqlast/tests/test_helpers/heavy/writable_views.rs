use crate::test_helpers::create_cache_context;

use ::clap::Parser;
use ::sql_fun_core::{SqlFunArgs, SqlFunMetadata};
use ::sql_fun_sqlast::{
    ParsedSchemaDump, PostgresDialect,
    context::{BaseContext, FrozenBaseContext, ParseContext, SchemaFileContext},
    sem::SemAst,
};

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
                PostgresDialect::parse_schema_file(file.path(), &base, &mut None, &args, &selected)?
            }
            #[cfg(not(feature = "sqlite"))]
            {
                PostgresDialect::parse_schema_file(file.path(), &base, &mut None, &args, &selected)?
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
        let mut cache_context = create_cache_context();
        let mut context =
            SchemaFileContext::new(Path::new("layer.sql"), base, &self.args, &self.metadata);
        for statement in ::sql_fun_sqlast::parse_statements(
            sql,
            ::sql_fun_sqlast::ast::Dialect::from(&self.metadata),
        ) {
            context = SemAst::from_ast(
                &statement.map_err(|error| error.to_string())?,
                context,
                &mut cache_context,
            )
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
        name: &::sql_fun_sqlast::names::FullName,
    ) -> Result<FrozenBaseContext, String> {
        use ::sql_fun_sqlast::context::BaseParseContext;
        use ::sql_fun_sqlast::context::ViewInfoWrite;

        let view = self
            .context()
            .get_view(&::sql_fun_sqlast::names::ViewName::from_full_name(name))
            .unwrap()
            .ordinary_definition()
            .unwrap();
        let base = FrozenBaseContext::from(
            BaseContext::new(&self.args, &self.metadata).map_err(|error| error.to_string())?,
        );
        SchemaFileContext::new(Path::new("detached.sql"), &base, &self.args, &self.metadata)
            .apply_create_view(view)
            .map(ParseContext::freeze)
            .map_err(|error| error.to_string())
    }
    pub fn incomplete_catalog(&self) -> Result<FrozenBaseContext, String> {
        use ::sql_fun_sqlast::context::ViewInfoWrite;
        let mut cache_context = create_cache_context();

        let SemAst::CreateView(view) =
            self.analyze("CREATE VIEW public.incomplete AS SELECT id FROM public.items")?
        else {
            panic!("expected view")
        };
        let context = SchemaFileContext::new(
            Path::new("older.sql"),
            &FrozenBaseContext::from(
                BaseContext::new(&self.args, &self.metadata).map_err(|error| error.to_string())?,
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
        Ok(SemAst::from_ast(&statement, context, &mut cache_context)
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
        let mut cache_context = create_cache_context();
        let mut context =
            SchemaFileContext::new(Path::new("query.sql"), base, &self.args, &self.metadata);
        for statement in ::sql_fun_sqlast::parse_statements(
            sql,
            ::sql_fun_sqlast::ast::Dialect::from(&self.metadata),
        ) {
            let statement = statement.map_err(|error| error.to_string())?;
            context.set_statement_span(*statement.span());
            context = SemAst::from_ast(&statement, context, &mut cache_context)
                .map_err(|error| error.to_string())?
                .into_context();
        }
        Ok(context)
    }
    pub fn analyze(&self, sql: &str) -> Result<SemAst, String> {
        let mut cache_context = create_cache_context();
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
        let result = SemAst::from_ast(&statement, context, &mut cache_context)
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
        view: &::sql_fun_sqlast::names::FullName,
    ) -> ::sql_fun_sqlast::sem::ViewInsertSupport {
        ::sql_fun_sqlast::sem::ViewWriteEvaluation::new(&mut self.analysis_context_at(base))
            .insert(view)
            .expect("analysis engine succeeds")
            .expect("complete view write facts")
    }
    pub fn analyze_update_at(
        &self,
        base: &FrozenBaseContext,
        view: &::sql_fun_sqlast::names::FullName,
    ) -> ::sql_fun_sqlast::sem::ViewUpdateSupport {
        ::sql_fun_sqlast::sem::ViewWriteEvaluation::new(&mut self.analysis_context_at(base))
            .update(view)
            .expect("analysis engine succeeds")
            .expect("complete view write facts")
    }
    pub fn analyze_insert(
        &self,
        view: &::sql_fun_sqlast::names::FullName,
    ) -> ::sql_fun_sqlast::sem::ViewInsertSupport {
        self.analyze_insert_at(self.context(), view)
    }
    pub fn analyze_update(
        &self,
        view: &::sql_fun_sqlast::names::FullName,
    ) -> ::sql_fun_sqlast::sem::ViewUpdateSupport {
        self.analyze_update_at(self.context(), view)
    }
}

/// Read emitted `pg_settings` and load its full production builtin catalog.
/// Each call loads the selected version independently, without caching.
pub fn builtin_settings(version: &str) -> Result<ViewSchema, Box<dyn std::error::Error>> {
    let path =
        std::path::Path::new(env!("SQL_FUN_HOME")).join(format!("postgres/{version}/schema.sql"));
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
