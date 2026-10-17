//! Input shapes more than one tool asks for, each refused as malformed before the tool runs and said so in its
//! schema.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

/// A whole number above nought.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Number")]
pub struct Positive(pub i64);

impl TryFrom<serde_json::Number> for Positive {
  type Error = &'static str;

  fn try_from(value: serde_json::Number) -> Result<Self, Self::Error> {
    value
      .as_i64()
      .filter(|whole| *whole > 0)
      .map(Self)
      .ok_or("must be a whole number above nought")
  }
}

impl JsonSchema for Positive {
  fn inline_schema() -> bool {
    true
  }

  fn schema_name() -> Cow<'static, str> {
    "Positive".into()
  }

  fn json_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({ "type": "integer", "exclusiveMinimum": 0 })
  }
}

/// A list holding at least one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<T>")]
pub struct AtLeastOne<T>(pub Vec<T>);

impl<T> TryFrom<Vec<T>> for AtLeastOne<T> {
  type Error = &'static str;

  fn try_from(value: Vec<T>) -> Result<Self, Self::Error> {
    if value.is_empty() { Err("must hold at least one") } else { Ok(Self(value)) }
  }
}

impl<T: JsonSchema> JsonSchema for AtLeastOne<T> {
  fn inline_schema() -> bool {
    true
  }

  fn schema_name() -> Cow<'static, str> {
    format!("AtLeastOne_{}", T::schema_name()).into()
  }

  fn json_schema(generator: &mut SchemaGenerator) -> Schema {
    json_schema!({ "type": "array", "items": generator.subschema_for::<T>(), "minItems": 1 })
  }
}
