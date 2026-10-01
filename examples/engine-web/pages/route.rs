#[get]
#[allow(unknown_lints, clippy::unused_async, clippy::unused_async_trait_impl)]
async fn read(&self) -> Result<Json<String>, Problem> {
    Ok(Json("home".into()))
}
