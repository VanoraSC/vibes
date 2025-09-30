//! REST API binary for the Vibes workspace.
//!
//! The binary exposes a Poem web server that publishes an OpenAPI
//! specification and implements two endpoints: an echo endpoint and a
//! telemetry ingestion endpoint.

use poem::{Error, Result as PoemResult, Route, Server, http::StatusCode, listener::TcpListener};
use poem_openapi::payload::Json;
use poem_openapi::{Object, OpenApi, OpenApiService};
use serde::{Deserialize, Serialize};

/// Payload accepted and returned by the echo endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, Object)]
struct EchoPayload {
    /// The text that will be echoed back in the response.
    message: String,
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
    /// Echoes a JSON payload back to the caller without any modification.
    #[oai(path = "/echo", method = "post")]
    async fn echo(&self, payload: Json<EchoPayload>) -> Json<EchoPayload> {
        Json(payload.0)
    }

    /// Accepts a JSON string, parses it into a [`TelemetryRecord`], logs the
    /// content, and returns the structured representation.
    #[oai(path = "/ingest", method = "post")]
    async fn ingest(&self, payload: Json<String>) -> PoemResult<Json<TelemetryRecord>> {
        let record: TelemetryRecord = serde_json::from_str(&payload.0).map_err(|error| {
            Error::from_string(
                format!("Invalid JSON payload: {error}"),
                StatusCode::BAD_REQUEST,
            )
        })?;

        println!(
            "Received telemetry record: message='{}', count={}, ratio={}",
            record.message, record.count, record.ratio
        );

        Ok(Json(record))
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
    use poem::http::StatusCode;
    use poem::test::TestClient;
    use serde_json::json;

    /// Ensures the echo route mirrors the input payload.
    #[tokio::test]
    async fn echo_round_trips_payload() {
        let client = TestClient::new(create_app());
        let payload = EchoPayload {
            message: String::from("Hello, Vibes!"),
        };

        let response = client.post("/api/echo").body_json(&payload).send().await;

        response.assert_status_is_ok();
        response.assert_json(&payload).await;
    }

    /// Ensures the ingestion route parses a JSON string and returns the
    /// structured telemetry data.
    #[tokio::test]
    async fn ingest_parses_json_string() {
        let client = TestClient::new(create_app());
        let telemetry_json = json!({
            "message": "Temperature reading",
            "count": 3,
            "ratio": 0.618
        })
        .to_string();

        let response = client
            .post("/api/ingest")
            .body_json(&telemetry_json)
            .send()
            .await;

        response.assert_status_is_ok();
        let json_body = response.json().await;
        let parsed: TelemetryRecord = json_body.value().deserialize();

        assert_eq!(parsed.message, "Temperature reading");
        assert_eq!(parsed.count, 3);
        assert!((parsed.ratio - 0.618).abs() < f64::EPSILON);
    }

    /// Validates that invalid JSON strings produce a bad request error.
    #[tokio::test]
    async fn ingest_rejects_invalid_json() {
        let client = TestClient::new(create_app());

        let response = client
            .post("/api/ingest")
            .body_json(&"not-json".to_string())
            .send()
            .await;

        response.assert_status(StatusCode::BAD_REQUEST);
        let body = response
            .0
            .into_body()
            .into_string()
            .await
            .expect("body should be readable");
        assert!(body.contains("Invalid JSON payload"));
    }
}
