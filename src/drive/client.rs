use crate::models::{DriveFile, FileListResponse};
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
            ("fields", "files(id,name,webViewLink)"),
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
