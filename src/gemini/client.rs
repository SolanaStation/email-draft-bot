use crate::models::*;
use worker::*;

pub async fn call_gemini(api_key: &str, prompt: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={}",
        api_key
    );

    let body = GeminiRequest {
        contents: vec![Content {
            parts: vec![Part {
                text: prompt.to_string(),
            }],
        }],
    };

    let res = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::from(format!("Reqwest error: {}", e)))?;

    if !res.status().is_success() {
        let error_text = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(Error::from(format!("Gemini API error: {}", error_text)));
    }

    let response_data = res
        .json::<GeminiResponse>()
        .await
        .map_err(|e| Error::from(format!("JSON parsing error: {}", e)))?;

    if let Some(candidate) = response_data.candidates.get(0) {
        if let Some(part) = candidate.content.parts.get(0) {
            return Ok(part.text.clone());
        }
    }

    Err(Error::from("Could not extract text from Gemini response"))
}
