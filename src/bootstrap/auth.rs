use base64::{ engine::general_purpose::URL_SAFE, Engine };
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;
use reqwest::Client;
use uuid::Uuid;

const PROFILE_URL: &str = "https://sessionserver.mojang.com/session/minecraft/profile/";

#[derive(Debug, Clone)]
pub struct UserAuthentication {
  pub username: String,
  pub uuid: Uuid,
  pub access_token: Option<String>,
}

impl UserAuthentication {
  pub fn offline(username: &str) -> Self {
    let uuid = Uuid::new_v3(&Uuid::NAMESPACE_DNS, format!("OfflinePlayer:{}", username).as_bytes());
    Self {
      username: username.to_string(),
      uuid,
      access_token: None,
    }
  }

  pub async fn online(access_token: &str) -> Result<Self, UserAuthenticationError> {
    let profile: ProfileResponse = Client::new().get(PROFILE_URL).bearer_auth(access_token).send().await?.error_for_status()?.json().await?;

    Ok(Self {
      access_token: Some(access_token.to_string()),
      username: profile.name,
      uuid: Uuid::parse_str(&profile.id)?,
    })
  }

  pub fn access_token(&self) -> &str {
    self.access_token.as_deref().unwrap_or("")
  }

  pub fn auth_session(&self) -> &str {
    self.access_token.as_deref().unwrap_or("-")
  }

  pub fn xuid(&self) -> Option<String> {
    let token = self.access_token.as_deref()?;
    if token.is_empty() {
      return None;
    }
    let parts: Vec<&str> = token.split('.').collect();
    let decoded = URL_SAFE.decode(parts.get(1)?).ok()?;
    let json: Value = serde_json::from_slice(&decoded).ok()?;
    let xuid = json["xuid"].as_str()?;
    Some(xuid.to_string())
  }

  pub fn user_type(&self) -> &str {
    if self.access_token.is_some() {
      "msa" // or "mojang"
    } else {
      "legacy"
    }
  }
}

#[derive(Debug, Error)]
pub enum UserAuthenticationError {
  #[error(transparent)] ReqwestError(#[from] reqwest::Error),
  #[error("{0}")] AuthenticationError(String),
  #[error(transparent)] JsonError(#[from] serde_json::Error),
  #[error(transparent)] UuidError(#[from] uuid::Error),
}

#[derive(Deserialize, Debug)]
struct ProfileResponse {
  name: String,
  id: String,
}
