use std::{future::Future, net::SocketAddr};

use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status, async_trait, transport::Server};

use proto_gen::generated::echo::message_service::Message;
use proto_gen::generated::echo::message_service::message_service_server::{
    MessageService, MessageServiceServer,
};

/// Server implementation that validates incoming payloads and echoes them with
/// additional context.
#[derive(Debug, Default, Clone, Copy)]
pub struct MessageEchoService;

impl MessageEchoService {
    /// Creates a new instance of the echo service implementation.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl MessageService for MessageEchoService {
    /// Processes the [`Message`] received from the client and returns a
    /// response that includes the original payload augmented with server-side
    /// metadata.
    async fn echo(&self, request: Request<Message>) -> Result<Response<Message>, Status> {
        let reply = build_response_message(request.into_inner())?;
        Ok(Response::new(reply))
    }
}

/// Builds a ready-to-serve gRPC service for the [`MessageEchoService`].
#[must_use]
pub fn message_service() -> MessageServiceServer<MessageEchoService> {
    MessageServiceServer::new(MessageEchoService::new())
}

/// Runs the gRPC server on the provided socket address until the process is
/// terminated.
///
/// This helper is intended for production entrypoints where no graceful
/// shutdown signal is required.
pub async fn serve(addr: SocketAddr) -> Result<(), tonic::transport::Error> {
    Server::builder()
        .add_service(message_service())
        .serve(addr)
        .await
}

/// Runs the gRPC server on the provided socket address and awaits the supplied
/// shutdown signal for graceful termination.
///
/// The shutdown future resolves when the server should start terminating
/// active connections and stop accepting new requests.
pub async fn serve_with_shutdown<F>(
    addr: SocketAddr,
    shutdown: F,
) -> Result<(), tonic::transport::Error>
where
    F: Future<Output = ()> + Send + 'static,
{
    Server::builder()
        .add_service(message_service())
        .serve_with_shutdown(addr, shutdown)
        .await
}

/// Runs the gRPC server using an already bound TCP listener and the supplied
/// shutdown signal.
///
/// This helper is particularly useful for integration tests where binding to a
/// random port is necessary.
pub async fn serve_with_listener_shutdown<F>(
    listener: TcpListener,
    shutdown: F,
) -> Result<(), tonic::transport::Error>
where
    F: Future<Output = ()> + Send + 'static,
{
    let incoming = TcpListenerStream::new(listener);
    Server::builder()
        .add_service(message_service())
        .serve_with_incoming_shutdown(incoming, shutdown)
        .await
}

/// Parses the incoming [`Message`], validates its payload, and constructs the
/// server's response message.
#[allow(clippy::result_large_err)]
fn build_response_message(request: Message) -> Result<Message, Status> {
    let Message {
        uid,
        data,
        data_length,
    } = request;

    let payload = decode_ascii_payload(&data, data_length)?;
    let reply_text = format!("Server reply: {payload}");
    let reply_bytes = reply_text.into_bytes();
    let reply_length = u64::try_from(reply_bytes.len())
        .map_err(|_| Status::internal("reply payload length exceeds u64"))?;

    Ok(Message {
        uid,
        data: reply_bytes,
        data_length: reply_length,
    })
}

/// Decodes the ASCII payload stored in the [`Message`] body.
#[allow(clippy::result_large_err)]
fn decode_ascii_payload(data: &[u8], declared_length: u64) -> Result<String, Status> {
    let expected_length = usize::try_from(declared_length)
        .map_err(|_| Status::invalid_argument("declared data length exceeds usize"))?;

    if data.len() < expected_length {
        return Err(Status::invalid_argument(
            "payload shorter than declared data length",
        ));
    }

    let ascii_slice = &data[..expected_length];
    if !ascii_slice.is_ascii() {
        return Err(Status::invalid_argument("payload is not ASCII encoded"));
    }

    Ok(std::str::from_utf8(ascii_slice)
        .expect("ASCII input is always valid UTF-8")
        .to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_ascii_payload_rejects_short_payloads() {
        let result = decode_ascii_payload(b"hi", 5);
        assert!(result.is_err());
    }

    #[test]
    fn decode_ascii_payload_rejects_non_ascii() {
        let result = decode_ascii_payload(&[0xff, 0x00], 1);
        assert!(result.is_err());
    }

    #[test]
    fn build_response_appends_reply_text() {
        let request = Message {
            uid: "123".to_string(),
            data: b"ping".to_vec(),
            data_length: 4,
        };

        let response = build_response_message(request).expect("valid message");
        assert_eq!(response.uid, "123");
        let text = String::from_utf8(response.data).expect("valid UTF-8");
        assert_eq!(text, "Server reply: ping");
        assert_eq!(response.data_length, text.len() as u64);
    }
}
