use fornthulani::{Params, Request, Response, Router, Server, ServerHandle, Status, ThulaniError};
use fornthulani_plugin_validate::{FormRequest, Rules, ValidateExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

#[derive(Serialize, Deserialize)]
struct NewUser {
    name: String,
    email: String,
    age: Option<u8>,
    newsletter: Option<bool>,
}

impl FormRequest for NewUser {
    fn rules() -> Rules {
        Rules::new()
            .field("name", "required|string|min:2|max:50")
            .field("email", "required|email")
            .field("age", "nullable|integer|between:18,120")
            .field("newsletter", "nullable|boolean")
    }

    fn authorize(req: &Request) -> bool {
        req.header("x-allow") != Some("no")
    }
}

fn register(req: &Request, _: &Params) -> Result<Response, ThulaniError> {
    let u: NewUser = req.validated()?;
    Ok(Response::json(Status::CREATED, &u))
}

fn ad_hoc(req: &Request, _: &Params) -> Result<Response, ThulaniError> {
    let v = req.validate([("q", "required|string|min:2")])?;
    Ok(Response::text(Status::OK, v.str("q").unwrap_or("")))
}

fn start() -> (String, ServerHandle, thread::JoinHandle<()>) {
    let mut router = Router::new();
    router.post("/users", register);
    router.get("/search", ad_hoc);
    let probe = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);
    let (h, j) = Server::new(router).spawn(&addr.to_string()).unwrap();
    thread::sleep(Duration::from_millis(50));
    (format!("http://{addr}"), h, j)
}

#[test]
fn valid_json_is_created() {
    let (base, h, j) = start();
    let res = reqwest::blocking::Client::new()
        .post(format!("{base}/users"))
        .json(&json!({"name": "Ada", "email": "ada@x.io", "age": 36, "admin": true}))
        .send()
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().unwrap();
    assert_eq!(body["name"], "Ada");
    assert_eq!(body["age"], 36);
    h.shutdown();
    j.join().unwrap();
}

#[test]
fn invalid_json_gets_422_with_field_errors() {
    let (base, h, j) = start();
    let res = reqwest::blocking::Client::new()
        .post(format!("{base}/users"))
        .json(&json!({"name": "A", "email": "nope", "age": 5}))
        .send()
        .unwrap();
    assert_eq!(res.status(), 422);
    let body: serde_json::Value = res.json().unwrap();
    assert!(
        body["errors"]["name"][0]
            .as_str()
            .unwrap()
            .contains("at least 2")
    );
    assert!(
        body["errors"]["email"][0]
            .as_str()
            .unwrap()
            .contains("valid email")
    );
    assert!(
        body["errors"]["age"][0]
            .as_str()
            .unwrap()
            .contains("between 18 and 120")
    );
    assert!(body["message"].as_str().unwrap().contains("more errors"));
    h.shutdown();
    j.join().unwrap();
}

#[test]
fn form_encoded_input_is_coerced() {
    let (base, h, j) = start();
    let res = reqwest::blocking::Client::new()
        .post(format!("{base}/users"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body("name=Ada&email=ada%40x.io&age=36&newsletter=1")
        .send()
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().unwrap();
    assert_eq!(body["age"], 36);
    assert_eq!(body["newsletter"], true);
    h.shutdown();
    j.join().unwrap();
}

#[test]
fn query_string_is_validated_and_authorize_gives_403() {
    let (base, h, j) = start();
    let c = reqwest::blocking::Client::new();
    assert_eq!(
        c.get(format!("{base}/search?q=hello"))
            .send()
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        c.get(format!("{base}/search?q=h")).send().unwrap().status(),
        422
    );
    assert_eq!(
        c.get(format!("{base}/search")).send().unwrap().status(),
        422
    );

    let res = c
        .post(format!("{base}/users"))
        .header("x-allow", "no")
        .json(&json!({"name": "Ada", "email": "ada@x.io"}))
        .send()
        .unwrap();
    assert_eq!(res.status(), 403);
    h.shutdown();
    j.join().unwrap();
}

#[test]
fn bad_bodies_are_client_errors() {
    let (base, h, j) = start();
    let c = reqwest::blocking::Client::new();
    let res = c
        .post(format!("{base}/users"))
        .header("content-type", "application/json")
        .body("{ nope")
        .send()
        .unwrap();
    assert_eq!(res.status(), 400);
    let res = c
        .post(format!("{base}/users"))
        .header("content-type", "application/json")
        .body("[1,2]")
        .send()
        .unwrap();
    assert_eq!(res.status(), 400);
    let res = c
        .post(format!("{base}/users"))
        .header("content-type", "text/plain")
        .body("hello")
        .send()
        .unwrap();
    assert_eq!(res.status(), 415);
    h.shutdown();
    j.join().unwrap();
}
