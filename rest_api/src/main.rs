//! REST API binary for the Vibes workspace.
//!
//! The binary exposes a Poem web server that publishes an OpenAPI
//! specification and implements three endpoints: a login endpoint, an echo
//! endpoint, and a telemetry ingestion endpoint. The login endpoint returns a
//! mock bearer token that must be supplied to access the other routes.

use poem::{Error, Result, Route, Server, http::StatusCode, listener::TcpListener};
use poem_openapi::auth::Bearer;
use poem_openapi::payload::Json;
use poem_openapi::{Object, OpenApi, OpenApiService, SecurityScheme};
use serde::{Deserialize, Serialize};

/// Mock bearer token returned by the login endpoint and required to access the
/// protected routes. In a production system this would be generated dynamically
/// and validated through a persistence layer, but the constant keeps the fixture
/// simple for testing.
const MOCK_BEARER_TOKEN: &str = "vibes-mock-bearer-token";

/// Security scheme wrapper for the bearer token required by the protected
/// routes. This leverages Poem OpenAPI's [`SecurityScheme`] derive to document
/// the requirement in the generated specification.
#[derive(SecurityScheme)]
#[oai(type = "bearer")]
struct MockTokenAuth(Bearer);

/// Payload accepted and returned by the echo endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, Object)]
struct EchoPayload {
    /// The text that will be echoed back in the response.
    message: String,
}

/// Payload accepted by the login endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, Object)]
struct LoginRequest {
    /// Username supplied by the caller.
    username: String,
    /// Password supplied by the caller.
    password: String,
}

/// Response returned by the login endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, Object)]
struct LoginResponse {
    /// Mock bearer token that must be provided when interacting with the
    /// protected routes.
    token: String,
}

/// Structured data that can be deserialized from a JSON string by the
/// telemetry ingestion endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, Object)]
struct TelemetryRecord {
    /// A human readable message supplied by the caller.
    message: String,
    /// An integer value associated with the telemetry payload.
    count: i32,
    /// A floating point measurement associated with the payload.
    ratio: f64,
}

/// Collection of OpenAPI endpoints exposed by the REST API server.
struct Api;

#[OpenApi]
impl Api {
    /// Issues a mock bearer token that can be used to access the protected
    /// routes. This endpoint does not validate the supplied credentials because
    /// the API acts as a test fixture.
    #[oai(path = "/login", method = "post")]
    async fn login(&self, payload: Json<LoginRequest>) -> Json<LoginResponse> {
        let _ = payload;

        Json(LoginResponse {
            token: MOCK_BEARER_TOKEN.to_string(),
        })
    }

    /// Echoes a JSON payload back to the caller without any modification.
    #[oai(path = "/echo", method = "post")]
    async fn echo(
        &self,
        auth: MockTokenAuth,
        payload: Json<EchoPayload>,
    ) -> Result<Json<EchoPayload>> {
        authorize(&auth)?;

        Ok(Json(payload.0))
    }

    /// Accepts a JSON payload that represents a [`TelemetryRecord`], logs the
    /// content, and returns the structured representation.
    #[oai(path = "/ingest", method = "post")]
    async fn ingest(
        &self,
        auth: MockTokenAuth,
        payload: Json<TelemetryRecord>,
    ) -> Result<Json<TelemetryRecord>> {
        authorize(&auth)?;
        let record = payload.0;

        println!(
            "Received telemetry record: message='{}', count={}, ratio={}",
            record.message, record.count, record.ratio
        );

        Ok(Json(record))
    }
}

/// Validates that the supplied bearer token matches the mock token returned by
/// the login endpoint. Returns an unauthorized error when the token is missing
/// or invalid.
fn authorize(auth: &MockTokenAuth) -> Result<()> {
    if auth.0.token == MOCK_BEARER_TOKEN {
        Ok(())
    } else {
        Err(Error::from_status(StatusCode::UNAUTHORIZED))
    }
}

/// Builds the Poem application with the OpenAPI service and documentation
/// routes.
fn create_app() -> Route {
    let api_service =
        OpenApiService::new(Api, "Vibes REST API", "1.0").server("http://localhost:3000/api");
    let ui = api_service.swagger_ui();
    let spec = api_service.spec_endpoint();

    Route::new()
        .nest("/api", api_service)
        .nest("/docs", ui)
        .at("/openapi.json", spec)
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let app = create_app();

    Server::new(TcpListener::bind("0.0.0.0:3000"))
        .name("vibes-rest-api")
        .run(app)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use poem::test::TestClient;
    use poem::{Endpoint, http::StatusCode};
    use serde_json::json;

    /// Helper that logs in and retrieves the mock bearer token.
    async fn login<E: Endpoint>(client: &TestClient<E>) -> String {
        let response = client
            .post("/api/login")
            .body_json(&LoginRequest {
                username: String::from("user"),
                password: String::from("pass"),
            })
            .send()
            .await;

        response.assert_status_is_ok();
        let token_response: LoginResponse = response.json().await.value().deserialize();

        token_response.token
    }

    /// Ensures the login route returns the mock bearer token.
    #[tokio::test]
    async fn login_returns_mock_token() {
        let client = TestClient::new(create_app());
        let response = client
            .post("/api/login")
            .body_json(&LoginRequest {
                username: String::from("demo"),
                password: String::from("secret"),
            })
            .send()
            .await;

        response.assert_status_is_ok();
        let body = response.json().await;
        let token: LoginResponse = body.value().deserialize();

        assert_eq!(token.token, MOCK_BEARER_TOKEN);
    }

    /// Ensures the echo route mirrors the input payload.
    #[tokio::test]
    async fn echo_round_trips_payload() {
        let client = TestClient::new(create_app());
        let token = login(&client).await;
        let payload = EchoPayload {
            message: String::from("Hello, Vibes!"),
        };

        let response = client
            .post("/api/echo")
            .header("Authorization", format!("Bearer {}", token))
            .body_json(&payload)
            .send()
            .await;

        response.assert_status_is_ok();
        response.assert_json(&payload).await;
    }

    /// Ensures the echo route rejects requests without the bearer token.
    #[tokio::test]
    async fn echo_requires_token() {
        let client = TestClient::new(create_app());
        let payload = EchoPayload {
            message: String::from("Hello, Vibes!"),
        };

        let response = client.post("/api/echo").body_json(&payload).send().await;

        response.assert_status(StatusCode::UNAUTHORIZED);
    }

    /// Ensures the echo route rejects requests with an invalid bearer token.
    #[tokio::test]
    async fn echo_rejects_invalid_token() {
        let client = TestClient::new(create_app());
        let payload = EchoPayload {
            message: String::from("Hello, Vibes!"),
        };

        let response = client
            .post("/api/echo")
            .header("Authorization", "Bearer wrong-token")
            .body_json(&payload)
            .send()
            .await;

        response.assert_status(StatusCode::UNAUTHORIZED);
    }

    /// Ensures the ingestion route parses a JSON payload and returns the
    /// structured telemetry data.
    #[tokio::test]
    async fn ingest_parses_json_payload() {
        let client = TestClient::new(create_app());
        let token = login(&client).await;
        let telemetry_record = TelemetryRecord {
            message: String::from("Temperature reading"),
            count: 3,
            ratio: 0.618,
        };

        let response = client
            .post("/api/ingest")
            .header("Authorization", format!("Bearer {}", token))
            .body_json(&telemetry_record)
            .send()
            .await;

        response.assert_status_is_ok();
        let json_body = response.json().await;
        let parsed: TelemetryRecord = json_body.value().deserialize();

        assert_eq!(parsed.message, "Temperature reading");
        assert_eq!(parsed.count, 3);
        assert!((parsed.ratio - 0.618).abs() < f64::EPSILON);
    }

    /// Validates that invalid JSON payloads produce a bad request error.
    #[tokio::test]
    async fn ingest_rejects_invalid_json() {
        let client = TestClient::new(create_app());
        let token = login(&client).await;

        let response = client
            .post("/api/ingest")
            .header("Authorization", format!("Bearer {}", token))
            .body_json(&json!({
                "message": "missing fields",
            }))
            .send()
            .await;

        response.assert_status(StatusCode::BAD_REQUEST);
        let body = response
            .0
            .into_body()
            .into_string()
            .await
            .expect("body should be readable");
        assert!(!body.is_empty(), "error response should include a message");
    }

    /// Ensures the ingestion route rejects requests when the bearer token is
    /// missing.
    #[tokio::test]
    async fn ingest_requires_token() {
        let client = TestClient::new(create_app());
        let telemetry_record = TelemetryRecord {
            message: String::from("Temperature reading"),
            count: 3,
            ratio: 0.618,
        };

        let response = client
            .post("/api/ingest")
            .body_json(&telemetry_record)
            .send()
            .await;

        response.assert_status(StatusCode::UNAUTHORIZED);
    }
}
