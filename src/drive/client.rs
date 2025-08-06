use crate::models::{AttachmentData, DriveFile, FileListResponse};
use worker::*;

pub async fn search_files(access_token: &str, query: &str) -> Result<Vec<DriveFile>> {
    let client = reqwest::Client::new();
    let url = "https://www.googleapis.com/drive/v3/files";

    let res = client
        .get(url)
        .bearer_auth(access_token)
        .query(&[
            ("q", query),
            // Important: This specifies we only want the fields we defined in our struct
            ("fields", "files(id,name,mimeType,webViewLink)"),
        ])
        .send()
        .await
        .map_err(|e| Error::from(format!("Reqwest error during Drive search: {}", e)))?;

    if !res.status().is_success() {
        let error_text = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown Drive API error".to_string());
        return Err(Error::from(format!(
            "Google Drive API error: {}",
            error_text
        )));
    }

    let response = res
        .json::<FileListResponse>()
        .await
        .map_err(|e| Error::from(format!("JSON parsing error from Drive API: {}", e)))?;

    Ok(response.files)
}

pub async fn download_file(access_token: &str, file_id: &str) -> Result<AttachmentData> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://www.googleapis.com/drive/v3/files/{}?alt=media",
        file_id
    );

    let res = client
        .get(&url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| Error::from(format!("Reqwest error during file download: {}", e)))?;

    if !res.status().is_success() {
        let error_text = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown Drive API error during download".to_string());
        return Err(Error::from(format!(
            "Google Drive API error during download: {}",
            error_text
        )));
    }

    let file_data = res
        .bytes()
        .await
        .map_err(|e| Error::from(format!("Failed to read file bytes: {}", e)))?;

    Ok(file_data.to_vec())
}

pub async fn export_file(
    access_token: &str,
    file_id: &str,
    mime_type: &str,
) -> Result<AttachmentData> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://www.googleapis.com/drive/v3/files/{}/export",
        file_id
    );

    let res = client
        .get(&url)
        .bearer_auth(access_token)
        .query(&[("mimeType", mime_type)])
        .send()
        .await
        .map_err(|e| Error::from(format!("Reqwest error during file export: {}", e)))?;

    if !res.status().is_success() {
        let error_text = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown Drive API error during export".to_string());
        return Err(Error::from(format!(
            "Google Drive API error during export: {}",
            error_text
        )));
    }

    let file_data = res
        .bytes()
        .await
        .map_err(|e| Error::from(format!("Failed to read exported file bytes: {}", e)))?;

    Ok(file_data.to_vec())
}
