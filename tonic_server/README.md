# tonic_server

The `tonic_server` crate hosts a Tonic-based RPC server for processing
`message.Message` payloads defined in `message.proto`. The server validates
the declared payload length, interprets the bytes as an ASCII string, and
responds with another `Message` that echoes the payload with additional text
indicating it is the server's reply.

The crate exposes helpers for running the gRPC server as well as utilities that
are exercised by the included unit tests.
