# fornthulani-plugin-validate

Laravel-style validation for [fornthulani](https://github.com/thomas666-beast/fornthulani).

```rust
use fornthulani_plugin_validate::ValidateExt;

fn register(req: &Request, _: &Params) -> Result<Response, ThulaniError> {
    let data = req.validate([
        ("name",     "required|string|min:2|max:100"),
        ("email",    "required|email|max:255|unique:users,email"),
        ("password", "required|string|min:12|confirmed"),
        ("age",      "nullable|integer|between:18,120"),
        ("tags.*",   "string|max:20|distinct"),
    ])?;                       // failure -> 422, body below
    let email = data.str("email").unwrap();
    // ...
}
```

A failure answers `422`:

```json
{ "message": "The email field must be a valid email address. (and 1 more error)",
  "errors": { "email": ["The email field must be a valid email address."],
              "age":   ["The age field must be between 18 and 120."] } }
```

## Where the input comes from

`req.input()` merges the query string and the body (JSON object or
`application/x-www-form-urlencoded`; the body wins), like Laravel's
`$request->all()`. Form keys understand `tags[]=a&tags[]=b` and
`user[name]=x`. A non-empty body with another content type is a `415`;
malformed JSON is a `400`.

## `validated()` only returns what you declared

The result contains **only fields that have rules**, and numeric/boolean
strings are coerced when you say `integer`, `numeric` or `boolean` — so
form and query input deserialises into typed structs. Undeclared fields
(`"is_admin": true`) are dropped. Declare leaf fields
(`address.city`) rather than `address` if you want to whitelist inside an
object.

## Form requests (typed input)

```rust
#[derive(Deserialize)]
struct StorePost { title: String, body: Option<String> }

impl FormRequest for StorePost {
    fn rules() -> Rules {
        Rules::new()
            .field("title", "required|string|max:200")
            .field("body", "nullable|string")
            .message("title.required", "Give your post a title.")
            .attribute("body", "post body")
    }
    fn authorize(req: &Request) -> bool { req.bearer_token().is_some() } // else 403
}

let input: StorePost = req.validated()?;
```

## Rules

Presence: `required` `nullable` `sometimes` `present` `filled`
`prohibited` `accepted` `declined` `bail` `required_if:f,v,..`
`required_unless:f,v,..` `required_with:a,b` `required_with_all:a,b`
`required_without:a,b` `required_without_all:a,b`

Types: `string` `integer` `numeric` `boolean` `array` `json`

Formats: `email` `url` `uuid` `ip` `ipv4` `ipv6` `date` `alpha`
`alpha_num` `alpha_dash` `ascii` `lowercase` `uppercase` `regex:pattern`
`not_regex:pattern` `digits:n` `digits_between:a,b` `starts_with:a,b`
`ends_with:a,b`

Size (characters for strings, value for numbers, items for arrays):
`min:n` `max:n` `size:n` `between:a,b` `gt:x` `gte:x` `lt:x` `lte:x`
(`x` is a number or another field)

Relations: `in:a,b` `not_in:a,b` `same:field` `different:field`
`confirmed` (`password` vs `password_confirmation`) `distinct`
`after:date|field` `after_or_equal` `before` `before_or_equal`
(dates compare on the `YYYY-MM-DD` part)

Wildcards: `items.*.qty` validates every element; nesting with dots.

Notes:
- Empty strings skip non-presence rules (like Laravel); `nullable` lets
  `null` skip them too.
- Bad rule arguments (`min:abc`) and unknown rules **fail closed**.
- `email` is ASCII-only and deliberately practical, not RFC-exhaustive.
- `regex` runs on the linear-time `regex` engine (no ReDoS). Patterns
  containing `|` or `,`: use `rules::regex("^(a|b)$")` instead of a string.
- Rules for other fields (`same:`, `required_if:`, ...) use absolute
  paths; they do not follow a wildcard index.

## Your own rules

```rust
// per field, inline
Rule::custom(|v| match v.as_str() {
    Some(s) if s.starts_with("ACME-") => Ok(()),
    _ => Err("The :attribute must start with ACME-.".into()),
})

// global, usable from rule strings
extend("even", |ctx| ctx.value.as_i64().is_some_and(|n| n % 2 == 0),
       "The :attribute must be even.");
```

`fornthulani-plugin-orm` (feature `validation`) registers `unique:` and
`exists:` this way.

## Without a request

```rust
let v = Validator::make(json!({"email": "x"}), [("email", "required|email")]);
if let Err(errors) = v.validate() { println!("{}", errors.to_json()); }
```

Status: **not compiled yet** — written without a Rust toolchain. Run
`cargo fmt && cargo clippy --all-targets && cargo test` and report errors.
