use flaremail_rs::{Address, Attachment, Email, Error, SendEmail, SendOptions};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

async fn mock_server(responses: Vec<&'static str>) -> (String, JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let mut requests = Vec::new();

        for response in responses {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = Vec::new();
            let mut chunk = [0; 1024];

            loop {
                let read = stream.read(&mut chunk).await.unwrap();
                if read == 0 {
                    break;
                }

                buffer.extend_from_slice(&chunk[..read]);
                if has_complete_request(&buffer) {
                    break;
                }
            }

            requests.push(String::from_utf8_lossy(&buffer).to_string());
            let http_response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                response.len(),
                response,
            );
            stream.write_all(http_response.as_bytes()).await.unwrap();
        }

        requests
    });

    (format!("http://{address}"), handle)
}

fn has_complete_request(buffer: &[u8]) -> bool {
    let request = String::from_utf8_lossy(buffer);
    let Some((headers, body)) = request.split_once("\r\n\r\n") else {
        return false;
    };

    let content_length = headers
        .lines()
        .find_map(|line| line.strip_prefix("content-length: "))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);

    body.len() >= content_length
}

fn request_body(request: &str) -> Value {
    let (_, body) = request.split_once("\r\n\r\n").unwrap();
    serde_json::from_str(body).unwrap()
}

#[tokio::test]
async fn sends_email_to_cloudflare_rest_endpoint() {
    let (base_url, handle) = mock_server(vec![
        r#"{"success":true,"errors":[],"messages":[],"result":{"messageId":"msg_123","delivered":["user@example.com"],"queued":[],"permanent_bounces":[]}}"#,
    ])
    .await;
    let email = Email::new("cf-token")
        .with_account_id("account-123")
        .with_base_url(base_url);
    let message = SendEmail::new(
        Address::named("Acme", "noreply@example.com"),
        ["user@example.com"],
        "Hello world",
    )
    .html("<strong>It works!</strong>")
    .text("It works!")
    .cc("cc@example.com")
    .bcc(["bcc@example.com"])
    .reply_to("reply@example.com")
    .header("X-Test", "true")
    .attachment(Attachment::new("SGVsbG8=", "hello.txt", "text/plain").inline("hello-file"));

    let result = email
        .emails()
        .send_with_options(
            message,
            SendOptions::new().idempotency_key("welcome/user-123"),
        )
        .await
        .unwrap();

    assert_eq!(result.id, "msg_123");
    assert_eq!(result.delivered, vec!["user@example.com"]);

    let requests = handle.await.unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("POST /accounts/account-123/email/sending/send HTTP/1.1"));
    assert!(requests[0].contains("authorization: Bearer cf-token"));
    assert!(requests[0].contains("idempotency-key: welcome/user-123"));

    let body = request_body(&requests[0]);
    assert_eq!(
        body["from"],
        serde_json::json!({"address":"noreply@example.com","name":"Acme"})
    );
    assert_eq!(body["to"], serde_json::json!(["user@example.com"]));
    assert_eq!(body["subject"], "Hello world");
    assert_eq!(body["html"], "<strong>It works!</strong>");
    assert_eq!(body["text"], "It works!");
    assert_eq!(body["cc"], "cc@example.com");
    assert_eq!(body["bcc"], serde_json::json!(["bcc@example.com"]));
    assert_eq!(body["reply_to"], "reply@example.com");
    assert_eq!(body["headers"], serde_json::json!({"X-Test":"true"}));
    assert_eq!(body["attachments"][0]["content_id"], "hello-file");
}

#[tokio::test]
async fn returns_api_errors() {
    let (base_url, _handle) = mock_server(vec![
        r#"{"success":false,"errors":[{"code":1234,"message":"Invalid from address"}],"messages":[],"result":null}"#,
    ])
    .await;
    let email = Email::new("cf-token")
        .with_account_id("account-123")
        .with_base_url(base_url);

    let error = email
        .emails()
        .send(
            SendEmail::new("noreply@example.com", "user@example.com", "Hello").html("<p>Hello</p>"),
        )
        .await
        .unwrap_err();

    match error {
        Error::Api {
            status,
            code,
            message,
            ..
        } => {
            assert_eq!(status, 200);
            assert_eq!(code.as_deref(), Some("1234"));
            assert_eq!(message, "Invalid from address");
        }
        error => panic!("unexpected error: {error:?}"),
    }
}

#[tokio::test]
async fn discovers_single_account() {
    let (base_url, handle) = mock_server(vec![
        r#"{"success":true,"errors":[],"messages":[],"result":[{"id":"account-123"}]}"#,
        r#"{"success":true,"errors":[],"messages":[],"result":{"messageId":"msg_123","queued":["user@example.com"]}}"#,
    ])
    .await;
    let email = Email::new("cf-token").with_base_url(base_url);

    let result = email
        .emails()
        .create(SendEmail::new("noreply@example.com", "user@example.com", "Hello").text("Hello"))
        .await
        .unwrap();

    assert_eq!(result.id, "msg_123");
    assert_eq!(result.queued, vec!["user@example.com"]);

    let requests = handle.await.unwrap();
    assert!(requests[0].starts_with("GET /accounts HTTP/1.1"));
    assert!(requests[1].starts_with("POST /accounts/account-123/email/sending/send HTTP/1.1"));
}

#[tokio::test]
async fn validates_content_before_request() {
    let (base_url, handle) = mock_server(Vec::new()).await;
    let email = Email::new("cf-token")
        .with_account_id("account-123")
        .with_base_url(base_url);

    let error = email
        .emails()
        .send(SendEmail::new(
            "noreply@example.com",
            "user@example.com",
            "Hello",
        ))
        .await
        .unwrap_err();

    assert!(matches!(error, Error::Validation(_)));
    assert!(handle.await.unwrap().is_empty());
}
