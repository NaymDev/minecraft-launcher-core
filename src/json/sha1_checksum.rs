use std::{ fmt::{ Debug, Display }, io::Read };

use serde::{ Deserialize, Serialize };
use sha1::{ Digest, Sha1 };

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct Sha1Sum([u8; 20]);

impl Sha1Sum {
  pub fn new(value: [u8; 20]) -> Self {
    Self(value)
  }

  pub fn from_reader<T: Read>(value: &mut T) -> Result<Self, Box<dyn std::error::Error>> {
    let mut sha1_hasher = Sha1::new();
    let mut buf = vec![];
    value.read_to_end(&mut buf)?;
    sha1_hasher.update(&buf);
    Ok(Sha1Sum(sha1_hasher.finalize().into()))
  }
}

impl TryFrom<String> for Sha1Sum {
  type Error = String;
  fn try_from(value: String) -> Result<Self, Self::Error> {
    let mut buf = [0u8; 20];
    hex::decode_to_slice(value, &mut buf).map_err(|e| e.to_string())?;
    Ok(Sha1Sum(buf))
  }
}

impl Into<String> for Sha1Sum {
  fn into(self) -> String {
    hex::encode(self.0)
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
