use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct ArgumentSubstitutorBuilder {
  map: HashMap<String, String>,
}

impl ArgumentSubstitutorBuilder {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn add(&mut self, key: impl AsRef<str>, value: impl AsRef<str>) -> &mut Self {
    self.map.insert(key.as_ref().to_string(), value.as_ref().to_string());
    self
  }

  pub fn add_all(&mut self, map: HashMap<impl AsRef<str>, impl AsRef<str>>) -> &mut Self {
    for (key, value) in map {
      self.add(key, value);
    }
    self
  }

  pub fn build(self) -> impl Fn(String) -> String {
    move |input| {
      let mut output = input;
      for (key, value) in &self.map {
        output = output.replace(&format!("${{{}}}", key), value);
      }
      output
    }
  }
}
