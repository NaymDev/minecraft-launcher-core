use serde::{ Deserialize, Serialize };

use super::rule::{ FeatureMatcher, Rule, RuleAction };

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
  pub fn apply(&self, matcher: &impl FeatureMatcher) -> Option<Vec<&String>> {
    if self.applies_to_current_environment(matcher) { Some(self.value()) } else { None }
  }

  pub fn value(&self) -> Vec<&String> {
    match self {
      Argument::Value(value) => value.value(),
      Argument::Object { value, .. } => value.value(),
    }
  }

  pub fn applies_to_current_environment(&self, matcher: &impl FeatureMatcher) -> bool {
    match self {
      Argument::Value(_) => true,
      Argument::Object { rules, .. } => {
        let mut action = RuleAction::Disallow;
        for rule in rules {
          if let Some(applied_action) = rule.get_applied_action(Some(matcher)) {
            action = applied_action;
          }
        }

        action == RuleAction::Allow
      }
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
