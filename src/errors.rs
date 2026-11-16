use fornthulani::{Response, Status, ThulaniError};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

/// Per-field validation messages, in the order the rules were declared.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationErrors {
    entries: Vec<(String, Vec<String>)>,
}

impl ValidationErrors {
    pub(crate) fn push(&mut self, field: &str, message: String) {
        if let Some((_, msgs)) = self.entries.iter_mut().find(|(f, _)| f == field) {
            msgs.push(message);
        } else {
            self.entries.push((field.to_string(), vec![message]));
        }
    }

    pub(crate) fn count_for(&self, field: &str) -> usize {
        self.entries
            .iter()
            .find(|(f, _)| f == field)
            .map(|(_, m)| m.len())
            .unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Total number of messages.
    pub fn count(&self) -> usize {
        self.entries.iter().map(|(_, m)| m.len()).sum()
    }

    pub fn get(&self, field: &str) -> Option<&[String]> {
        self.entries
            .iter()
            .find(|(f, _)| f == field)
            .map(|(_, m)| m.as_slice())
    }

    pub fn has(&self, field: &str) -> bool {
        self.get(field).is_some()
    }

    /// The first message of the first failing field.
    pub fn first(&self) -> Option<&str> {
        self.entries
            .first()
            .and_then(|(_, m)| m.first())
            .map(String::as_str)
    }

    pub fn fields(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(f, _)| f.as_str())
    }

    /// Laravel's 422 body:
    /// `{"message": "The email field is required. (and 1 more error)",
    ///   "errors": {"email": ["..."], "age": ["..."]}}`
    pub fn to_json(&self) -> Value {
        let mut errors = Map::new();
        for (f, msgs) in &self.entries {
            errors.insert(f.clone(), json!(msgs));
        }
        let more = self.count().saturating_sub(1);
        let mut message = self
            .first()
            .unwrap_or("The given data was invalid.")
            .to_string();
        match more {
            0 => {}
            1 => message.push_str(" (and 1 more error)"),
            n => message.push_str(&format!(" (and {n} more errors)")),
        }
        json!({ "message": message, "errors": errors })
    }

    pub fn into_response(self) -> Response {
        Response::json(Status::UNPROCESSABLE_ENTITY, &self.to_json())
    }
}

impl std::fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.first().unwrap_or("validation failed"))
    }
}

impl std::error::Error for ValidationErrors {}

/// So controllers can simply use `?`: the client gets a 422 with the
/// per-field messages.
impl From<ValidationErrors> for ThulaniError {
    fn from(e: ValidationErrors) -> Self {
        ThulaniError::Response(Box::new(e.into_response()))
    }
}

/// The data that passed validation: **only** the fields that have rules
/// (the equivalent of Laravel's `$request->validated()`), with numbers and
/// booleans coerced when the rules say `integer` / `numeric` / `boolean`.
///
/// Anything the client sent that you did not declare a rule for is dropped,
/// which is what protects you from mass-assignment of unexpected fields.
#[derive(Debug, Clone, PartialEq)]
pub struct Validated(pub(crate) Value);

impl Validated {
    pub fn as_value(&self) -> &Value {
        &self.0
    }

    pub fn into_value(self) -> Value {
        self.0
    }

    /// Look up a (dotted) path: `validated.get("address.city")`.
    pub fn get(&self, path: &str) -> Option<&Value> {
        crate::checks::get_path(&self.0, path)
    }

    pub fn str(&self, path: &str) -> Option<&str> {
        self.get(path).and_then(Value::as_str)
    }

    pub fn i64(&self, path: &str) -> Option<i64> {
        self.get(path).and_then(Value::as_i64)
    }

    pub fn f64(&self, path: &str) -> Option<f64> {
        self.get(path).and_then(Value::as_f64)
    }

    pub fn bool(&self, path: &str) -> Option<bool> {
        self.get(path).and_then(Value::as_bool)
    }

    /// Turn the validated data into your own struct.
    pub fn deserialize<T: DeserializeOwned>(&self) -> Result<T, ThulaniError> {
        serde_json::from_value(self.0.clone()).map_err(|e| {
            eprintln!("[fornthulani-validate] validated data does not fit the target type: {e}");
            ThulaniError::Unprocessable("input does not match the expected shape".into())
        })
    }
}
