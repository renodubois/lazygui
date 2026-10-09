use super::*;
use crate::test_support::http::Server;

#[tokio::test]
async fn decodes_real_http_and_encodes_queries_without_leaking_transport() {
    let server = Server::respond(
        "200 OK",
        r#"{"items":[{"id":"a","title":"Alpha","description":"Details"}]}"#,
        "",
    );
    let adapter = Http::new(&server.url).unwrap();
    let records = adapter.list("a & b".into()).await.unwrap();
    assert_eq!(records[0].id, "a");
    let request = server.request.lock().unwrap();
    assert!(
        request.starts_with("GET /records?q=a+%26+b HTTP/1.1"),
        "{request}"
    );
    assert!(!request.to_ascii_lowercase().contains("authorization:"));
}
#[tokio::test]
async fn malformed_duplicate_and_empty_id_records_are_rejected() {
    for body in [
        "not JSON",
        r#"{"items":[{"id":"","title":"A","description":""}]}"#,
        r#"{"items":[{"id":"a","title":"","description":""}]}"#,
        r#"{"items":[{"id":"a","title":"A","description":""},{"id":"a","title":"B","description":""}]}"#,
    ] {
        let server = Server::respond("200 OK", body, "");
        assert_eq!(
            Http::new(&server.url).unwrap().list(String::new()).await,
            Err(Error::InvalidResponse)
        );
    }
}
#[tokio::test]
async fn redirects_and_status_failures_remain_typed_without_following_location() {
    let server = Server::respond(
        "302 Found",
        "",
        "Location: http://example.invalid/secret\r\n",
    );
    assert_eq!(
        Http::new(&server.url).unwrap().list(String::new()).await,
        Err(Error::Http(302))
    );
    let server = Server::respond("503 Service Unavailable", "not JSON", "");
    assert_eq!(
        Http::new(&server.url).unwrap().list(String::new()).await,
        Err(Error::Http(503))
    );
}
#[tokio::test]
async fn response_size_is_bounded_and_slow_transport_times_out() {
    let body = " ".repeat(1024 * 1024 + 1);
    let server = Server::respond("200 OK", &body, "");
    assert_eq!(
        Http::new(&server.url).unwrap().list(String::new()).await,
        Err(Error::InvalidResponse)
    );
    let server = Server::respond_after("200 OK", r#"{"items":[]}"#, "", Duration::from_secs(6));
    assert_eq!(
        Http::new(&server.url).unwrap().list(String::new()).await,
        Err(Error::Timeout)
    );
}

#[tokio::test]
async fn refuses_unavailable_transport_and_redacts_details() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/records", listener.local_addr().unwrap());
    drop(listener);
    let error = Http::new(&endpoint)
        .unwrap()
        .list("private query".into())
        .await
        .unwrap_err();
    assert_eq!(error, Error::Transport);
    assert!(!format!("{error:?} {error}").contains("private query"));
    assert!(!error.to_string().contains(&endpoint));
}
