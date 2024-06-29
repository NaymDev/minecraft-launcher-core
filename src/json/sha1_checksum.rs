use std::{ fmt::{ Debug, Display }, io::{ copy, Read, Error as IoError } };

use hex::FromHexError;
use serde::{ Deserialize, Serialize };
use sha1::{ Digest, Sha1 };

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct Sha1Sum([u8; 20]);

impl Sha1Sum {
  pub fn new(value: [u8; 20]) -> Self {
    Self(value)
  }

  pub fn as_slice(&self) -> &[u8] {
    &self.0
  }

  pub fn from_reader<T: Read>(value: &mut T) -> Result<Self, IoError> {
    let mut sha1_hasher = Sha1::new();
    copy(value, &mut sha1_hasher)?;
    Ok(sha1_hasher.into())
  }

  pub fn null() -> Self {
    Self([0u8; 20])
  }
}

impl From<Sha1> for Sha1Sum {
  fn from(value: Sha1) -> Self {
    Self(value.finalize().into())
  }
}

impl From<Sha1Sum> for Vec<u8> {
  fn from(val: Sha1Sum) -> Self {
    val.0.to_vec()
  }
}

impl From<Sha1Sum> for [u8; 20] {
  fn from(val: Sha1Sum) -> Self {
    val.0
  }
}

impl TryFrom<String> for Sha1Sum {
  type Error = FromHexError;
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
