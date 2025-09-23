//! gRPC server utilities for processing `message.proto` payloads.
//!
//! The crate exposes a [`MessageService`] implementation that validates the
//! ASCII payload embedded in [`Message`] requests and responds with a modified
//! payload indicating the server processed the message.

//! Thin crate root that exposes the server module and re-exports its public
//! helpers. The actual server implementation lives in `message_echo_server.rs`.

pub mod message_echo_server;

pub use message_echo_server::{
    MessageEchoService, message_service, serve, serve_with_listener_shutdown, serve_with_shutdown,
};
