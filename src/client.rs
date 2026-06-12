use crate::decrypt::EncryptedResponse;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),
    #[error("invalid response (HTTP {status}): {body}")]
    InvalidResponse { status: u16, body: String },
    #[error("invalid response: {0}")]
    BadData(String),
}

pub async fn fetch_encrypted(
    client: &reqwest::Client,
    base_url: &str,
    uuid: &str,
) -> Result<EncryptedResponse, ClientError> {
    let url = format!("{}/get/{}", base_url.trim_end_matches('/'), uuid);
    let resp = client.get(&url).send().await?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let mut body = resp.text().await.unwrap_or_default();
        if body.len() > 500 {
            body.truncate(500);
            body.push_str("... (truncated)");
        }
        return Err(ClientError::InvalidResponse { status, body });
    }

    let data: EncryptedResponse = resp.json().await?;

    if data.encrypted.is_empty() {
        return Err(ClientError::BadData("empty encrypted field".into()));
    }

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn setup_server() -> MockServer {
        MockServer::start().await
    }

    #[tokio::test]
    async fn fetch_encrypted_success() {
        let server = setup_server().await;
        Mock::given(method("GET"))
            .and(path("/get/test-uuid"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "encrypted": "abc123",
                "crypto_type": "legacy"
            })))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = fetch_encrypted(&client, &server.uri(), "test-uuid").await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.encrypted, "abc123");
        assert_eq!(resp.crypto_type, Some("legacy".to_string()));
    }

    #[tokio::test]
    async fn fetch_encrypted_success_no_crypto_type() {
        let server = setup_server().await;
        Mock::given(method("GET"))
            .and(path("/get/test-uuid"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "encrypted": "xyz789"
            })))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = fetch_encrypted(&client, &server.uri(), "test-uuid").await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.encrypted, "xyz789");
        assert!(resp.crypto_type.is_none());
    }

    #[tokio::test]
    async fn fetch_encrypted_http_404() {
        let server = setup_server().await;
        Mock::given(method("GET"))
            .and(path("/get/missing"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = fetch_encrypted(&client, &server.uri(), "missing").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ClientError::InvalidResponse { status, body } => {
                assert_eq!(status, 404);
                assert_eq!(body, "Not Found");
            }
            _ => panic!("expected InvalidResponse"),
        }
    }

    #[tokio::test]
    async fn fetch_encrypted_500_body_truncated() {
        let server = setup_server().await;
        let long_body = "x".repeat(1000);
        Mock::given(method("GET"))
            .and(path("/get/uuid"))
            .respond_with(ResponseTemplate::new(500).set_body_string(&long_body))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = fetch_encrypted(&client, &server.uri(), "uuid").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ClientError::InvalidResponse { status, body } => {
                assert_eq!(status, 500);
                assert!(body.ends_with("... (truncated)"));
                assert!(body.len() < 600);
            }
            _ => panic!("expected InvalidResponse"),
        }
    }

    #[tokio::test]
    async fn fetch_encrypted_empty_encrypted() {
        let server = setup_server().await;
        Mock::given(method("GET"))
            .and(path("/get/uuid"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "encrypted": ""
            })))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = fetch_encrypted(&client, &server.uri(), "uuid").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ClientError::BadData(msg) => assert!(msg.contains("empty encrypted")),
            _ => panic!("expected BadData"),
        }
    }

    #[tokio::test]
    async fn fetch_encrypted_invalid_json() {
        let server = setup_server().await;
        Mock::given(method("GET"))
            .and(path("/get/uuid"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = fetch_encrypted(&client, &server.uri(), "uuid").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ClientError::HttpError(_)));
    }

    #[tokio::test]
    async fn fetch_encrypted_base_url_trailing_slash() {
        let server = setup_server().await;
        Mock::given(method("GET"))
            .and(path("/get/uuid"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "encrypted": "data"
            })))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        // URI with trailing slash should still work
        let base = server.uri();
        let base_with_slash = format!("{base}/");
        let result = fetch_encrypted(&client, &base_with_slash, "uuid").await;
        assert!(result.is_ok());
    }
}
