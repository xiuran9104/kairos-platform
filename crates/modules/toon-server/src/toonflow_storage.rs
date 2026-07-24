use base64::Engine;
use chrono::Utc;
use hmac::{Hmac, Mac};
use kairos_platform_framework_web::AppError;
use reqwest::{Method, Url, header};
use sha2::{Digest, Sha256};

use axum::{
    body::Body,
    extract::Path,
    http::{HeaderValue, Response},
};

type HmacSha256 = Hmac<Sha256>;

struct MinioConfig {
    endpoint: String,
    access_key: String,
    secret_key: String,
    bucket: String,
    region: String,
}

fn config() -> MinioConfig {
    MinioConfig {
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

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn hmac(key: &[u8], data: &str) -> Result<Vec<u8>, String> {
    let mut mac = HmacSha256::new_from_slice(key).map_err(|error| error.to_string())?;
    mac.update(data.as_bytes());
    Ok(mac.finalize().into_bytes().to_vec())
}

async fn signed_request(
    method: Method,
    object_key: Option<&str>,
    body: Vec<u8>,
) -> Result<reqwest::Response, String> {
    let config = config();
    let canonical_uri = match object_key {
        Some(key) => format!("/{}/{}", config.bucket, key.trim_start_matches('/')),
        None => format!("/{}", config.bucket),
    };
    let url = Url::parse(&format!("{}{}", config.endpoint, canonical_uri))
        .map_err(|error| error.to_string())?;
    let host = match url.port() {
        Some(port) => format!("{}:{port}", url.host_str().unwrap_or_default()),
        None => url.host_str().unwrap_or_default().to_string(),
    };
    let now = Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date = now.format("%Y%m%d").to_string();
    let payload_hash = sha256_hex(&body);
    let canonical_headers =
        format!("host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n");
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";
    let canonical_request = format!(
        "{}\n{canonical_uri}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}",
        method.as_str(),
    );
    let scope = format!("{date}/{}/s3/aws4_request", config.region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes()),
    );
    let date_key = hmac(format!("AWS4{}", config.secret_key).as_bytes(), &date)?;
    let region_key = hmac(&date_key, &config.region)?;
    let service_key = hmac(&region_key, "s3")?;
    let signing_key = hmac(&service_key, "aws4_request")?;
    let signature = hex::encode(hmac(&signing_key, &string_to_sign)?);
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
        config.access_key,
    );
    reqwest::Client::new()
        .request(method, url)
        .header(header::HOST, host)
        .header("x-amz-content-sha256", payload_hash)
        .header("x-amz-date", amz_date)
        .header(header::AUTHORIZATION, authorization)
        .body(body)
        .send()
        .await
        .map_err(|error| error.to_string())
}

async fn ensure_bucket() -> Result<(), String> {
    let response = signed_request(Method::PUT, None, Vec::new()).await?;
    if response.status().is_success() || response.status().as_u16() == 409 {
        return Ok(());
    }
    Err(format!(
        "创建 MinIO bucket 失败：HTTP {}",
        response.status()
    ))
}

pub async fn persist_remote_image(url: &str, asset_id: i64) -> Result<String, String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Ok(url.to_string());
    }
    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|error| format!("下载生成图片失败：{error}"))?;
    if !response.status().is_success() {
        return Err(format!("下载生成图片失败：HTTP {}", response.status()));
    }
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();
    let bytes = response.bytes().await.map_err(|error| error.to_string())?;
    ensure_bucket().await?;
    let extension = if content_type.contains("png") {
        "png"
    } else if content_type.contains("webp") {
        "webp"
    } else {
        "jpg"
    };
    let key = format!(
        "toonflow/assets/{asset_id}/{}.{}",
        uuid::Uuid::new_v4(),
        extension
    );
    let upload = signed_request(Method::PUT, Some(&key), bytes.to_vec()).await?;
    if !upload.status().is_success() {
        return Err(format!(
            "上传生成图片到 MinIO 失败：HTTP {}",
            upload.status()
        ));
    }
    Ok(format!("/toonflow/assets/files/{key}"))
}

async fn read_image(key: &str) -> Result<(String, Vec<u8>), String> {
    let response = signed_request(Method::GET, Some(key), Vec::new()).await?;
    if !response.status().is_success() {
        return Err(format!("读取 MinIO 图片失败：HTTP {}", response.status()));
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

pub async fn image_data_url(file_path: &str) -> Result<String, String> {
    let key = file_path
        .strip_prefix("/toonflow/assets/files/")
        .ok_or_else(|| "不支持的资产图片路径".to_string())?;
    let (stored_content_type, bytes) = read_image(key).await?;
    let content_type = image_content_type(&bytes).ok_or_else(|| {
        format!(
            "资产参考图不是有效的 PNG、JPEG、GIF 或 WebP 图片（存储类型：{stored_content_type}）"
        )
    })?;
    Ok(format!(
        "data:{content_type};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn image_content_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else {
        None
    }
}

pub async fn serve_image(Path(key): Path<String>) -> Result<Response<Body>, AppError> {
    let (content_type, bytes) = read_image(&key)
        .await
        .map_err(|_| AppError::not_found("asset image not found"))?;
    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&content_type)
            .map_err(|_| AppError::internal("invalid asset content type"))?,
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_reference_image_content_type_from_file_signature() {
        assert_eq!(
            image_content_type(b"\x89PNG\r\n\x1a\nrest"),
            Some("image/png")
        );
        assert_eq!(image_content_type(b"\xff\xd8\xffrest"), Some("image/jpeg"));
        assert_eq!(image_content_type(b"GIF89arest"), Some("image/gif"));
        assert_eq!(image_content_type(b"RIFF1234WEBPrest"), Some("image/webp"));
        assert_eq!(image_content_type(b"not-an-image"), None);
    }

    #[tokio::test]
    #[ignore = "requires the local MinIO service"]
    async fn minio_round_trip() {
        ensure_bucket().await.expect("create test bucket");
        let key = format!("toonflow/tests/{}.txt", uuid::Uuid::new_v4());
        let uploaded = signed_request(Method::PUT, Some(&key), b"kairos-platform".to_vec())
            .await
            .expect("upload object");
        assert!(uploaded.status().is_success());
        let (_, bytes) = read_image(&key).await.expect("read object");
        assert_eq!(bytes, b"kairos-platform");
    }
}
