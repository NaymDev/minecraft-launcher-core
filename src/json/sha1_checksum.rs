use std::{ fmt::{ Debug, Display }, io::Read };

use thiserror::Error;
use serde::{ Deserialize, Serialize };
use sha1::{ Digest, Sha1 };

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct Sha1Sum([u8; 20]);

impl Sha1Sum {
  pub fn new(value: [u8; 20]) -> Self {
    Self(value)
  }

  pub fn from_reader<T: Read>(value: &mut T) -> Result<Self, Sha1SumError> {
    let mut sha1_hasher = Sha1::new();
    std::io::copy(value, &mut sha1_hasher)?;
    Ok(Sha1Sum(sha1_hasher.finalize().into()))
  }

  pub fn null() -> Self {
    Self([0u8; 20])
  }
}

impl TryFrom<String> for Sha1Sum {
  type Error = Sha1SumError;
  fn try_from(value: String) -> Result<Self, Self::Error> {
    let mut buf = [0u8; 20];
    hex::decode_to_slice(value, &mut buf)?;
    Ok(Sha1Sum(buf))
  }
}

impl From<Sha1Sum> for String {
  fn from(val: Sha1Sum) -> Self {
    hex::encode(val.0)
  }
}

impl Debug for Sha1Sum {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", hex::encode(self.0))
  }
}

impl Display for Sha1Sum {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", hex::encode(self.0))
  }
}

#[derive(Error, Debug)]
pub enum Sha1SumError {
  #[error(transparent)] HexError(#[from] hex::FromHexError),
  #[error(transparent)] IoError(#[from] std::io::Error),
}
