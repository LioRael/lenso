use platform_core::Migration;

pub const NOTIFICATIONS_MIGRATIONS: &[Migration] = &[Migration {
    name: "notifications/0001_create_notifications_schema",
    sql: include_str!("../migrations/0001_create_notifications_schema.sql"),
}];
