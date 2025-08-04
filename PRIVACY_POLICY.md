# Privacy Policy for Email Draft Bot

**Last Updated:** August 4, 2025

This Privacy Policy describes how the Email Draft Bot handles your information. This is a personal tool and is not intended for public use.

## Information We Use

The Email Draft Bot is granted access to your Google Account to use the Gmail API. The application processes the following information:

- **Email Content:** Sender, subject, and body of incoming unread emails.
- **Authentication Tokens:** An OAuth 2.0 refresh token is stored securely in Cloudflare KV storage to maintain authorization.

## How We Use Information

- Email content (sender, subject, body) is sent to the Google Gemini API for the sole purpose of analyzing whether a reply is needed and generating a draft reply.
- Email content is **not** stored or logged by the application after processing.
- Authentication tokens are used exclusively to interact with the Google Gmail and Vertex AI APIs on your behalf.

## Information Sharing

Your data is not shared with any third parties other than Google for the API processing described above.

## Contact

If you have any questions about this Privacy Policy, please contact John Tashiro at john.tashiro@solana-station.com.
