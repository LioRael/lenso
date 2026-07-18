use platform_core::Migration;

pub const STORY_MIGRATIONS: &[Migration] = &[Migration {
    name: "story/0001_aggregate_federated_runtime_stories",
    sql: include_str!("../migrations/0001_aggregate_federated_runtime_stories.sql"),
}];
