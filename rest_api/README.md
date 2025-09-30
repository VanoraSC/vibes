# rest_api

This crate hosts a Poem based REST API that exposes:

* A login route that returns a mock bearer token for subsequent requests.
* An echo route that reflects the payload back to the caller when the bearer
  token is supplied.
* A telemetry ingestion route that parses structured JSON telemetry and returns
  the parsed representation when authorized.

The API publishes an OpenAPI specification and includes unit tests
demonstrating the primary interactions.
