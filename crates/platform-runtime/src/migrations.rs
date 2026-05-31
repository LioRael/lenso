use platform_core::Migration;

pub const RUNTIME_MIGRATIONS: &[Migration] = &[Migration {
    name: "runtime/0001_create_runtime_schema",
    sql: include_str!("../migrations/0001_create_runtime_schema.sql"),
}];
