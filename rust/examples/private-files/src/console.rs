use lenso_app::{
    Action, ActionError, Catalog, ConsoleExtension, ConsoleManifest, Context, Module, require_scope,
};
use serde_json::Value;

use crate::{
    actions,
    store::{FileInfo, Store, User},
};

pub struct Users;
pub struct Files;
pub struct Requests;
pub struct Manifest;

impl Action<Store> for Users {
    type Input = ();
    type Output = Vec<User>;
    fn id(&self) -> &str {
        "console.users"
    }
    async fn authorize(&self, context: &Context<Store>, _: &()) -> Result<(), ActionError> {
        require_scope(context.principal.as_ref(), "console:read")
    }
    async fn run(&self, context: &Context<Store>, _: ()) -> Result<Self::Output, ActionError> {
        context.resources.users()
    }
}
impl Action<Store> for Files {
    type Input = ();
    type Output = Vec<FileInfo>;
    fn id(&self) -> &str {
        "console.files"
    }
    async fn authorize(&self, context: &Context<Store>, _: &()) -> Result<(), ActionError> {
        require_scope(context.principal.as_ref(), "console:read")
    }
    async fn run(&self, context: &Context<Store>, _: ()) -> Result<Self::Output, ActionError> {
        context.resources.files()
    }
}
impl Action<Store> for Requests {
    type Input = ();
    type Output = Vec<Value>;
    fn id(&self) -> &str {
        "console.requests"
    }
    async fn authorize(&self, context: &Context<Store>, _: &()) -> Result<(), ActionError> {
        require_scope(context.principal.as_ref(), "console:read")
    }
    async fn run(&self, context: &Context<Store>, _: ()) -> Result<Self::Output, ActionError> {
        context.resources.requests()
    }
}
pub fn extension() -> Result<ConsoleExtension<Store>, ActionError> {
    let mut extension = ConsoleExtension::new();
    extension
        .page("users", "Users", Users)?
        .page("files", "Files", Files)?
        .page("requests", "Requests", Requests)?;
    Ok(extension)
}

pub fn catalog(extension: &ConsoleExtension<Store>) -> Result<Catalog, ActionError> {
    let mut auth = Module::new("auth");
    auth.action(&actions::Register)
        .action(&actions::Login)
        .resource("sqlite");
    let mut files = Module::new("files");
    files
        .action(&actions::Upload)
        .action(&actions::Read)
        .action(&actions::List)
        .resource("sqlite");
    let mut console = extension.module("console");
    console.action(&Manifest).resource("sqlite");
    let mut catalog = Catalog::new();
    for module in [auth, files, console] {
        catalog.register(module)?;
    }
    catalog.require_resources(["sqlite"])?;
    Ok(catalog)
}

impl Action<Store> for Manifest {
    type Input = ();
    type Output = ConsoleManifest;
    fn id(&self) -> &str {
        "console.manifest"
    }
    async fn authorize(&self, context: &Context<Store>, _: &()) -> Result<(), ActionError> {
        require_scope(context.principal.as_ref(), "console:read")
    }
    async fn run(&self, _: &Context<Store>, _: ()) -> Result<Self::Output, ActionError> {
        let extension = extension()?;
        extension.manifest(&catalog(&extension)?)
    }
}
