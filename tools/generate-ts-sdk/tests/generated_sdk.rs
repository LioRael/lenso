#[test]
fn generated_types_include_openapi_models() {
    let source =
        generate_ts_sdk::generated_types_source().expect("generated types source should render");

    assert!(source.contains("export type CreateUserRequest"));
    assert!(source.contains("export type CreateUserResponse"));
    assert!(source.contains("export type AdminRuntimeStoryDetail"));
    assert!(source.contains("export type AdminRuntimeStoryListItem"));
    assert!(source.contains("export type AdminRuntimeHeatmapResponse"));
    assert!(source.contains("export type AdminRuntimeTimelineItem"));
    assert!(source.contains("export type AdminOutboxListResponse"));
    assert!(source.contains("export type AdminFunctionRunListResponse"));
    assert!(source.contains("export type ErrorResponse"));
}

#[test]
fn committed_generated_files_are_fresh() {
    let committed_types = include_str!("../../../packages/ts-sdk/src/generated/types.ts");
    let committed_client = include_str!("../../../packages/ts-sdk/src/generated/client.ts");

    assert_eq!(
        committed_types,
        generate_ts_sdk::generated_types_source().expect("types should render")
    );
    assert_eq!(
        committed_client,
        generate_ts_sdk::generated_client_source().expect("client should render")
    );
}
