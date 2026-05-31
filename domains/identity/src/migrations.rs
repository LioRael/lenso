use platform_core::Migration;

pub const IDENTITY_MIGRATIONS: &[Migration] = &[
    Migration {
        name: "identity/0001_create_identity_schema",
        sql: include_str!("../migrations/0001_create_identity_schema.sql"),
    },
    Migration {
        name: "identity/0002_create_users",
        sql: include_str!("../migrations/0002_create_users.sql"),
    },
];
