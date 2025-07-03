use chrono::{ DateTime, FixedOffset };
use serde::{ Serializer, Deserializer, Serialize, Deserialize };

#[derive(Debug, Clone, PartialEq)]
pub struct Date {
  date: DateTime<FixedOffset>,
}

impl Date {
  pub fn inner(&self) -> &DateTime<FixedOffset> {
    &self.date
  }
}

impl From<Date> for DateTime<FixedOffset> {
  fn from(val: Date) -> Self {
    val.date
  }
}

impl From<DateTime<FixedOffset>> for Date {
  fn from(date: DateTime<FixedOffset>) -> Self {
    Date { date }
  }
}

impl Serialize for Date {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
    serializer.serialize_str(&self.date.to_rfc3339())
  }
}

impl<'de> Deserialize<'de> for Date {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: Deserializer<'de> {
    let mut date = String::deserialize(deserializer)?;

    // Insert colon before the last two digits of the offset if missing
    if date.len() > 5 {
      let tz_part = &date[date.len()-5..];
      if tz_part.chars().nth(3) != Some(':') {
        // Example: "+0000" -> "+00:00"
        date.insert(date.len() - 2, ':');
      }
    }

    let date = DateTime::parse_from_rfc3339(&date).map_err(serde::de::Error::custom)?;
    Ok(Date { date })
  }
}