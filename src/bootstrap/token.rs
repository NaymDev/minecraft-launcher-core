use base64::{engine::general_purpose, Engine as _};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub(crate) struct JwtPayload {
    pub(crate) profiles: Profiles,
    pub(crate) pfd: Vec<Pfd>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Profiles {
    pub(crate) mc: String,
}

#[derive(Debug, Deserialize)]
pub struct Pfd {
    #[serde(rename = "type")]
    pub(crate) profile_type: String,
    pub(crate) id: String,
    pub(crate) name: String,
}

