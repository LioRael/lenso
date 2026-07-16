use platform_core::Migration;

pub const RUNTIME_MIGRATIONS: &[Migration] = &[
    Migration {
        name: "runtime/0001_create_runtime_schema",
        sql: include_str!("../migrations/0001_create_runtime_schema.sql"),
    },
    Migration {
        name: "runtime/0002_create_function_runs",
        sql: include_str!("../migrations/0002_create_function_runs.sql"),
    },
    Migration {
        name: "runtime/0003_add_function_runs_summary_index",
        sql: include_str!("../migrations/0003_add_function_runs_summary_index.sql"),
    },
    Migration {
        name: "runtime/0004_add_function_runs_story_indexes",
        sql: include_str!("../migrations/0004_add_function_runs_story_indexes.sql"),
    },
    Migration {
        name: "runtime/0005_create_scheduled_functions",
        sql: include_str!("../migrations/0005_create_scheduled_functions.sql"),
    },
    Migration {
        name: "runtime/0006_preserve_function_tenant_context",
        sql: include_str!("../migrations/0006_preserve_function_tenant_context.sql"),
    },
];
