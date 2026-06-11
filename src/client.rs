use crate::decrypt::EncryptedResponse;

#[derive(Debug)]
pub enum ClientError {
    HttpError(reqwest::Error),
    // Response is valid JSON but missing required fields
    InvalidResponse(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::HttpError(e) => write!(f, "HTTP error: {e}"),
            ClientError::InvalidResponse(msg) => write!(f, "invalid response: {msg}"),
        }
    }
}

impl std::error::Error for ClientError {}

pub async fn fetch_encrypted(
    client: &reqwest::Client,
    base_url: &str,
    uuid: &str,
) -> Result<EncryptedResponse, ClientError> {
    let url = format!("{}/get/{}", base_url.trim_end_matches('/'), uuid);
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(ClientError::HttpError)?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(ClientError::InvalidResponse(format!(
            "HTTP {status}: {body}"
        )));
    }

    let data: EncryptedResponse = resp.json().await.map_err(ClientError::HttpError)?;

    if data.encrypted.is_empty() {
        return Err(ClientError::InvalidResponse("empty encrypted field".into()));
    }

    Ok(data)
}
