use super::*;
#[tokio::test]
async fn memory_is_deterministic_and_query_aware() {
    let client = Client::memory();
    assert_eq!(client.list(String::new()).await.unwrap().len(), 4);
    assert_eq!(
        client.list("CONNECTORS".into()).await.unwrap()[0].id,
        "connectors"
    );
    assert!(client.list("not present".into()).await.unwrap().is_empty());
}
#[test]
fn example_network_configuration_is_explicitly_local_and_path_bound() {
    for value in [
        "https://127.0.0.1/records",
        "http://example.com/records",
        "http://localhost/records",
        "http://127.0.0.1/other",
        "http://user:secret@127.0.0.1/records",
        "http://127.0.0.1/records?q=x",
        "http://127.0.0.1/records#fragment",
    ] {
        assert!(
            matches!(Client::http(value), Err(Error::InvalidEndpoint)),
            "{value}"
        );
    }
    assert!(Client::http("http://127.0.0.1:8082/records").is_ok());
    assert!(Client::http("http://[::1]:8082/records").is_ok());
}
