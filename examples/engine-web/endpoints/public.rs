#[get("health", "/health")]
#[allow(unknown_lints, clippy::unused_async)]
async fn health(&self) -> Result<Json<String>, Problem> {
    Ok(Json("ok".into()))
}

#[get("item", "/items/{id}")]
#[allow(unknown_lints, clippy::unused_async)]
async fn item(&self, Path(path): Path<ItemPath>) -> Result<Json<String>, Problem> {
    Ok(Json(path.id))
}
