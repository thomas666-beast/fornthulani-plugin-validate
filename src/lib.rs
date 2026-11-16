//! Laravel-style validation for fornthulani.
//!
//! ```ignore
//! use fornthulani_plugin_validate::ValidateExt;
//!
//! fn register(req: &Request, _: &Params) -> Result<Response, ThulaniError> {
//!     let data = req.validate([
//!         ("name",     "required|string|min:2|max:100"),
//!         ("email",    "required|email|max:255"),
//!         ("password", "required|string|min:12|confirmed"),
//!         ("age",      "nullable|integer|between:18,120"),
//!         ("tags.*",   "string|max:20"),
//!     ])?;                                   // 422 + JSON errors on failure
//!     let email = data.str("email").unwrap();
//!     ...
//! }
//! ```
//!
//! See [`FormRequest`] for typed input structs (Laravel's Form Requests).

mod checks;
mod errors;
mod input;
mod messages;
mod rule;

pub use errors::{Validated, ValidationErrors};
pub use input::{FormRequest, ValidateExt, merged_input};
pub use rule::{CustomFn, IntoRules, Rule, Rules, rules};

use checks::{Ctx, get_path, is_blank, kind_of, scalar_string};
use messages::{default_template, render};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

// ---------------------------------------------------------------------
// extension rules (e.g. `unique` / `exists` from the ORM plugin)
// ---------------------------------------------------------------------

/// What an extension rule gets to look at.
pub struct ExtCtx<'a> {
    pub value: &'a Value,
    pub args: &'a [String],
    pub attribute: &'a str,
    pub input: &'a Value,
}

pub type ExtFn = fn(&ExtCtx<'_>) -> bool;

fn registry() -> &'static RwLock<HashMap<String, (ExtFn, String)>> {
    static REG: OnceLock<RwLock<HashMap<String, (ExtFn, String)>>> = OnceLock::new();
    REG.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Register a global rule usable from every rule string
/// (`"required|unique:users,email"`). `message` may use `:attribute`.
/// Return `true` from `f` when the value is valid.
pub fn extend(name: &str, f: ExtFn, message: &str) {
    if let Ok(mut reg) = registry().write() {
        reg.insert(name.to_ascii_lowercase(), (f, message.to_string()));
    }
}

// ---------------------------------------------------------------------
// Validator
// ---------------------------------------------------------------------

pub struct Validator {
    input: Value,
    rules: Rules,
}

enum Fail {
    Default,
    Message(String),
}

impl Validator {
    /// `input` is usually a JSON object.
    pub fn make(input: Value, rules: impl Into<Rules>) -> Self {
        Validator {
            input,
            rules: rules.into(),
        }
    }

    pub fn fails(&self) -> bool {
        self.validate().is_err()
    }

    pub fn errors(&self) -> ValidationErrors {
        self.validate().err().unwrap_or_default()
    }

    /// Run every rule. `Ok` carries only the fields that have rules.
    pub fn validate(&self) -> Result<Validated, ValidationErrors> {
        let mut errors = ValidationErrors::default();
        let mut passed: Vec<(String, &Vec<Rule>)> = Vec::new();

        for (pattern, rules) in &self.rules.fields {
            let segs: Vec<&str> = pattern.split('.').collect();
            let mut paths = Vec::new();
            expand(Some(&self.input), &segs, String::new(), &mut paths);
            for path in paths {
                let before = errors.count_for(&path);
                self.check_field(&path, pattern, rules, &mut errors);
                if errors.count_for(&path) == before {
                    passed.push((path, rules));
                }
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }

        let mut out = Value::Object(Map::new());
        for (path, rules) in passed {
            if let Some(v) = get_path(&self.input, &path) {
                let segs: Vec<&str> = path.split('.').collect();
                insert_path(&mut out, &self.input, &segs, coerce(v, rules));
            }
        }
        Ok(Validated(out))
    }

    fn check_field(
        &self,
        path: &str,
        pattern: &str,
        rules: &[Rule],
        errors: &mut ValidationErrors,
    ) {
        let value = get_path(&self.input, path);
        let has = |n: &str| rules.iter().any(|r| r.name == n);

        if has("sometimes") && value.is_none() {
            return;
        }
        let nullable = has("nullable");
        let bail = has("bail");
        let numeric = has("integer") || has("numeric");
        let ctx = Ctx {
            input: &self.input,
            path,
            numeric,
        };
        let blank = is_blank(value);

        for rule in rules {
            let fail = self.eval(rule, value, blank, nullable, &ctx);
            let Some(fail) = fail else { continue };

            let kind = kind_of(value, numeric);
            let template = self
                .override_message(path, pattern, &rule.name)
                .unwrap_or_else(|| match fail {
                    Fail::Default => default_template(rule, kind).to_string(),
                    Fail::Message(m) => m,
                });
            errors.push(
                path,
                render(&template, &self.attribute(path, pattern), rule),
            );
            if bail {
                break;
            }
        }
    }

    /// `Some(..)` when the rule fails for this value.
    fn eval(
        &self,
        rule: &Rule,
        value: Option<&Value>,
        blank: bool,
        nullable: bool,
        ctx: &Ctx<'_>,
    ) -> Option<Fail> {
        let input = &self.input;
        let arg = |i: usize| rule.args.get(i).map(String::as_str).unwrap_or("");
        let other_present = |p: &str| !is_blank(get_path(input, p));
        let fail_if = |cond: bool| if cond { Some(Fail::Default) } else { None };

        match rule.name.as_str() {
            "bail" | "nullable" | "sometimes" => return None,

            // ---- implicit rules: run even when the value is missing/empty ----
            "required" => return fail_if(blank),
            "present" => return fail_if(value.is_none()),
            "filled" => return fail_if(value.is_some() && blank),
            "prohibited" => return fail_if(value.is_some() && !blank),
            "accepted" => {
                let ok = value.and_then(scalar_string).is_some_and(|s| {
                    matches!(s.to_lowercase().as_str(), "yes" | "on" | "1" | "true")
                });
                return fail_if(!ok);
            }
            "declined" => {
                let ok = value.and_then(scalar_string).is_some_and(|s| {
                    matches!(s.to_lowercase().as_str(), "no" | "off" | "0" | "false")
                });
                return fail_if(!ok);
            }
            "required_if" | "required_unless" => {
                let other = get_path(input, arg(0)).and_then(scalar_string);
                let listed = other.is_some_and(|o| rule.args.iter().skip(1).any(|v| *v == o));
                let required = if rule.name == "required_if" {
                    listed
                } else {
                    !listed
                };
                return fail_if(required && blank);
            }
            "required_with" => {
                return fail_if(rule.args.iter().any(|p| other_present(p)) && blank);
            }
            "required_with_all" => {
                return fail_if(
                    !rule.args.is_empty() && rule.args.iter().all(|p| other_present(p)) && blank,
                );
            }
            "required_without" => {
                return fail_if(rule.args.iter().any(|p| !other_present(p)) && blank);
            }
            "required_without_all" => {
                return fail_if(
                    !rule.args.is_empty() && rule.args.iter().all(|p| !other_present(p)) && blank,
                );
            }
            _ => {}
        }

        // ---- everything else only looks at values that are there ----
        let v = match value {
            None => return None,
            Some(Value::Null) if nullable => return None,
            Some(Value::String(s)) if s.trim().is_empty() => return None,
            Some(v) => v,
        };

        if let Some(f) = &rule.custom {
            return f(v).err().map(Fail::Message);
        }

        match checks::run(rule, v, ctx) {
            Some(true) => None,
            Some(false) => Some(Fail::Default),
            None => self.eval_extension(rule, v, ctx),
        }
    }

    fn eval_extension(&self, rule: &Rule, v: &Value, ctx: &Ctx<'_>) -> Option<Fail> {
        let found = registry()
            .read()
            .ok()
            .and_then(|reg| reg.get(&rule.name).cloned());
        match found {
            Some((f, message)) => {
                let ext = ExtCtx {
                    value: v,
                    args: &rule.args,
                    attribute: ctx.path,
                    input: &self.input,
                };
                if f(&ext) {
                    None
                } else {
                    Some(Fail::Message(message))
                }
            }
            None => {
                eprintln!(
                    "[fornthulani-validate] unknown rule `{}` on `{}`; failing closed",
                    rule.name, ctx.path
                );
                Some(Fail::Message(format!(
                    "The :attribute field uses an undefined rule ({}).",
                    rule.name
                )))
            }
        }
    }

    fn override_message(&self, path: &str, pattern: &str, rule: &str) -> Option<String> {
        let m = &self.rules.messages;
        m.get(&format!("{path}.{rule}"))
            .or_else(|| m.get(&format!("{pattern}.{rule}")))
            .or_else(|| m.get(rule))
            .cloned()
    }

    fn attribute(&self, path: &str, pattern: &str) -> String {
        let a = &self.rules.attributes;
        a.get(path)
            .or_else(|| a.get(pattern))
            .cloned()
            .unwrap_or_else(|| path.replace('_', " "))
    }
}

// ---------------------------------------------------------------------
// path expansion & output building
// ---------------------------------------------------------------------

const MAX_EXPANDED_PATHS: usize = 10_000;

fn join(prefix: &str, seg: &str) -> String {
    if prefix.is_empty() {
        seg.to_string()
    } else {
        format!("{prefix}.{seg}")
    }
}

/// Expand wildcards (`items.*.qty`) into concrete paths (`items.0.qty`).
/// A missing non-wildcard segment still yields its path, so `required`
/// can report it. A wildcard over a missing/empty array yields nothing.
fn expand(cur: Option<&Value>, segs: &[&str], prefix: String, out: &mut Vec<String>) {
    if out.len() >= MAX_EXPANDED_PATHS {
        return;
    }
    let Some((seg, rest)) = segs.split_first() else {
        out.push(prefix);
        return;
    };
    if *seg == "*" {
        match cur {
            Some(Value::Array(a)) => {
                for (i, v) in a.iter().enumerate() {
                    expand(Some(v), rest, join(&prefix, &i.to_string()), out);
                }
            }
            Some(Value::Object(m)) => {
                for (k, v) in m {
                    expand(Some(v), rest, join(&prefix, k), out);
                }
            }
            _ => {}
        }
    } else {
        let next = match cur {
            Some(Value::Object(m)) => m.get(*seg),
            Some(Value::Array(a)) => seg.parse::<usize>().ok().and_then(|i| a.get(i)),
            _ => None,
        };
        expand(next, rest, join(&prefix, seg), out);
    }
}

/// Copy `val` into `out` at `segs`, creating objects/arrays to mirror the
/// shape of `input`.
fn insert_path(out: &mut Value, input: &Value, segs: &[&str], val: Value) {
    let mut cur_out = out;
    let mut cur_in = Some(input);
    for (i, seg) in segs.iter().enumerate() {
        let last = i + 1 == segs.len();
        if matches!(cur_in, Some(Value::Array(_))) {
            let idx = seg.parse::<usize>().unwrap_or(0);
            if !cur_out.is_array() {
                *cur_out = Value::Array(Vec::new());
            }
            let Some(arr) = cur_out.as_array_mut() else {
                return;
            };
            while arr.len() <= idx {
                arr.push(Value::Null);
            }
            if last {
                arr[idx] = val;
                return;
            }
            cur_in = cur_in.and_then(|v| v.get(idx));
            cur_out = &mut arr[idx];
        } else {
            if !cur_out.is_object() {
                *cur_out = Value::Object(Map::new());
            }
            let Some(map) = cur_out.as_object_mut() else {
                return;
            };
            if last {
                map.insert(seg.to_string(), val);
                return;
            }
            cur_in = cur_in.and_then(|v| v.get(*seg));
            cur_out = map.entry(seg.to_string()).or_insert(Value::Null);
        }
    }
}

/// `"42"` -> `42` when the field is declared `integer`/`numeric`, and
/// `"true"`/`"1"` -> `true` for `boolean`, so form and query input
/// deserialises into typed structs.
fn coerce(v: &Value, rules: &[Rule]) -> Value {
    let has = |n: &str| rules.iter().any(|r| r.name == n);
    if let Value::String(s) = v {
        if has("integer") {
            if let Ok(n) = s.parse::<i64>() {
                return Value::from(n);
            }
        } else if has("numeric")
            && let Some(n) = s.parse::<f64>().ok().and_then(serde_json::Number::from_f64)
        {
            return Value::Number(n);
        }
    }
    if let Some(b) = checks::as_bool(v).filter(|_| has("boolean")) {
        return Value::Bool(b);
    }
    v.clone()
}
