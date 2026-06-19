use platform_core::Migration;

pub const AUTH_MIGRATIONS: &[Migration] = &[
    Migration {
        name: "auth/0001_create_auth_schema",
        sql: include_str!("../migrations/0001_create_auth_schema.sql"),
    },
    Migration {
        name: "auth/0002_create_auth_core_tables",
        sql: include_str!("../migrations/0002_create_auth_core_tables.sql"),
    },
    Migration {
        name: "auth/0003_add_user_disable_details",
        sql: include_str!("../migrations/0003_add_user_disable_details.sql"),
    },
];
