use platform_core::{DatabaseConfig, DbPool, IdGenerator};
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct SequentialIdGenerator {
    next: Mutex<u64>,
}

#[allow(dead_code)]
impl IdGenerator for SequentialIdGenerator {
    fn new_id(&self, prefix: &str) -> String {
        let mut next = self.next.lock().expect("id generator lock poisoned");
        *next += 1;
        format!("{prefix}_{next}")
    }
}

#[derive(Debug)]
pub struct TestDatabase {
    admin_url: String,
    name: String,
    pub pool: DbPool,
}

impl TestDatabase {
    pub async fn create() -> Option<Self> {
        let admin_url = match std::env::var("DATABASE_URL") {
            Ok(url) => url,
            Err(_) => {
                eprintln!("skipping Postgres integration test: DATABASE_URL is not set");
                return None;
            }
        };

        let name = format!("lenso_test_{}", Uuid::now_v7().simple());
        let admin_pool = match sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&admin_url)
            .await
        {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!(
                    "skipping Postgres integration test: could not connect to DATABASE_URL: {error}"
                );
                return None;
            }
        };

        let create_sql = format!(r#"create database "{name}""#);
        if let Err(error) = sqlx::query(sqlx::AssertSqlSafe(create_sql))
            .execute(&admin_pool)
            .await
        {
            eprintln!(
                "skipping Postgres integration test: could not create test database: {error}"
            );
            return None;
        }
        admin_pool.close().await;

        let url = database_url_with_name(&admin_url, &name);
        let pool = match platform_core::connect_pool(&DatabaseConfig {
            url,
            max_connections: 5,
        })
        .await
        {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!(
                    "skipping Postgres integration test: could not connect to test database: {error}"
                );
                return None;
            }
        };

        Some(Self {
            admin_url,
            name,
            pool,
        })
    }

    pub async fn cleanup(self) {
        self.pool.close().await;
        if let Ok(admin_pool) = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&self.admin_url)
            .await
        {
            let terminate_sql = r#"
                select pg_terminate_backend(pid)
                from pg_stat_activity
                where datname = $1
            "#;
            let _ = sqlx::query(terminate_sql)
                .bind(&self.name)
                .execute(&admin_pool)
                .await;

            let drop_sql = format!(r#"drop database if exists "{}""#, self.name);
            let _ = sqlx::query(sqlx::AssertSqlSafe(drop_sql))
                .execute(&admin_pool)
                .await;
            admin_pool.close().await;
        }
    }
}

fn database_url_with_name(url: &str, name: &str) -> String {
    let without_query = url.split_once('?').map_or(url, |(base, _)| base);
    let slash = without_query
        .rfind('/')
        .expect("DATABASE_URL must include a database name");
    format!("{}/{}", &without_query[..slash], name)
}
