use axum::http::StatusCode;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleClient {
    http: reqwest::Client,
    base_url: String,
}

impl OpenAiCompatibleClient {
    pub fn new(base_url: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_owned(),
        }
    }

    pub async fn chat_completions(
        &self,
        authorization: HeaderValue,
        body: Value,
    ) -> Result<ProviderJsonResponse, ProviderError> {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, authorization);

        let response = self
            .http
            .post(format!("{}/v1/chat/completions", self.base_url))
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(ProviderError::request)?;

        let status = StatusCode::from_u16(response.status().as_u16())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = response
            .json::<Value>()
            .await
            .map_err(ProviderError::response)?;

        Ok(ProviderJsonResponse { status, body })
    }
}

#[derive(Debug)]
pub struct ProviderJsonResponse {
    pub status: StatusCode,
    pub body: Value,
}

#[derive(Debug)]
pub struct ProviderError {
    pub message: String,
}

impl ProviderError {
    fn request(error: reqwest::Error) -> Self {
        Self {
            message: format!("provider request failed: {error}"),
        }
    }

    fn response(error: reqwest::Error) -> Self {
        Self {
            message: format!("provider returned a non-JSON response: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        Json, Router,
        http::{HeaderMap, header::AUTHORIZATION},
        routing::post,
    };
    use serde_json::{Value, json};
    use tokio::net::TcpListener;

    use super::*;

    #[tokio::test]
    #[ignore = "binds a local mock upstream; run explicitly when local networking is available"]
    async fn forwards_authorization_and_json_body() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock listener");
        let addr = listener.local_addr().expect("mock listener addr");
        let app = Router::new().route("/v1/chat/completions", post(mock_chat_completions));
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("mock server");
        });

        let client = OpenAiCompatibleClient::new(format!("http://{addr}"));
        let response = client
            .chat_completions(
                "Bearer sk-pass-through".parse().unwrap(),
                json!({
                    "model": "gpt-4.1-mini",
                    "messages": [{ "role": "user", "content": "hello" }]
                }),
            )
            .await
            .expect("provider response");

        assert_eq!(response.status, StatusCode::OK);
        assert_eq!(response.body["authorization"], "Bearer sk-pass-through");
        assert_eq!(response.body["model"], "gpt-4.1-mini");
    }

    async fn mock_chat_completions(headers: HeaderMap, Json(body): Json<Value>) -> Json<Value> {
        Json(json!({
            "authorization": headers
                .get(AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .unwrap_or(""),
            "model": body["model"],
        }))
    }
}
