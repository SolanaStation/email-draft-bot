use base64::{engine::general_purpose::URL_SAFE, Engine as _};
use serde::{Deserialize, Serialize};
use worker::*;

mod models;
use models::*;

mod gemini;
mod gmail;

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

    let access_token =
        match gmail::client::get_access_token(&client_id, &client_secret, &refresh_token).await {
            Ok(token_response) => {
                logs.push("Successfully authenticated with Google.".to_string());
                token_response.access_token
            }
            Err(e) => return Response::error(format!("Failed to get access token: {}", e), 500),
        };

    logs.push("Checking for unread emails...".to_string());
    match gmail::client::find_unread_emails(&access_token, &user_email).await {
        Ok(messages) => {
            if messages.is_empty() {
                logs.push("No unread emails found.".to_string());
            } else {
                logs.push(format!(
                    "Found {} unread email(s). Fetching details...",
                    messages.len()
                ));
                for (i, message_id) in messages.iter().enumerate() {
                    match gmail::client::get_email_details(
                        &access_token,
                        &user_email,
                        &message_id.id,
                    )
                    .await
                    {
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
                            let gemini_decision = match gemini::client::call_gemini(
                                &gemini_api_key,
                                &classification_prompt,
                            )
                            .await
                            {
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

                                match gemini::client::call_gemini(&gemini_api_key, &draft_prompt)
                                    .await
                                {
                                    Ok(draft_text) => {
                                        logs.push(format!("- Draft from Gemini: {}", draft_text));
                                        match gmail::client::create_draft(
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
                                                match gmail::client::mark_as_read(&access_token, &user_email, &message_id.id).await {
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
