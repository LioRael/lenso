use lenso_app::{
    Action, ActionError, Context, ErrorCode, require_owner, require_principal, require_scope,
};
use scrypt::{Params, scrypt};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

use crate::store::{File, FileInfo, Store, User, hex, internal, random_hex};

#[derive(Debug, Deserialize)]
pub struct Credentials {
    pub name: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct Session {
    pub token: String,
}

pub struct Register;
pub struct Login;
pub struct Upload;
pub struct Read;
pub struct List;

fn validate_credentials(input: &Credentials) -> Result<(), ActionError> {
    if !(3..=64).contains(&input.name.len())
        || !input
            .name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        || !(12..=128).contains(&input.password.len())
    {
        return Err(ActionError::new(
            ErrorCode::InvalidInput,
            "Use a 3–64 character username and 12–128 byte password",
        ));
    }
    Ok(())
}

async fn password_hash(password: String, salt: String) -> Result<String, ActionError> {
    tokio::task::spawn_blocking(move || {
        let mut key = [0; 64];
        let params = Params::new(14, 8, 1, 64).map_err(internal)?;
        scrypt(password.as_bytes(), salt.as_bytes(), &params, &mut key).map_err(internal)?;
        Ok(hex(&key))
    })
    .await
    .map_err(internal)?
}

impl Action<Store> for Register {
    type Input = Credentials;
    type Output = User;
    fn id(&self) -> &str {
        "auth.register"
    }
    fn validate_input(&self, input: &Credentials) -> Result<(), ActionError> {
        validate_credentials(input)
    }
    async fn authorize(&self, _: &Context<Store>, _: &Credentials) -> Result<(), ActionError> {
        Ok(())
    }
    async fn run(&self, context: &Context<Store>, input: Credentials) -> Result<User, ActionError> {
        let salt = random_hex(16)?;
        let hash = password_hash(input.password, salt.clone()).await?;
        context.resources.add_user(input.name, salt, hash)
    }
}

impl Action<Store> for Login {
    type Input = Credentials;
    type Output = Session;
    fn id(&self) -> &str {
        "auth.login"
    }
    fn validate_input(&self, input: &Credentials) -> Result<(), ActionError> {
        validate_credentials(input)
    }
    async fn authorize(&self, _: &Context<Store>, _: &Credentials) -> Result<(), ActionError> {
        Ok(())
    }
    async fn run(
        &self,
        context: &Context<Store>,
        input: Credentials,
    ) -> Result<Session, ActionError> {
        let credentials = context.resources.credentials(&input.name)?;
        let salt = credentials
            .as_ref()
            .map(|value| value.salt.clone())
            .unwrap_or_else(|| "missing-user-salt".to_owned());
        let hash = password_hash(input.password, salt).await?;
        let Some(credentials) = credentials else {
            return Err(ActionError::new(
                ErrorCode::Unauthenticated,
                "Invalid credentials",
            ));
        };
        if !bool::from(hash.as_bytes().ct_eq(credentials.hash.as_bytes())) {
            return Err(ActionError::new(
                ErrorCode::Unauthenticated,
                "Invalid credentials",
            ));
        }
        Ok(Session {
            token: context.resources.session(&credentials.id)?,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct UploadInput {
    pub name: String,
    #[serde(deserialize_with = "decode_bytes")]
    pub content: Vec<u8>,
}

fn decode_bytes<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    use base64::Engine;
    let encoded = String::deserialize(deserializer)?;
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(serde::de::Error::custom)
}

impl Action<Store> for Upload {
    type Input = UploadInput;
    type Output = FileInfo;
    fn id(&self) -> &str {
        "files.upload"
    }
    fn validate_input(&self, input: &UploadInput) -> Result<(), ActionError> {
        if input.name.is_empty()
            || input.name.encode_utf16().count() > 255
            || input.content.len() > 1_048_576
        {
            return Err(ActionError::new(
                ErrorCode::InvalidInput,
                "Invalid filename or file exceeds 1 MiB",
            ));
        }
        Ok(())
    }
    async fn authorize(
        &self,
        context: &Context<Store>,
        _: &UploadInput,
    ) -> Result<(), ActionError> {
        require_scope(context.principal.as_ref(), "files:write")
    }
    async fn run(
        &self,
        context: &Context<Store>,
        input: UploadInput,
    ) -> Result<FileInfo, ActionError> {
        let owner = &require_principal(context.principal.as_ref())?.subject;
        context.resources.upload(owner, input.name, input.content)
    }
}

#[derive(Debug, Deserialize)]
pub struct ReadInput {
    pub id: String,
}

impl Action<Store> for Read {
    type Input = ReadInput;
    type Output = File;
    fn id(&self) -> &str {
        "files.read"
    }
    async fn authorize(
        &self,
        context: &Context<Store>,
        input: &ReadInput,
    ) -> Result<(), ActionError> {
        require_scope(context.principal.as_ref(), "files:read")?;
        require_owner(
            context.principal.as_ref(),
            &context.resources.read(&input.id)?.info.owner,
        )
    }
    async fn run(&self, context: &Context<Store>, input: ReadInput) -> Result<File, ActionError> {
        context.resources.read(&input.id)
    }
}

impl Action<Store> for List {
    type Input = ();
    type Output = Vec<FileInfo>;
    fn id(&self) -> &str {
        "files.list"
    }
    async fn authorize(&self, context: &Context<Store>, _: &()) -> Result<(), ActionError> {
        require_scope(context.principal.as_ref(), "files:read")
    }
    async fn run(&self, context: &Context<Store>, _: ()) -> Result<Vec<FileInfo>, ActionError> {
        context
            .resources
            .list(&require_principal(context.principal.as_ref())?.subject)
    }
}
