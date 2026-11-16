//! Rules and rule sets.
//!
//! A rule is a name plus arguments. Write them Laravel-style as strings
//! (`"required|email|max:255"`) or build them with the helpers in [`rules`]
//! (`rules::min(3)`), or write your own with [`Rule::custom`].

use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

pub type CustomFn = Arc<dyn Fn(&Value) -> Result<(), String> + Send + Sync>;

#[derive(Clone)]
pub struct Rule {
    pub(crate) name: String,
    pub(crate) args: Vec<String>,
    pub(crate) custom: Option<CustomFn>,
}

impl fmt::Debug for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Rule({}:{})", self.name, self.args.join(","))
    }
}

impl Rule {
    pub fn new(name: &str, args: &[&str]) -> Self {
        Rule {
            name: name.to_ascii_lowercase(),
            args: args.iter().map(|a| a.to_string()).collect(),
            custom: None,
        }
    }

    /// Parse one `name` or `name:arg1,arg2` rule. For `regex` / `not_regex`
    /// the whole remainder is a single argument (patterns may contain
    /// commas).
    pub fn parse(s: &str) -> Rule {
        let (name, rest) = match s.split_once(':') {
            Some((n, r)) => (n.trim(), Some(r)),
            None => (s.trim(), None),
        };
        let name = name.to_ascii_lowercase();
        let args = match rest {
            None => Vec::new(),
            Some(r) if name == "regex" || name == "not_regex" => vec![r.to_string()],
            Some(r) => r.split(',').map(|a| a.trim().to_string()).collect(),
        };
        Rule {
            name,
            args,
            custom: None,
        }
    }

    /// A rule backed by your own closure. Return `Err(message)` to fail;
    /// `:attribute` in the message is replaced by the field name.
    ///
    /// ```ignore
    /// Rule::custom(|v| match v.as_str() {
    ///     Some(s) if s.starts_with("ACME-") => Ok(()),
    ///     _ => Err("The :attribute must start with ACME-.".into()),
    /// })
    /// ```
    pub fn custom<F>(f: F) -> Self
    where
        F: Fn(&Value) -> Result<(), String> + Send + Sync + 'static,
    {
        Rule {
            name: "custom".into(),
            args: Vec::new(),
            custom: Some(Arc::new(f)),
        }
    }
}

/// Typed constructors for rules, for when strings are not convenient
/// (regexes containing `|`, values computed at runtime, ...).
pub mod rules {
    use super::Rule;
    use std::fmt::Display;

    fn bare(name: &str) -> Rule {
        Rule::new(name, &[])
    }
    fn with(name: &str, args: Vec<String>) -> Rule {
        Rule {
            name: name.to_string(),
            args,
            custom: None,
        }
    }

    pub fn required() -> Rule {
        bare("required")
    }
    pub fn nullable() -> Rule {
        bare("nullable")
    }
    pub fn string() -> Rule {
        bare("string")
    }
    pub fn integer() -> Rule {
        bare("integer")
    }
    pub fn numeric() -> Rule {
        bare("numeric")
    }
    pub fn boolean() -> Rule {
        bare("boolean")
    }
    pub fn array() -> Rule {
        bare("array")
    }
    pub fn email() -> Rule {
        bare("email")
    }
    pub fn url() -> Rule {
        bare("url")
    }
    pub fn uuid() -> Rule {
        bare("uuid")
    }
    pub fn date() -> Rule {
        bare("date")
    }
    pub fn confirmed() -> Rule {
        bare("confirmed")
    }
    pub fn min(n: impl Display) -> Rule {
        with("min", vec![n.to_string()])
    }
    pub fn max(n: impl Display) -> Rule {
        with("max", vec![n.to_string()])
    }
    pub fn size(n: impl Display) -> Rule {
        with("size", vec![n.to_string()])
    }
    pub fn between(a: impl Display, b: impl Display) -> Rule {
        with("between", vec![a.to_string(), b.to_string()])
    }
    pub fn one_of<I, S>(values: I) -> Rule
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        with("in", values.into_iter().map(Into::into).collect())
    }
    pub fn not_one_of<I, S>(values: I) -> Rule
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        with("not_in", values.into_iter().map(Into::into).collect())
    }
    /// A regular expression (linear-time engine: no catastrophic backtracking).
    pub fn regex(pattern: &str) -> Rule {
        with("regex", vec![pattern.to_string()])
    }
    pub fn same(field: &str) -> Rule {
        with("same", vec![field.to_string()])
    }
    pub fn different(field: &str) -> Rule {
        with("different", vec![field.to_string()])
    }
    /// Needs the `fornthulani-plugin-orm` `validation` feature.
    pub fn unique(table: &str, column: &str) -> Rule {
        with("unique", vec![table.to_string(), column.to_string()])
    }
    /// Needs the `fornthulani-plugin-orm` `validation` feature.
    pub fn exists(table: &str, column: &str) -> Rule {
        with("exists", vec![table.to_string(), column.to_string()])
    }
}

/// Anything that can describe the rules of one field.
pub trait IntoRules {
    fn into_rules(self) -> Vec<Rule>;
}

impl IntoRules for &str {
    fn into_rules(self) -> Vec<Rule> {
        self.split('|')
            .filter(|s| !s.trim().is_empty())
            .map(Rule::parse)
            .collect()
    }
}
impl IntoRules for String {
    fn into_rules(self) -> Vec<Rule> {
        self.as_str().into_rules()
    }
}
impl IntoRules for Rule {
    fn into_rules(self) -> Vec<Rule> {
        vec![self]
    }
}
impl IntoRules for Vec<Rule> {
    fn into_rules(self) -> Vec<Rule> {
        self
    }
}
impl<const N: usize> IntoRules for [Rule; N] {
    fn into_rules(self) -> Vec<Rule> {
        self.into_iter().collect()
    }
}

/// The rule set for one validation: fields, custom messages, attribute names.
///
/// ```ignore
/// let rules = Rules::new()
///     .field("email", "required|email|max:255")
///     .field("age", "nullable|integer|between:18,120")
///     .field("tags.*", "string|max:20")
///     .message("email.required", "We need your email address.")
///     .attribute("dob", "date of birth");
/// ```
#[derive(Clone, Debug, Default)]
pub struct Rules {
    pub(crate) fields: Vec<(String, Vec<Rule>)>,
    pub(crate) messages: HashMap<String, String>,
    pub(crate) attributes: HashMap<String, String>,
}

impl Rules {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add rules for a field. Use dots for nesting (`address.city`) and `*`
    /// for every element of an array (`items.*.qty`).
    pub fn field(mut self, name: &str, rules: impl IntoRules) -> Self {
        self.fields.push((name.to_string(), rules.into_rules()));
        self
    }

    /// Override a message: key is `rule` (all fields) or `field.rule`.
    /// Placeholders: `:attribute`, `:min`, `:max`, `:size`, `:other`, ...
    pub fn message(mut self, key: &str, text: &str) -> Self {
        self.messages.insert(key.to_string(), text.to_string());
        self
    }

    /// Human-friendly name for a field in messages.
    pub fn attribute(mut self, field: &str, label: &str) -> Self {
        self.attributes.insert(field.to_string(), label.to_string());
        self
    }
}

impl<const N: usize> From<[(&str, &str); N]> for Rules {
    fn from(pairs: [(&str, &str); N]) -> Self {
        pairs
            .iter()
            .fold(Rules::new(), |r, (f, spec)| r.field(f, *spec))
    }
}

impl<const N: usize> From<&[(&str, &str); N]> for Rules {
    fn from(pairs: &[(&str, &str); N]) -> Self {
        pairs
            .iter()
            .fold(Rules::new(), |r, (f, spec)| r.field(f, *spec))
    }
}

impl From<&[(&str, &str)]> for Rules {
    fn from(pairs: &[(&str, &str)]) -> Self {
        pairs
            .iter()
            .fold(Rules::new(), |r, (f, spec)| r.field(f, *spec))
    }
}
