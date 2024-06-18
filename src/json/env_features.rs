use std::collections::HashMap;

use serde_json::Value;

use super::manifest::rule::{ Rule, RuleFeatureType };

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EnvironmentFeatures {
  pub features: HashMap<RuleFeatureType, Value>,
}

impl EnvironmentFeatures {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn set_feature(&mut self, feature_type: RuleFeatureType, value: Value) {
    self.features.insert(feature_type, value);
  }

  pub fn remove(&mut self, feature_type: RuleFeatureType) {
    self.features.remove(&feature_type);
  }

  pub fn has_feature(&self, feature_type: &RuleFeatureType, value: &Value) -> bool {
    self.features.get(feature_type) == Some(value)
  }
}

impl EnvironmentFeatures {
  /// Checks if the current object is compatible with the given rule.
  ///
  /// This function iterates through the features of the rule and checks if the current object
  /// has all those features with the required values.
  ///
  /// # Arguments
  ///
  /// * `rule` - A reference to the rule whose compatibility with the current object is to be checked.
  ///
  /// # Returns
  ///
  /// Returns `true` if the current object is compatible with all the features of the rule,
  /// otherwise returns `false`.
  pub fn compatible(&self, rule: &Rule) -> bool {
    if let Some(rule_features) = &rule.features {
      for (feature_type, value) in rule_features {
        if !self.has_feature(feature_type, value) {
          return false;
        }
      }
    }
    true
  }
}
