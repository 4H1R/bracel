use super::error::{AppError, IssueCode, ValidationErrors};
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, Visitor},
};
use serde_json::Value;
use std::{collections::BTreeMap, fmt};

/// A JSON object that rejects duplicates and aggregates safe field errors.
pub struct Fields {
    values: BTreeMap<String, Value>,
    errors: ValidationErrors,
}
impl<'de> Deserialize<'de> for Fields {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Object;
        impl<'de> Visitor<'de> for Object {
            type Value = Fields;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an object with unique fields")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Fields, M::Error> {
                let mut values = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, Value>()? {
                    if values.insert(key, value).is_some() {
                        return Err(serde::de::Error::custom("duplicate field"));
                    }
                }
                Ok(Fields {
                    values,
                    errors: ValidationErrors::default(),
                })
            }
        }
        deserializer.deserialize_map(Object)
    }
}
impl Fields {
    pub fn text(&mut self, field: &str, maximum: usize) -> String {
        match self.values.remove(field) {
            Some(Value::String(value)) => {
                let value = value.trim().to_owned();
                if value.is_empty() {
                    self.errors.add(
                        [field.into()],
                        IssueCode::TooSmall,
                        "This field is required.",
                    );
                }
                if value.chars().count() > maximum {
                    self.errors
                        .add([field.into()], IssueCode::TooBig, "This field is too long.");
                }
                value
            }
            _ => {
                self.invalid(field);
                String::new()
            }
        }
    }
    pub fn integer(&mut self, field: &str) -> i64 {
        match self.values.remove(field).and_then(|v| v.as_i64()) {
            Some(v) => v,
            None => {
                self.invalid(field);
                0
            }
        }
    }
    pub fn boolean(&mut self, field: &str) -> bool {
        match self.values.remove(field).and_then(|v| v.as_bool()) {
            Some(v) => v,
            None => {
                self.invalid(field);
                false
            }
        }
    }
    fn invalid(&mut self, field: &str) {
        self.errors.add(
            [field.into()],
            IssueCode::InvalidType,
            "This field is required and must have the expected type.",
        );
    }
    pub fn finish(mut self) -> Result<(), AppError> {
        if !self.values.is_empty() {
            self.errors.add(
                [],
                IssueCode::UnrecognizedKeys,
                "Unknown fields are not allowed.",
            );
        }
        self.errors.finish()
    }
}
