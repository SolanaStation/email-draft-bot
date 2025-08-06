use crate::models::*;
use base64::{engine::general_purpose::URL_SAFE, Engine as _};
use worker::*;

pub async fn get_access_token(
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<GoogleTokenResponse> {
    let client = reqwest::Client::new();
    let params = [
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("refresh_token", refresh_token),
        ("grant_type", "refresh_token"),
    ];

    let res = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await
        .map_err(|e| Error::from(format!("Reqwest error: {}", e)))?;

    if !res.status().is_success() {
        let error_text = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(Error::from(format!("Google API error: {}", error_text)));
    }

    res.json::<GoogleTokenResponse>()
        .await
        .map_err(|e| Error::from(format!("JSON parsing error: {}", e)))
}

pub async fn find_unread_emails(access_token: &str, user_id: &str) -> Result<Vec<MessageId>> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/{}/messages",
        user_id
    );

    let res = client
        .get(&url)
        .bearer_auth(access_token)
        .query(&[("q", "is:unread")])
        .send()
        .await
        .map_err(|e| Error::from(format!("Reqwest error: {}", e)))?;

    let response_text = res
        .text()
        .await
        .map_err(|e| Error::from(format!("Failed to read response text: {}", e)))?;

    match serde_json::from_str::<MessageListResponse>(&response_text) {
        Ok(parse_data) => Ok(parse_data.messages.unwrap_or_default()),
        Err(_) => Err(Error::from(format!(
            "Gmail API returned non-JSON or error response: {}",
            response_text
        ))),
    }
}

pub async fn get_email_details(
    access_token: &str,
    user_id: &str,
    message_id: &str,
) -> Result<Message> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/{}/messages/{}",
        user_id, message_id
    );

    let res = client
        .get(&url)
        .bearer_auth(access_token)
        .query(&[
            ("format", "full"),
            ("metadataHeaders", "Date"),
            ("metadataHeaders", "From"),
            ("metadataHeaders", "To"),
            ("metadataHeaders", "Cc"),
            ("metadataHeaders", "Bcc"),
            ("metadataHeaders", "Subject"),
        ])
        .send()
        .await
        .map_err(|e| Error::from(format!("Reqwest error: {}", e)))?;

    let response_text = res
        .text()
        .await
        .map_err(|e| Error::from(format!("Failed to read response text: {}", e)))?;

    match serde_json::from_str::<Message>(&response_text) {
        Ok(parsed_text) => Ok(parsed_text),
        Err(_) => Err(Error::from(format!(
            "Gmail API returned non-JSON or error response: {}",
            response_text
        ))),
    }
}

pub async fn create_draft_with_attachment(
    access_token: &str,
    user_id: &str,
    thread_id: &str,
    to_all: &str,
    cc_all: &str,
    subject: &str,
    body: &str,
    attachment: Option<Attachment>,
) -> Result<()> {
    let raw_email = if let Some(att) = attachment {
        let boundary = "boundary_string_for_email_draft_bot";
        let mut headers = format!("To: {}\r\n", to_all);
        if !cc_all.is_empty() {
            headers.push_str(&format!("Cc: {}\r\n", cc_all));
        }
        headers.push_str(&format!("Subject: Re: {}\r\n", subject));
        headers.push_str("MIME-Version: 1.0\r\n");
        headers.push_str(&format!(
            "Content-Type: multipart/mixed; boundary=\"{}\"\r\n",
            boundary
        ));

        let body_part = format!(
            "--{boundary}\r\n\
             Content-Type: text/plain; charset=\"UTF-8\"\r\n\r\n\
             {body}\r\n",
            boundary = boundary,
            body = body
        );

        let attachment_part = format!(
            "--{boundary}\r\n\
             Content-Type: {mime_type}; name=\"{filename}\"\r\n\
             Content-Disposition: attachment; filename=\"{filename}\"\r\n\
             Content-Transfer-Encoding: base64\r\n\r\n\
             {data}\r\n",
            boundary = boundary,
            mime_type = att.mime_type,
            filename = att.filename,
            data = base64::engine::general_purpose::STANDARD.encode(&att.data)
        );

        format!(
            "{headers}\r\n{body_part}{attachment_part}--{boundary}--\r\n",
            headers = headers,
            body_part = body_part,
            attachment_part = attachment_part,
            boundary = boundary
        )
    } else {
        let mut headers = format!("To: {}\r\nSubject: Re: {}\r\n", to_all, subject);
        if !cc_all.is_empty() {
            headers.push_str(&format!("Cc: {}\r\n", cc_all));
        }
        format!("{}\r\n{}", headers, body)
    };

    let encoded_email = URL_SAFE.encode(raw_email);

    let draft_request = CreateDraftRequest {
        message: DraftMessage {
            thread_id: thread_id.to_string(),
            raw: encoded_email,
        },
    };

    let client = reqwest::Client::new();
    let url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/{}/drafts",
        user_id
    );

    let res = client
        .post(&url)
        .bearer_auth(access_token)
        .json(&draft_request)
        .send()
        .await
        .map_err(|e| Error::from(format!("Reqwest error: {}", e)))?;

    if res.status().is_success() {
        Ok(())
    } else {
        let error_text = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        Err(Error::from(format!(
            "Failed to create draft: {}",
            error_text
        )))
    }
}

pub async fn mark_as_read(access_token: &str, user_id: &str, message_id: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/{}/messages/{}/modify",
        user_id, message_id
    );

    let modify_request = ModifyMessageRequest {
        remove_label_ids: vec!["UNREAD".to_string()],
    };

    let res = client
        .post(&url)
        .bearer_auth(access_token)
        .json(&modify_request)
        .send()
        .await
        .map_err(|e| Error::from(format!("Reqwest error: {}", e)))?;

    if res.status().is_success() {
        Ok(())
    } else {
        let error_text = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        Err(Error::from(format!(
            "Failed to modify message: {}",
            error_text
        )))
    }
}
