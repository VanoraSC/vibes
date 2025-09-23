#![doc = "Integration tests validating the generated Protocol Buffer types."]

use prost::Message as _;

use proto_gen::generated::echo::Message;

/// Ensures that the generated `Message` type can be serialized and deserialized
/// without losing information.
#[test]
fn message_round_trip_serialization() {
    let payload = vec![0_u8, 1, 2, 3, 4, 5];
    let message = Message {
        uid: "123e4567-e89b-12d3-a456-426614174000".to_string(),
        data: payload.clone(),
        data_length: payload.len() as u64,
    };

    let encoded = message.encode_to_vec();
    let decoded = Message::decode(encoded.as_slice()).expect("Protobuf decoding must succeed");

    assert_eq!(decoded.uid, message.uid);
    assert_eq!(decoded.data, payload);
    assert_eq!(decoded.data_length as usize, decoded.data.len());
}
