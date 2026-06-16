use platform_core::Migration;

pub const AUTH_PASSWORD_MIGRATIONS: &[Migration] = &[
    Migration {
        name: "auth-password/0001_create_auth_password_schema",
        sql: include_str!("../migrations/0001_create_auth_password_schema.sql"),
    },
    Migration {
        name: "auth-password/0002_create_password_credentials",
        sql: include_str!("../migrations/0002_create_password_credentials.sql"),
    },
];
