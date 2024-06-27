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

  pub fn build(self) -> ArgumentSubstitutor {
    ArgumentSubstitutor::new(self.map)
  }
}

pub struct ArgumentSubstitutor {
  substitutions: HashMap<String, String>,
}

impl ArgumentSubstitutor {
  pub fn new(substitutions: HashMap<String, String>) -> Self {
    Self { substitutions }
  }

  pub fn substitute(&self, input: &str) -> String {
    let mut output = input.to_string();
    for (key, value) in &self.substitutions {
      output = output.replace(&format!("${{{}}}", key), value);
    }
    output
  }

  pub fn substitute_all(&self, input: Vec<&str>) -> Vec<String> {
    let mut output = input
      .iter()
      .map(|s| s.to_string())
      .collect::<Vec<_>>();

    for (key, value) in &self.substitutions {
      for input in &mut output {
        *input = input.replace(&format!("${{{}}}", key), value);
      }
    }
    output
  }
}
