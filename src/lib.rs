use base64::{engine::general_purpose::URL_SAFE, Engine as _};
use worker::*;

mod models;
use models::*;

mod drive;
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

                            let classification_prompt =
                                gemini::prompts::get_classification_prompt(from, subject, &body);

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

                                let draft_prompt =
                                    gemini::prompts::get_drafting_prompt(from, subject, &body);

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
                            } else if gemini_decision == "IS_FILE_REQUEST" {
                                logs.push(
                                    "- ✅ INTENT: File Request Detected. Proceeding to file research..."
                                        .to_string(),
                                );

                                let keywords_prompt =
                                    gemini::prompts::get_search_keywords_prompt(&body);

                                match gemini::client::call_gemini(&gemini_api_key, &keywords_prompt)
                                    .await
                                {
                                    Ok(search_keywords) => {
                                        logs.push(format!(
                                            "- Keywords for search: '{}'",
                                            search_keywords
                                        ));

                                        // 2. Build a robust search query for the Drive API
                                        // This splits keywords and requires all of them to be in the file name.
                                        let query = search_keywords
                                            .split_whitespace()
                                            .map(|word| {
                                                format!(
                                                    "name contains '{}'",
                                                    word.trim_matches('\'')
                                                )
                                            })
                                            .collect::<Vec<_>>()
                                            .join(" and ");

                                        let final_query = format!("{} and mimeType != 'application/vnd.google-apps.folder'", query);
                                        logs.push(format!(
                                            "- Executing Drive search with query: {}",
                                            final_query
                                        ));

                                        // 3. Call our new drive client to search for files
                                        match drive::client::search_files(
                                            &access_token,
                                            &final_query,
                                        )
                                        .await
                                        {
                                            Ok(files) => {
                                                if files.is_empty() {
                                                    logs.push("- ⚠️ No files found matching the search query.".to_string());
                                                    // FUTURE: Phase 4 (Human-in-the-loop) logic will go here.
                                                } else {
                                                    logs.push(format!(
                                                        "- ✅ Found {} matching file(s):",
                                                        files.len()
                                                    ));
                                                    for file in files {
                                                        // 4. For now, just log the name and URL of each found file
                                                        logs.push(format!(
                                                            "  - Name: {}, URL: {}",
                                                            file.name, file.web_view_link
                                                        ));
                                                    }
                                                    // FUTURE: Logic to select a file and attach it will go here.
                                                }
                                            }
                                            Err(e) => {
                                                logs.push(format!(
                                                    "- ❌ Error during Google Drive search: {}",
                                                    e
                                                ));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        logs.push(format!(
                                            "- ❌ Error getting search keywords from Gemini: {}",
                                            e
                                        ));
                                    }
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
