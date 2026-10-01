#[get]
#[allow(unknown_lints, clippy::unused_async, clippy::unused_async_trait_impl)]
#[route_id("files.read")]
#[middleware(local)]
async fn read(&self, Path(path): Path<ItemPath>, _observed: Observed) -> Result<Json<String>, Problem> {
    self.events.borrow_mut().push("handler");
    Ok(Json(path.id))
}
