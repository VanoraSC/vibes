use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tonic_server::{Message, MessageServiceClient, serve_with_listener_shutdown};

/// Verifies that the gRPC endpoint echoes incoming messages with server metadata.
#[tokio::test]
async fn echo_endpoint_returns_augmented_message() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let addr = listener.local_addr().expect("listener address");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let server = tokio::spawn(async move {
        serve_with_listener_shutdown(listener, async {
            let _ = shutdown_rx.await;
        })
        .await
        .expect("server should run cleanly");
    });

    let mut client = MessageServiceClient::connect(format!("http://{addr}"))
        .await
        .expect("connect client");

    let payload = b"ping".to_vec();
    let request = Message {
        uid: "123e4567-e89b-12d3-a456-426614174000".to_string(),
        data: payload.clone(),
        data_length: payload.len() as u64,
    };

    let response = client
        .echo(request)
        .await
        .expect("echo response")
        .into_inner();
    let response_text = String::from_utf8(response.data.clone()).expect("utf8 response");
    assert_eq!(response_text, "Server reply: ping");
    assert_eq!(response.uid, "123e4567-e89b-12d3-a456-426614174000");
    assert_eq!(response.data_length, response.data.len() as u64);

    shutdown_tx.send(()).expect("send shutdown signal");
    server.await.expect("await server");
}
