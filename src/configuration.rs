use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};
use subtle::ConstantTimeEq;

use crate::contracts::{Failure, FailureCode};

#[derive(Clone)]
pub struct ConfigurationState {
    pool: SqlitePool,
    username: String,
    password: String,
}

#[derive(Debug, Clone, Serialize, FromRow, JsonSchema)]
pub struct Configuration {
    pub id: i64,
    pub name: String,
    pub gmail_query: Option<String>,
    pub inbox_only: bool,
    pub include_spam_trash: bool,
    pub max_results: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConfigurationInput {
    pub name: String,
    pub gmail_query: Option<String>,
    #[serde(default)]
    pub inbox_only: bool,
    #[serde(default)]
    pub include_spam_trash: bool,
    pub max_results: u32,
}

#[derive(Debug)]
struct ApiError(StatusCode, Failure);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(self.1)).into_response()
    }
}

impl ConfigurationState {
    pub async fn new(
        database_url: &str,
        username: String,
        password: String,
    ) -> anyhow::Result<Self> {
        let pool = SqlitePool::connect(database_url).await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(Self {
            pool,
            username,
            password,
        })
    }

    pub fn pool(&self) -> SqlitePool {
        self.pool.clone()
    }
}

pub fn router(state: ConfigurationState) -> Router {
    Router::new()
        .route("/configurations", post(create).get(list))
        .route(
            "/configurations/{id}",
            get(get_one).put(replace).delete(remove),
        )
        .with_state(state)
}

async fn create(
    State(state): State<ConfigurationState>,
    headers: HeaderMap,
    Json(input): Json<ConfigurationInput>,
) -> Result<(StatusCode, Json<Configuration>), ApiError> {
    authorize(&state, &headers)?;
    validate(&input)?;
    let result = sqlx::query(
        "INSERT INTO configurations (name, gmail_query, inbox_only, include_spam_trash, max_results) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(input.name.trim())
    .bind(input.gmail_query)
    .bind(input.inbox_only)
    .bind(input.include_spam_trash)
    .bind(input.max_results)
    .execute(&state.pool)
    .await
    .map_err(db_error)?;
    let value = fetch(&state.pool, result.last_insert_rowid()).await?;
    Ok((StatusCode::CREATED, Json(value)))
}

async fn list(
    State(state): State<ConfigurationState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Configuration>>, ApiError> {
    authorize(&state, &headers)?;
    let values = sqlx::query_as::<_, Configuration>("SELECT * FROM configurations ORDER BY id")
        .fetch_all(&state.pool)
        .await
        .map_err(db_error)?;
    Ok(Json(values))
}

async fn get_one(
    State(state): State<ConfigurationState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<Configuration>, ApiError> {
    authorize(&state, &headers)?;
    fetch(&state.pool, id).await.map(Json)
}

async fn replace(
    State(state): State<ConfigurationState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(input): Json<ConfigurationInput>,
) -> Result<Json<Configuration>, ApiError> {
    authorize(&state, &headers)?;
    validate(&input)?;
    let result = sqlx::query(
        "UPDATE configurations SET name = ?, gmail_query = ?, inbox_only = ?, include_spam_trash = ?, max_results = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(input.name.trim())
    .bind(input.gmail_query)
    .bind(input.inbox_only)
    .bind(input.include_spam_trash)
    .bind(input.max_results)
    .bind(id)
    .execute(&state.pool)
    .await
    .map_err(db_error)?;
    if result.rows_affected() == 0 {
        return Err(not_found(id));
    }
    fetch(&state.pool, id).await.map(Json)
}

async fn remove(
    State(state): State<ConfigurationState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    authorize(&state, &headers)?;
    let result = sqlx::query("DELETE FROM configurations WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(db_error)?;
    if result.rows_affected() == 0 {
        return Err(not_found(id));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn fetch(pool: &SqlitePool, id: i64) -> Result<Configuration, ApiError> {
    sqlx::query_as::<_, Configuration>("SELECT * FROM configurations WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(db_error)?
        .ok_or_else(|| not_found(id))
}

fn validate(input: &ConfigurationInput) -> Result<(), ApiError> {
    if input.name.trim().is_empty() {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            Failure::invalid("name", "name must not be empty"),
        ));
    }
    if !(1..=100).contains(&input.max_results) {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            Failure::invalid("max_results", "max_results must be between 1 and 100"),
        ));
    }
    Ok(())
}

fn authorize(state: &ConfigurationState, headers: &HeaderMap) -> Result<(), ApiError> {
    let supplied = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Basic "))
        .and_then(|value| STANDARD.decode(value).ok())
        .and_then(|value| String::from_utf8(value).ok());
    let expected = format!("{}:{}", state.username, state.password);
    let valid = supplied.as_deref().is_some_and(|value| {
        let supplied_hash = Sha256::digest(value.as_bytes());
        let expected_hash = Sha256::digest(expected.as_bytes());
        bool::from(supplied_hash.ct_eq(&expected_hash))
    });
    if valid {
        Ok(())
    } else {
        Err(ApiError(
            StatusCode::UNAUTHORIZED,
            Failure::new(
                FailureCode::Unauthorized,
                "valid Basic authentication is required",
                false,
            ),
        ))
    }
}

fn db_error(error: sqlx::Error) -> ApiError {
    let conflict = matches!(&error, sqlx::Error::Database(db) if db.is_unique_violation());
    if conflict {
        ApiError(
            StatusCode::CONFLICT,
            Failure::new(
                FailureCode::Conflict,
                "configuration name already exists",
                false,
            ),
        )
    } else {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            Failure::new(
                FailureCode::StorageError,
                "configuration storage failed",
                true,
            ),
        )
    }
}

fn not_found(id: i64) -> ApiError {
    ApiError(
        StatusCode::NOT_FOUND,
        Failure::new(FailureCode::NotFound, "configuration was not found", false)
            .with_context("id", id.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;

    #[test]
    fn validates_configuration_contract() {
        let invalid = ConfigurationInput {
            name: " ".into(),
            gmail_query: None,
            inbox_only: false,
            include_spam_trash: false,
            max_results: 101,
        };
        assert!(validate(&invalid).is_err());
    }

    #[tokio::test]
    async fn crud_requires_auth_and_persists_non_secret_fields() {
        let directory = tempfile::tempdir().unwrap();
        let database_url = format!(
            "sqlite://{}?mode=rwc",
            directory.path().join("test.db").display()
        );
        let state = ConfigurationState::new(&database_url, "operator".into(), "password".into())
            .await
            .unwrap();
        let app = router(state);

        let unauthorized = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/configurations")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let payload = serde_json::json!({
            "name": "inbox",
            "gmail_query": "is:unread",
            "inbox_only": true,
            "include_spam_trash": false,
            "max_results": 25
        });
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/configurations")
                    .header(
                        header::AUTHORIZATION,
                        format!("Basic {}", STANDARD.encode("operator:password")),
                    )
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let stored: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(stored["name"], "inbox");
        assert!(stored.get("password").is_none());
    }

    #[tokio::test]
    async fn replace_delete_and_name_conflict_have_stable_routes() {
        let directory = tempfile::tempdir().unwrap();
        let database_url = format!(
            "sqlite://{}?mode=rwc",
            directory.path().join("test.db").display()
        );
        let state = ConfigurationState::new(&database_url, "operator".into(), "password".into())
            .await
            .unwrap();
        let app = router(state);
        let authorization = format!("Basic {}", STANDARD.encode("operator:password"));
        let payload = |name: &str, max_results: u32| {
            serde_json::json!({
                "name": name,
                "gmail_query": null,
                "inbox_only": false,
                "include_spam_trash": false,
                "max_results": max_results
            })
            .to_string()
        };

        for name in ["first", "second"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/configurations")
                        .header(header::AUTHORIZATION, &authorization)
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(payload(name, 10)))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::CREATED);
        }

        let replaced = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/configurations/1")
                    .header(header::AUTHORIZATION, &authorization)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(payload("updated", 50)))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(replaced.status(), StatusCode::OK);
        let body = to_bytes(replaced.into_body(), 64 * 1024).await.unwrap();
        let stored: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(stored["name"], "updated");
        assert_eq!(stored["max_results"], 50);

        let conflict = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/configurations/1")
                    .header(header::AUTHORIZATION, &authorization)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(payload("second", 10)))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(conflict.status(), StatusCode::CONFLICT);

        let deleted = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/configurations/1")
                    .header(header::AUTHORIZATION, &authorization)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

        let missing = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/configurations/1")
                    .header(header::AUTHORIZATION, authorization)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    }
}
