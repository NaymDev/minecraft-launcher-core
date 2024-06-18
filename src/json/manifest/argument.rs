use serde::{ Deserialize, Serialize };

use crate::json::EnvironmentFeatures;

use super::rule::{ Rule, RuleAction };

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ArgumentType {
  Game,
  Jvm,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged, rename_all = "camelCase")]
pub enum Argument {
  Value(ArgumentValue),
  Object {
    rules: Vec<Rule>,
    value: ArgumentValue,
  },
}

impl Argument {
  pub fn apply(&self, env_features: &EnvironmentFeatures) -> Option<Vec<&String>> {
    if self.applies_to_current_environment(env_features) { Some(self.value()) } else { None }
  }

  pub fn value(&self) -> Vec<&String> {
    match self {
      Argument::Value(value) => value.value(),
      Argument::Object { value, .. } => value.value(),
    }
  }

  pub fn applies_to_current_environment(&self, env_features: &EnvironmentFeatures) -> bool {
    if let Argument::Object { rules, .. } = self {
      // TODO: needed?
      if rules.is_empty() {
        return true;
      }

      let mut action = RuleAction::Disallow;
      for rule in rules {
        if let Some(applied_action) = rule.get_applied_action(env_features) {
          action = applied_action;
        }
      }

      action == RuleAction::Allow
    } else {
      true
    }
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgumentValue {
  String(String),
  List(Vec<String>),
}

impl ArgumentValue {
  pub fn value(&self) -> Vec<&String> {
    match self {
      ArgumentValue::List(list) => list.iter().collect(),
      ArgumentValue::String(string) => vec![string],
    }
  }
}
