use crate::{Rules, Validated, Validator};
use fornthulani::{Request, ThulaniError, parse_urlencoded};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

const MAX_FORM_DEPTH: usize = 8;
const MAX_FORM_ARRAY: usize = 1000;

/// Everything the client sent, as one JSON object: query-string parameters
/// first, then the body (JSON object or `application/x-www-form-urlencoded`)
/// which wins on conflicts — like Laravel's `$request->all()`.
///
/// Form keys understand `tags[]=a&tags[]=b` and `user[name]=x`.
/// A non-empty body with any other content type is a 415.
pub fn merged_input(req: &Request) -> Result<Value, ThulaniError> {
    let mut map = Map::new();
    for (k, v) in req.query() {
        insert_form_pair(&mut map, &k, v);
    }

    if !req.body.is_empty() {
        let mt = req.media_type();
        match mt.as_deref() {
            Some(m)
                if m == "application/json"
                    || (m.starts_with("application/") && m.ends_with("+json")) =>
            {
                let parsed: Value = serde_json::from_slice(&req.body)
                    .map_err(|_| ThulaniError::BadRequest("invalid json".into()))?;
                match parsed {
                    Value::Object(o) => {
                        for (k, v) in o {
                            map.insert(k, v);
                        }
                    }
                    _ => {
                        return Err(ThulaniError::BadRequest(
                            "json body must be an object".into(),
                        ));
                    }
                }
            }
            Some("application/x-www-form-urlencoded") => {
                let text = std::str::from_utf8(&req.body)
                    .map_err(|_| ThulaniError::BadRequest("body is not valid utf-8".into()))?;
                for (k, v) in parse_urlencoded(text) {
                    insert_form_pair(&mut map, &k, v);
                }
            }
            _ => {
                return Err(ThulaniError::UnsupportedMediaType(
                    "expected application/json or application/x-www-form-urlencoded".into(),
                ));
            }
        }
    }
    Ok(Value::Object(map))
}

/// `name`, `name[]`, `name[key]`, `name[a][b]`.
fn insert_form_pair(map: &mut Map<String, Value>, key: &str, value: String) {
    let (base, rest) = match key.find('[') {
        Some(i) => (&key[..i], &key[i..]),
        None => (key, ""),
    };
    if base.is_empty() {
        return;
    }

    let mut parts: Vec<&str> = Vec::new();
    let mut r = rest;
    while let Some(stripped) = r.strip_prefix('[') {
        match stripped.find(']') {
            Some(j) => {
                parts.push(&stripped[..j]);
                r = &stripped[j + 1..];
            }
            None => return,
        }
        if parts.len() > MAX_FORM_DEPTH {
            return;
        }
    }
    if !r.is_empty() {
        return;
    }

    let mut cur: &mut Value = map.entry(base.to_string()).or_insert(Value::Null);
    if parts.is_empty() {
        *cur = Value::String(value);
        return;
    }
    for (i, p) in parts.iter().enumerate() {
        let last = i + 1 == parts.len();
        if p.is_empty() {
            if !cur.is_array() {
                *cur = Value::Array(Vec::new());
            }
            let Some(arr) = cur.as_array_mut() else {
                return;
            };
            if arr.len() >= MAX_FORM_ARRAY {
                return;
            }
            if last {
                arr.push(Value::String(value));
                return;
            }
            arr.push(Value::Null);
            let idx = arr.len() - 1;
            cur = &mut arr[idx];
        } else {
            if !cur.is_object() {
                *cur = Value::Object(Map::new());
            }
            let Some(obj) = cur.as_object_mut() else {
                return;
            };
            if last {
                obj.insert(p.to_string(), Value::String(value));
                return;
            }
            cur = obj.entry(p.to_string()).or_insert(Value::Null);
        }
    }
}

/// Laravel's Form Request: a typed input struct that carries its own rules.
///
/// ```ignore
/// #[derive(Deserialize)]
/// struct StorePost { title: String, body: String, published: Option<bool> }
///
/// impl FormRequest for StorePost {
///     fn rules() -> Rules {
///         Rules::new()
///             .field("title", "required|string|max:200")
///             .field("body", "required|string")
///             .field("published", "nullable|boolean")
///     }
///     fn authorize(req: &Request) -> bool { req.bearer_token().is_some() }
/// }
///
/// fn store(req: &Request, _: &Params) -> Result<Response, ThulaniError> {
///     let input: StorePost = req.validated()?;   // 403 / 422 handled for you
///     ...
/// }
/// ```
pub trait FormRequest: DeserializeOwned {
    fn rules() -> Rules;

    /// Return `false` to answer 403 before validating.
    fn authorize(_req: &Request) -> bool {
        true
    }
}

/// Validation on the request itself.
pub trait ValidateExt {
    /// The merged client input (see [`merged_input`]).
    fn input(&self) -> Result<Value, ThulaniError>;

    /// Validate the request against `rules`; failures become a 422 whose
    /// JSON body lists the messages per field.
    fn validate<R: Into<Rules>>(&self, rules: R) -> Result<Validated, ThulaniError>;

    /// Authorise, validate and deserialize into a [`FormRequest`] type.
    fn validated<T: FormRequest>(&self) -> Result<T, ThulaniError>;
}

impl ValidateExt for Request {
    fn input(&self) -> Result<Value, ThulaniError> {
        merged_input(self)
    }

    fn validate<R: Into<Rules>>(&self, rules: R) -> Result<Validated, ThulaniError> {
        let input = merged_input(self)?;
        Validator::make(input, rules)
            .validate()
            .map_err(ThulaniError::from)
    }

    fn validated<T: FormRequest>(&self) -> Result<T, ThulaniError> {
        if !T::authorize(self) {
            return Err(ThulaniError::Forbidden(
                "this action is unauthorized".into(),
            ));
        }
        self.validate(T::rules())?.deserialize::<T>()
    }
}
