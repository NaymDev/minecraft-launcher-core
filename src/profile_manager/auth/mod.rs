use std::{ fmt::Debug, collections::HashMap };

use reqwest::Client;
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
#[error("{0}")]
pub struct MinecraftAuthenticationError(String);

pub trait UserAuthentication: Send + Debug {
  fn get_authenticated_token(&self) -> String;
  // TODO: fn get_user_properties(&self);
  // TODO: fn get_user_property_map(&self);
  fn get_auth_session(&self) -> String; // auth token again, "-" if it is offline mode
  fn auth_player_name(&self) -> String;
  fn auth_uuid(&self) -> Uuid;
  fn user_type(&self) -> String; // "legacy" - "mojang" - "msa"
  fn get_extra_substitutors(&self) -> HashMap<String, String>;
  // TODO: only on msa auth, or figure out how!
  // fn auth_xuid(&self) -> Option<String>;
}

// TODO: add an msa auth type, that in extra substitutors adds xuid if present

#[derive(Debug, Clone)]
pub struct CommonUserAuthentication {
  access_token: String,
  auth_playername: String,
  auth_uuid: Uuid,
  user_type: String,
}

impl CommonUserAuthentication {
  pub async fn from_minecraft_token(mc_token: &str) -> Result<Self, Box<dyn std::error::Error>> {
    // Get player profile
    let profile_res = Client::new()
      .get("https://api.minecraftservices.com/minecraft/profile")
      .bearer_auth(mc_token)
      .send().await?
      .error_for_status()?
      .json::<Value>().await?;

    if let Some(error) = profile_res.get("error") {
      return Err(
        Box::new(MinecraftAuthenticationError(format!("An error ocurred while getting player profile {}", error.as_str().unwrap().to_string())))
      );
    }

    Ok(Self {
      access_token: mc_token.to_string(),
      auth_playername: profile_res["name"].as_str().unwrap().to_string(),
      auth_uuid: Uuid::parse_str(profile_res["id"].as_str().unwrap())?,
      user_type: "msa".to_string(), // The only one allowed atm
    })
  }
}

impl UserAuthentication for CommonUserAuthentication {
  fn get_authenticated_token(&self) -> String {
    self.access_token.clone()
  }

  fn get_auth_session(&self) -> String {
    self.access_token.clone()
  }

  fn auth_player_name(&self) -> String {
    self.auth_playername.clone()
  }

  fn auth_uuid(&self) -> Uuid {
    self.auth_uuid.clone()
  }

  fn user_type(&self) -> String {
    self.user_type.clone()
  }

  fn get_extra_substitutors(&self) -> HashMap<String, String> {
    HashMap::new()
  }
}

#[derive(Debug, Clone)]
pub struct OfflineUserAuthentication {
  pub username: String,
  pub uuid: Uuid,
}

impl UserAuthentication for OfflineUserAuthentication {
  fn get_authenticated_token(&self) -> String {
    String::new()
  }

  fn get_auth_session(&self) -> String {
    "-".to_string()
  }

  fn auth_player_name(&self) -> String {
    self.username.clone()
  }

  fn auth_uuid(&self) -> Uuid {
    self.uuid.clone()
  }

  fn user_type(&self) -> String {
    "legacy".to_string()
  }

  fn get_extra_substitutors(&self) -> HashMap<String, String> {
    HashMap::new()
  }
}
