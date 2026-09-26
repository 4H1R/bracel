//! Declarative JSON validation shared by request decoding, schemas and generators.
use super::error::{AppError, IssueCode, PathSegment, ValidationErrors};
use serde::Serialize;
use serde_json::{Map, Value, json};

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Rule {
    Text { min: usize, max: usize },
    Integer { min: i64, max: i64 },
    Boolean,
    Uuid,
    Timestamp,
    Decimal { precision: usize, scale: usize },
    Enum { values: Vec<String> },
    Object { fields: Vec<Field> },
    Array { item: Box<Rule>, max: usize },
}

#[derive(Clone, Serialize)]
pub struct Field {
    pub name: String,
    pub rule: Rule,
    pub required: bool,
    pub nullable: bool,
}

impl Field {
    pub fn required(name: &str, rule: Rule) -> Self {
        Self {
            name: name.into(),
            rule,
            required: true,
            nullable: false,
        }
    }
    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }
    pub fn nullable(mut self) -> Self {
        self.nullable = true;
        self
    }
}

#[derive(Clone)]
pub struct Schema(pub Vec<Field>);

impl Schema {
    /// PATCH validates present fields without inventing defaults or erasing explicit nulls.
    pub fn validate(&self, value: Value, patch: bool) -> Result<Map<String, Value>, AppError> {
        let mut errors = ValidationErrors::default();
        let output = object(&self.0, value, patch, &[], &mut errors);
        errors.finish()?;
        Ok(output)
    }
    pub fn openapi(&self, patch: bool) -> Value {
        object_schema(&self.0, patch)
    }
}

fn object(
    fields: &[Field],
    value: Value,
    patch: bool,
    path: &[PathSegment],
    errors: &mut ValidationErrors,
) -> Map<String, Value> {
    let Value::Object(mut input) = value else {
        errors.add(
            path.to_vec(),
            IssueCode::InvalidType,
            "An object is required.",
        );
        return Map::new();
    };
    let mut output = Map::new();
    for field in fields {
        let mut location = path.to_vec();
        location.push(PathSegment::Field(field.name.clone()));
        match input.remove(&field.name) {
            None if field.required && !patch => {
                errors.add(location, IssueCode::InvalidType, "This field is required.")
            }
            None => {}
            Some(Value::Null) if field.nullable => {
                output.insert(field.name.clone(), Value::Null);
            }
            Some(value) => {
                output.insert(
                    field.name.clone(),
                    check(&field.rule, value, &location, errors),
                );
            }
        }
    }
    if !input.is_empty() {
        errors.add(
            path.to_vec(),
            IssueCode::UnrecognizedKeys,
            "Unknown fields are not allowed.",
        );
    }
    output
}

fn check(rule: &Rule, value: Value, path: &[PathSegment], errors: &mut ValidationErrors) -> Value {
    let valid = match (rule, &value) {
        (Rule::Text { min, max }, Value::String(text)) => {
            let text = text.trim();
            let length = text.chars().count();
            if length < *min {
                errors.add(
                    path.to_vec(),
                    IssueCode::TooSmall,
                    "This field is too short.",
                );
            }
            if length > *max {
                errors.add(path.to_vec(), IssueCode::TooBig, "This field is too long.");
            }
            return Value::String(text.into());
        }
        (Rule::Integer { min, max }, value) => {
            value.as_i64().is_some_and(|v| (*min..=*max).contains(&v))
        }
        (Rule::Boolean, Value::Bool(_)) => true,
        (Rule::Uuid, Value::String(text)) => text.parse::<uuid::Uuid>().is_ok(),
        (Rule::Timestamp, Value::String(text)) => crate::query::timestamp(text).is_some(),
        (Rule::Decimal { precision, scale }, Value::String(text)) => {
            decimal(text, *precision, *scale)
        }
        (Rule::Enum { values }, Value::String(text)) => values.contains(text),
        (Rule::Object { fields }, _) => {
            return Value::Object(object(fields, value, false, path, errors));
        }
        (Rule::Array { item, max }, Value::Array(items)) => {
            if items.len() > *max {
                errors.add(path.to_vec(), IssueCode::TooBig, "Too many items.");
                return Value::Null;
            }
            return Value::Array(
                items
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        let mut location = path.to_vec();
                        location.push(PathSegment::Index(index));
                        check(item, value.clone(), &location, errors)
                    })
                    .collect(),
            );
        }
        _ => false,
    };
    if !valid {
        errors.add(
            path.to_vec(),
            IssueCode::InvalidType,
            "This field has an invalid value or type.",
        );
    }
    value
}

fn decimal(text: &str, precision: usize, scale: usize) -> bool {
    if precision == 0 || precision > 38 || scale > precision || text.len() > precision + 2 {
        return false;
    }
    let value = text.strip_prefix('-').unwrap_or(text);
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or("");
    let fraction = parts.next();
    !whole.is_empty()
        && whole.bytes().all(|c| c.is_ascii_digit())
        && whole.len() <= precision - scale
        && fraction.is_none_or(|s| {
            !s.is_empty() && s.len() <= scale && s.bytes().all(|c| c.is_ascii_digit())
        })
        && parts.next().is_none()
}

fn object_schema(fields: &[Field], patch: bool) -> Value {
    let properties: Map<String, Value> = fields
        .iter()
        .map(|field| {
            let value = rule_schema(&field.rule);
            (
                field.name.clone(),
                if field.nullable {
                    json!({"anyOf":[value,{"type":"null"}]})
                } else {
                    value
                },
            )
        })
        .collect();
    json!({"type":"object", "additionalProperties":false, "properties":properties,
        "required":fields.iter().filter(|f| f.required && !patch).map(|f| &f.name).collect::<Vec<_>>()})
}

fn rule_schema(rule: &Rule) -> Value {
    match rule {
        Rule::Text { min, max } => json!({"type":"string","minLength":min,"maxLength":max}),
        Rule::Integer { min, max } => json!({"type":"integer","minimum":min,"maximum":max}),
        Rule::Boolean => json!({"type":"boolean"}),
        Rule::Uuid => json!({"type":"string","format":"uuid"}),
        Rule::Timestamp => json!({"type":"string","format":"date-time"}),
        Rule::Decimal { precision, scale } => {
            json!({"type":"string","format":"decimal","x-precision":precision,"x-scale":scale})
        }
        Rule::Enum { values } => json!({"type":"string","enum":values}),
        Rule::Object { fields } => object_schema(fields, false),
        Rule::Array { item, max } => {
            json!({"type":"array","items":rule_schema(item),"maxItems":max})
        }
    }
}
