use base64::{engine::general_purpose::URL_SAFE, Engine as _};
use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize, Debug)]
struct GoogleTokenResponse {
    access_token: String,
    expires_in: u32,
    scope: String,
    token_type: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct MessageListResponse {
    messages: Option<Vec<MessageId>>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct MessageId {
    id: String,
    thread_id: String,
}

#[derive(Deserialize, Debug)]
struct Message {
    id: String,
    snippet: String,
    payload: MessagePart,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct MessagePart {
    part_id: String,
    mime_type: String,
    filename: String,
    headers: Vec<MessagePartHeaders>,
    body: MessagePartBody,
    parts: Option<Vec<MessagePart>>,
}

#[derive(Deserialize, Debug)]
struct MessagePartHeaders {
    name: String,
    value: String,
}

#[derive(Deserialize, Debug)]
struct MessagePartBody {
    size: u32,
    data: Option<String>,
}

// --- Gemini Structs ---
// These structs are for building the request we send TO Gemini.
#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<Content>,
}
#[derive(Serialize)]
struct Content {
    parts: Vec<Part>,
}
#[derive(Serialize)]
struct Part {
    text: String,
}

// These structs are for parsing the response we get FROM Gemini.
#[derive(Deserialize, Debug)]
struct GeminiResponse {
    candidates: Vec<Candidate>,
}
#[derive(Deserialize, Debug)]
struct Candidate {
    content: GeminiContent,
}
#[derive(Deserialize, Debug)]
struct GeminiContent {
    parts: Vec<PartText>,
}
#[derive(Deserialize, Debug)]
struct PartText {
    text: String,
}

#[derive(Serialize)]
struct CreateDraftRequest {
    message: DraftMessage,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DraftMessage {
    thread_id: String,
    raw: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModifyMessageRequest {
    remove_label_ids: Vec<String>,
}

#[event(fetch)]
pub async fn main(_req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let mut logs: Vec<String> = Vec::new();
    logs.push("Fetch event started!".to_string());

    let client_id = env.secret("GMAIL_CLIENT_ID")?.to_string();
    let client_secret = env.secret("GMAIL_CLIENT_SECRET")?.to_string();
    let user_email = env.secret("USER_EMAIL")?.to_string();
    let kv = env.kv("GMAIL_AUTH")?;
    let gemini_api_key = env.secret("GEMINI_API_KEY")?.to_string();

    let refresh_token = match kv.get("refresh_token").text().await? {
        Some(token) => token,
        None => return Response::error("FATAL: Refresh token not found.", 500),
    };

    let access_token = match get_access_token(&client_id, &client_secret, &refresh_token).await {
        Ok(token_response) => {
            logs.push("Successfully authenticated with Google.".to_string());
            token_response.access_token
        }
        Err(e) => return Response::error(format!("Failed to get access token: {}", e), 500),
    };

    logs.push("Checking for unread emails...".to_string());
    match find_unread_emails(&access_token, &user_email).await {
        Ok(messages) => {
            if messages.is_empty() {
                logs.push("No unread emails found.".to_string());
            } else {
                logs.push(format!(
                    "Found {} unread email(s). Fetching details...",
                    messages.len()
                ));
                for (i, message_id) in messages.iter().enumerate() {
                    match get_email_details(&access_token, &user_email, &message_id.id).await {
                        Ok(details) => {
                            let from = details
                                .payload
                                .headers
                                .iter()
                                .find(|h| h.name == "From")
                                .map_or("Unknown Sender", |h| &h.value);
                            let to = details
                                .payload
                                .headers
                                .iter()
                                .find(|h| h.name == "To")
                                .map_or("", |h| h.value.as_str());
                            let cc = details
                                .payload
                                .headers
                                .iter()
                                .find(|h| h.name == "Cc")
                                .map_or("", |h| h.value.as_str());
                            let subject = details
                                .payload
                                .headers
                                .iter()
                                .find(|h| h.name == "Subject")
                                .map_or("No Subject", |h| &h.value);
                            let body = find_plain_text_body(&details.payload)
                                .map(|data| {
                                    URL_SAFE
                                        .decode(data)
                                        .ok()
                                        .map(|decoded| {
                                            String::from_utf8_lossy(&decoded).to_string()
                                        })
                                        .unwrap_or_else(|| "Could not decode body.".to_string())
                                })
                                .unwrap_or_else(|| "No plain text body found".to_string());

                            let classification_prompt = format!("
                                # WHO YOU ARE: You are an AI assistant for John Tashiro. Your task is to analyze an email and determine if it requires a personal reply from John. Respond with only a single word: YES or NO

                                ---EXAMPLE
                                [EMAIL]
                                From: Emika <emika@example.com>
                                Subject: Attendance Report
                                Body: Hi John, Can you provide the Attendance Report for last week? Best regards, Emika
                                [DECISION]
                                YES

                                ---EXAMPLE
                                [EMAIL]
                                From: Sakamoto <sakamoto@example.com>
                                Subject: 明日の予定の件
                                Body: Johnさん お疲れ様です。明日の予定を教えていただけないでしょうか。何卒よろしくお願いいたします。坂本
                                [DECISION]
                                YES

                                ---
                                [EMAIL]
                                From: {}
                                Subject: {}
                                Body: {}
                                [DECISION]
                                YES or NO?
                                ",
                                from, subject, body
                            );
                            let gemini_decision =
                                match call_gemini(&gemini_api_key, &classification_prompt).await {
                                    Ok(text) => text.trim().to_uppercase(),
                                    Err(e) => format!("Gemini Error: {}", e),
                                };

                            logs.push(format!("\n===== Email #{} =====", i + 1));
                            logs.push(format!("- Subject: {}", subject));
                            logs.push(format!("- Needs Reply?: {}", gemini_decision));

                            if gemini_decision == "YES" {
                                logs.push("- Decision is YES. Drafting reply...".to_string());

                                let mut to_recipients: Vec<&str> =
                                    from.split(',').chain(to.split(',')).collect();
                                let mut cc_recipients: Vec<&str> = cc.split(',').collect();

                                to_recipients.retain(|email| {
                                    !email.contains(&user_email) && !email.trim().is_empty()
                                });
                                cc_recipients.retain(|email| {
                                    !email.contains(&user_email) && !email.trim().is_empty()
                                });

                                to_recipients.sort();
                                cc_recipients.dedup();
                                let to_all = to_recipients.join(", ");

                                cc_recipients.sort();
                                cc_recipients.dedup();
                                let cc_all = cc_recipients.join(", ");

                                let draft_prompt = format!("
                                    # WHO YOU ARE: You are an AI assistant for John Tashiro. Your task is to draft a polite and professional reply to the following email. Keep the reply concise and helpful.

                                    # SPECIFICS:
                                    - Sign off as 'John'
                                    - Include the sender's first name if it's in English. If it's in Japanese, use their Japanese last name in Kanji. But if there's no kanji, use their last name in English while adding a '-san.' For example, 'Hello Yamashita-san,'...
                                    - If the conversation is in English have a nice friendly opening, but if it's Japanese, you don't need that.
                                    - If it's a Japanese conversation and the sender had 'お疲れ様です' in their opening, use the same in your reply draft, 'お疲れ様です。'
                                    - If it's a Japanese conversation and the sender said 'お疲れ様です' or 'お疲れ様でございます,' use 'さん' instead of '様' because we're the same company member.
                                    - If it's a Japanese conversation, have the closure as '何卒よろしくお願いいたします。 John'
                                    - No need to create a draft if someone sends a 'test' email. They'll say like 'It's a test.' or 'Test 1' or 'Test2' or 'テスト' or 'テストです！' etc.

                                    ---EXAMPLE
                                    [EMAIL]
                                    From: Sakamoto <sakamoto@example.com>
                                    Subject: 明日の予定の件
                                    [DRAFT REPLY]
                                    坂本さん お疲れ様です。明日の午前はクライアントMTGがありますので、午後でしたら空いております。13時からの30分はいかがでしょうか。何卒よろしくお願いいたします。John

                                    ---
                                    [ORIGINAL EMAIL]
                                    - From: {}
                                    - Subject: {}
                                    - Body: {}\
                                    [DRAFT REPLY]",
                                    from, subject, body
                                );

                                match call_gemini(&gemini_api_key, &draft_prompt).await {
                                    Ok(draft_text) => {
                                        logs.push(format!("- Draft from Gemini: {}", draft_text));
                                        match create_draft(
                                            &access_token,
                                            &user_email,
                                            &message_id.thread_id,
                                            &to_all,
                                            &cc_all,
                                            subject,
                                            &draft_text,
                                        )
                                        .await
                                        {
                                            Ok(_) => {
                                                logs.push(
                                                    "- Successfully created draft in Gmail."
                                                        .to_string(),
                                                );
                                                match mark_as_read(&access_token, &user_email, &message_id.id).await {
                                                    Ok(_) => logs.push("- Successfully marked original email as read.".to_string()),
                                                    Err(e) => logs.push(format!("- Failed to mark email as read: {}", e)),
                                                }
                                            }
                                            Err(e) => logs
                                                .push(format!("- Failed to create draft: {}", e)),
                                        }
                                    }
                                    Err(e) => logs.push(format!(
                                        "- Failed to generate draft from Gemini: {}",
                                        e
                                    )),
                                }
                            }
                        }
                        Err(e) => {
                            logs.push(format!(
                                "Error fetching details for message {}: {}",
                                message_id.id, e
                            ));
                        }
                    }
                }
            }
        }
        Err(e) => return Response::error(format!("Failed to fetch emails: {}", e), 500),
    }

    Response::ok(logs.join("\n"))
}

async fn get_access_token(
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
        // This is the fix: Manually map the reqwest error to a worker::Error
        .map_err(|e| Error::from(format!("Reqwest error: {}", e)))?;

    if !res.status().is_success() {
        let error_text = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(Error::from(format!("Google API error: {}", error_text)));
    }

    // Also apply the fix here for the JSON parsing error
    res.json::<GoogleTokenResponse>()
        .await
        .map_err(|e| Error::from(format!("JSON parsing error: {}", e)))
}

async fn find_unread_emails(access_token: &str, user_id: &str) -> Result<Vec<MessageId>> {
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

async fn get_email_details(access_token: &str, user_id: &str, message_id: &str) -> Result<Message> {
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

fn find_plain_text_body(part: &MessagePart) -> Option<&str> {
    if part.mime_type == "text/plain" {
        if let Some(data) = &part.body.data {
            return Some(data);
        }
    }

    if let Some(parts) = &part.parts {
        for sub_part in parts {
            if let Some(body) = find_plain_text_body(sub_part) {
                return Some(body);
            }
        }
    }

    None
}

async fn call_gemini(api_key: &str, prompt: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let url = format!("
        https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash-lite:generateContent?key={}", api_key);

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

    // Extract the text from the complex response structure
    if let Some(candidate) = response_data.candidates.get(0) {
        if let Some(part) = candidate.content.parts.get(0) {
            return Ok(part.text.clone());
        }
    }

    Err(Error::from("Could not extract text from Gemini response"))
}

async fn create_draft(
    access_token: &str,
    user_id: &str,
    thread_id: &str,
    to_all: &str,
    cc_all: &str,
    subject: &str,
    body: &str,
) -> Result<()> {
    let mut headers = format!("To: {}\r\nSubject: Re: {}\r\n", to_all, subject);
    if !cc_all.is_empty() {
        headers.push_str(&format!("Cc: {}\r\n", cc_all));
    }

    let raw_email = format!("{}\r\n{}", headers, body);
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

async fn mark_as_read(access_token: &str, user_id: &str, message_id: &str) -> Result<()> {
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
