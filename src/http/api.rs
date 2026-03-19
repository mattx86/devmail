use crate::model::{EmailDetail, EmailSummary};
use crate::store::SharedStore;
use axum::{
    body::Body,
    extract::{Form, Path, State},
    http::{header, Request, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

const INDEX_HTML: &str = include_str!("../../assets/index.html");

const LOGIN_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>devmail — Sign in</title>
  <style>
    *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      min-height: 100vh;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      background: #f3f4f6;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
    }
    .brand { font-size: 22px; font-weight: 700; color: #1a2e4a; letter-spacing: 0.02em; margin-bottom: 24px; }
    .card {
      background: #fff;
      border-radius: 12px;
      box-shadow: 0 4px 24px rgba(0,0,0,0.10);
      padding: 32px 32px 28px;
      width: 320px;
      display: flex;
      flex-direction: column;
      gap: 18px;
    }
    h2 { font-size: 16px; font-weight: 600; color: #111827; }
    .error {
      font-size: 13px;
      color: #b91c1c;
      background: #fee2e2;
      border: 1px solid #fca5a5;
      border-radius: 6px;
      padding: 8px 12px;
      display: none;
    }
    .error.show { display: block; }
    .field { display: flex; flex-direction: column; gap: 6px; }
    label { font-size: 13px; font-weight: 500; color: #374151; }
    input[type=password] {
      padding: 9px 11px;
      border: 1px solid #d1d5db;
      border-radius: 6px;
      font-size: 14px;
      outline: none;
      width: 100%;
      transition: border-color 0.15s, box-shadow 0.15s;
    }
    input[type=password]:focus { border-color: #3b82f6; box-shadow: 0 0 0 3px rgba(59,130,246,0.15); }
    button[type=submit] {
      padding: 10px;
      background: #1a2e4a;
      color: #fff;
      border: none;
      border-radius: 6px;
      font-size: 14px;
      font-weight: 600;
      cursor: pointer;
      width: 100%;
      transition: background 0.15s;
    }
    button[type=submit]:hover { background: #243d63; }
  </style>
</head>
<body>
  <div class="brand">devmail</div>
  <div class="card">
    <h2>Sign in</h2>
    __ERROR__
    <form method="POST" action="/login">
      <div class="field">
        <label for="pw">Password</label>
        <input type="password" id="pw" name="password" autofocus autocomplete="current-password">
      </div>
      <br>
      <button type="submit">Sign in</button>
    </form>
  </div>
</body>
</html>"#;

#[derive(Clone)]
pub struct AppState {
    pub store: SharedStore,
    pub auth: Option<Auth>,
    pub smtp_hint: String,
}

#[derive(Clone)]
pub struct Auth {
    pub password: String,
    pub session_token: Arc<String>,
}

#[derive(Deserialize)]
struct LoginForm {
    password: String,
}

pub fn build_router(store: SharedStore, password: Option<String>, smtp_hint: String) -> Router {
    let auth = password.map(|pw| Auth {
        password: pw,
        session_token: Arc::new(Uuid::new_v4().to_string()),
    });
    let state = AppState { store, auth, smtp_hint };

    Router::new()
        .route("/", get(serve_index))
        .route("/api/emails", get(list_emails))
        .route("/api/emails/:id", get(get_email))
        .route("/api/emails/:id/read", post(mark_read))
        .route("/api/emails/:id", delete(delete_email))
        .route("/api/emails/:id/raw", get(get_raw))
        .route(
            "/api/emails/:id/attachments/:filename",
            get(download_attachment),
        )
        .route("/login", get(serve_login).post(handle_login))
        .route("/logout", post(handle_logout))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state)
}

fn is_authenticated(auth: &Auth, cookie_header: &str) -> bool {
    cookie_header.split(';').any(|c| {
        let c = c.trim();
        c.starts_with("devmail_session=")
            && &c["devmail_session=".len()..] == auth.session_token.as_str()
    })
}

async fn auth_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let path = request.uri().path().to_owned();

    // Login and logout endpoints are always accessible.
    if path == "/login" || path == "/logout" {
        return next.run(request).await;
    }

    if let Some(auth) = &state.auth {
        let cookie_header = request
            .headers()
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !is_authenticated(auth, cookie_header) {
            if path.starts_with("/api/") {
                return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
            }
            return Html(LOGIN_HTML.replace("__ERROR__", "")).into_response();
        }
    }

    next.run(request).await
}

async fn serve_login(State(state): State<AppState>) -> Response {
    if state.auth.is_none() {
        return Redirect::to("/").into_response();
    }
    Html(LOGIN_HTML.replace("__ERROR__", "")).into_response()
}

async fn handle_login(
    State(state): State<AppState>,
    Form(form): Form<LoginForm>,
) -> Response {
    if let Some(auth) = &state.auth {
        if form.password == auth.password {
            let cookie = format!(
                "devmail_session={}; HttpOnly; SameSite=Strict; Path=/",
                auth.session_token
            );
            return Response::builder()
                .status(StatusCode::SEE_OTHER)
                .header(header::LOCATION, "/")
                .header(header::SET_COOKIE, cookie)
                .body(Body::from(""))
                .unwrap();
        }
        let html = LOGIN_HTML.replace(
            "__ERROR__",
            r#"<div class="error show">Incorrect password. Please try again.</div>"#,
        );
        return Html(html).into_response();
    }
    Redirect::to("/").into_response()
}

async fn handle_logout() -> Response {
    Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, "/")
        .header(
            header::SET_COOKIE,
            "devmail_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
        )
        .body(Body::from(""))
        .unwrap()
}

async fn serve_index(State(state): State<AppState>) -> Html<String> {
    let auth_flag = if state.auth.is_some() { "true" } else { "false" };
    Html(INDEX_HTML
        .replace("__AUTH_ENABLED__", auth_flag)
        .replace("__SMTP_HINT__", &state.smtp_hint))
}

async fn list_emails(State(state): State<AppState>) -> Json<Vec<EmailSummary>> {
    let store = state.store.read().await;
    Json(store.list())
}

async fn get_email(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<EmailDetail>, StatusCode> {
    let store = state.store.read().await;
    store
        .get(id)
        .map(|e| Json(EmailDetail::from(e)))
        .ok_or(StatusCode::NOT_FOUND)
}

async fn mark_read(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    let mut store = state.store.write().await;
    if store.mark_read(id) {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn delete_email(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    let mut store = state.store.write().await;
    if store.delete(id) {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn get_raw(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let raw = {
        let store = state.store.read().await;
        store
            .get(id)
            .map(|e| e.raw.clone())
            .ok_or(StatusCode::NOT_FOUND)?
    };

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(raw))
        .unwrap())
}

async fn download_attachment(
    State(state): State<AppState>,
    Path((id, filename)): Path<(Uuid, String)>,
) -> Result<impl IntoResponse, StatusCode> {
    let (content_type, data) = {
        let store = state.store.read().await;
        let att = store
            .get_attachment(id, &filename)
            .ok_or(StatusCode::NOT_FOUND)?;
        (att.content_type.clone(), att.data.clone())
    };

    let disposition = format!("attachment; filename=\"{}\"", filename);

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_DISPOSITION, disposition)
        .body(Body::from(data))
        .unwrap())
}
