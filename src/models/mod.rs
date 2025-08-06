use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
pub struct GoogleTokenResponse {
    pub access_token: String,
    pub expires_in: u32,
    pub scope: String,
    pub token_type: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MessageListResponse {
    pub messages: Option<Vec<MessageId>>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MessageId {
    pub id: String,
    pub thread_id: String,
}

#[derive(Deserialize, Debug)]
pub struct Message {
    pub id: String,
    pub snippet: String,
    pub payload: MessagePart,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MessagePart {
    pub part_id: String,
    pub mime_type: String,
    pub filename: String,
    pub headers: Vec<MessagePartHeaders>,
    pub body: MessagePartBody,
    pub parts: Option<Vec<MessagePart>>,
}

#[derive(Deserialize, Debug)]
pub struct MessagePartHeaders {
    pub name: String,
    pub value: String,
}

#[derive(Deserialize, Debug)]
pub struct MessagePartBody {
    pub size: u32,
    pub data: Option<String>,
}

// --- Gemini Structs ---
// These structs are for building the request we send TO Gemini.
#[derive(Serialize, Debug)]
pub struct GeminiRequest {
    pub contents: Vec<Content>,
}
#[derive(Serialize, Debug)]
pub struct Content {
    pub parts: Vec<Part>,
}
#[derive(Serialize, Debug)]
pub struct Part {
    pub text: String,
}

// These structs are for parsing the response we get FROM Gemini.
#[derive(Deserialize, Debug)]
pub struct GeminiResponse {
    pub candidates: Vec<Candidate>,
}
#[derive(Deserialize, Debug)]
pub struct Candidate {
    pub content: GeminiContent,
}
#[derive(Deserialize, Debug)]
pub struct GeminiContent {
    pub parts: Vec<PartText>,
}
#[derive(Deserialize, Debug)]
pub struct PartText {
    pub text: String,
}

#[derive(Serialize, Debug)]
pub struct CreateDraftRequest {
    pub message: DraftMessage,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DraftMessage {
    pub thread_id: String,
    pub raw: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ModifyMessageRequest {
    pub remove_label_ids: Vec<String>,
}

// Google Drive Structs
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileListResponse {
    pub files: Vec<DriveFile>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    pub web_view_link: String,
}
