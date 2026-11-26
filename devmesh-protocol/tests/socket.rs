use devmesh_protocol::socket::{accept_key, Socket};
#[test]
fn websocket_accept_matches_rfc6455() {
    assert_eq!(
        accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
        "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
    );
}
#[tokio::test]
async fn masked_client_and_server_exchange_json_over_a_stream() {
    let (a, b) = tokio::io::duplex(4096);
    let mut client = Socket::new(a, false);
    let mut server = Socket::new(b, true);
    client.send("{\"type\":\"ping\"}").await.unwrap();
    assert_eq!(
        server.receive().await.unwrap().unwrap(),
        "{\"type\":\"ping\"}"
    );
    server.send("{\"type\":\"pong\"}").await.unwrap();
    assert_eq!(
        client.receive().await.unwrap().unwrap(),
        "{\"type\":\"pong\"}"
    );
}
