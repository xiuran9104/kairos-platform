//! MinIO object store adapter.
//!
//! Minimal S3 SigV4 client for pent artifacts (reports, evidence payloads).
//! Follows the same signing approach as `toon-server::toonflow_storage`.
//! Configuration comes from `MINIO_ENDPOINT` / `MINIO_ACCESS_KEY` /
//! `MINIO_SECRET_KEY` / `MINIO_BUCKET` / `MINIO_REGION`.

use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::{Method, Url, header};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone)]
pub struct ObjectStore {
    endpoint: String,
    access_key: String,
    secret_key: String,
    bucket: String,
    region: String,
}

impl ObjectStore {
    pub fn from_env() -> Self {
        Self {
            endpoint: std::env::var("MINIO_ENDPOINT")
                .unwrap_or_else(|_| "http://127.0.0.1:9000".to_string())
                .trim_end_matches('/')
                .to_string(),
            access_key: std::env::var("MINIO_ACCESS_KEY")
                .unwrap_or_else(|_| "kairos_platform".to_string()),
            secret_key: std::env::var("MINIO_SECRET_KEY")
                .unwrap_or_else(|_| "kairos_platform_password".to_string()),
            bucket: std::env::var("MINIO_BUCKET").unwrap_or_else(|_| "kairos-platform".to_string()),
            region: std::env::var("MINIO_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
        }
    }

    pub async fn put_object(
        &self,
        key: &str,
        bytes: Vec<u8>,
        content_type: &str,
    ) -> Result<(), String> {
        self.ensure_bucket().await?;
        let response = self
            .signed_request(Method::PUT, Some(key), bytes, Some(content_type))
            .await?;
        if !response.status().is_success() {
            return Err(format!("上传产物到 MinIO 失败：HTTP {}", response.status()));
        }
        Ok(())
    }

    /// Returns `(content_type, bytes)`.
    pub async fn get_object(&self, key: &str) -> Result<(String, Vec<u8>), String> {
        let response = self.signed_request(Method::GET, Some(key), Vec::new(), None).await?;
        if !response.status().is_success() {
            return Err(format!("读取 MinIO 产物失败：HTTP {}", response.status()));
        }
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        let bytes = response.bytes().await.map_err(|error| error.to_string())?;
        Ok((content_type, bytes.to_vec()))
    }

    async fn ensure_bucket(&self) -> Result<(), String> {
        let response = self
            .signed_request(Method::PUT, None, Vec::new(), None)
            .await?;
        if response.status().is_success() || response.status().as_u16() == 409 {
            return Ok(());
        }
        Err(format!(
            "创建 MinIO bucket 失败：HTTP {}",
            response.status()
        ))
    }

    async fn signed_request(
        &self,
        method: Method,
        object_key: Option<&str>,
        body: Vec<u8>,
        content_type: Option<&str>,
    ) -> Result<reqwest::Response, String> {
        let canonical_uri = match object_key {
            Some(key) => format!("/{}/{}", self.bucket, key.trim_start_matches('/')),
            None => format!("/{}", self.bucket),
        };
        let url = Url::parse(&format!("{}{}", self.endpoint, canonical_uri))
            .map_err(|error| error.to_string())?;
        let host = match url.port() {
            Some(port) => format!("{}:{port}", url.host_str().unwrap_or_default()),
            None => url.host_str().unwrap_or_default().to_string(),
        };
        let now = Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date = now.format("%Y%m%d").to_string();
        let payload_hash = hex::encode(Sha256::digest(&body));
        let canonical_headers =
            format!("host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n");
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";
        let canonical_request = format!(
            "{}\n{canonical_uri}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}",
            method.as_str(),
        );
        let scope = format!("{date}/{}/s3/aws4_request", self.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
            hex::encode(Sha256::digest(canonical_request.as_bytes())),
        );
        let date_key = hmac(format!("AWS4{}", self.secret_key).as_bytes(), &date)?;
        let region_key = hmac(&date_key, &self.region)?;
        let service_key = hmac(&region_key, "s3")?;
        let signing_key = hmac(&service_key, "aws4_request")?;
        let signature = hex::encode(hmac(&signing_key, &string_to_sign)?);
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
            self.access_key,
        );
        let mut request = reqwest::Client::new()
            .request(method, url)
            .header(header::HOST, host)
            .header("x-amz-content-sha256", payload_hash)
            .header("x-amz-date", amz_date)
            .header(header::AUTHORIZATION, authorization)
            .body(body);
        if let Some(content_type) = content_type {
            request = request.header(header::CONTENT_TYPE, content_type);
        }
        request.send().await.map_err(|error| error.to_string())
    }
}

fn hmac(key: &[u8], data: &str) -> Result<Vec<u8>, String> {
    let mut mac = HmacSha256::new_from_slice(key).map_err(|error| error.to_string())?;
    mac.update(data.as_bytes());
    Ok(mac.finalize().into_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires the local MinIO service"]
    async fn minio_round_trip() {
        let store = ObjectStore::from_env();
        let key = format!("pent/tests/{}.txt", uuid::Uuid::new_v4());
        store
            .put_object(&key, b"kairos-pent".to_vec(), "text/plain")
            .await
            .expect("upload object");
        let (_, bytes) = store.get_object(&key).await.expect("read object");
        assert_eq!(bytes, b"kairos-pent");
    }
}
