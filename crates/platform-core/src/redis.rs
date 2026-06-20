use crate::config::RedisConfig;
use crate::error::{AppError, AppResult, ErrorCode};

pub type RedisConnection = ::redis::aio::ConnectionManager;

pub async fn connect_redis(config: &RedisConfig) -> AppResult<Option<RedisConnection>> {
    let Some(url) = config.url.as_deref() else {
        return Ok(None);
    };

    let client = ::redis::Client::open(url).map_err(map_redis_error)?;
    let connection = ::redis::aio::ConnectionManager::new(client)
        .await
        .map_err(map_redis_error)?;
    Ok(Some(connection))
}

fn map_redis_error(source: ::redis::RedisError) -> AppError {
    AppError::new(ErrorCode::ExternalDependency, "Redis connection failed")
        .with_source(source)
        .retryable()
}
