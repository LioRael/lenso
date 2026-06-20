use platform_core::{RedisConfig, connect_redis};

#[tokio::test]
async fn disabled_redis_config_does_not_connect() {
    let redis = connect_redis(&RedisConfig { url: None })
        .await
        .expect("disabled redis should not fail");

    assert!(redis.is_none());
}
