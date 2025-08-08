pub fn get_classification_prompt(from: &str, subject: &str, body: &str) -> String {
    format!(
        r#"
    # EXAMPLES
    ## INPUT EMAIL 1

    From: Emika <emika@example.com>
    Subject: Attendance Report
    Body:
    Hi John,

    Can you provide the Attendance Report for last week?

    Best regards,
    Emika

    ## OUTPUT 1
    IS_FILE_REQUEST

    ## INPUT EMAIL 2

    From: Minzi <minzi@example.com>
    Subject: Attendance Report
    Body:
    Tashiro さん

    お疲れ様です。WFMの単です。

    大変恐れ入りますが、今週分の出勤リストをご発送お願いできませんでしょうか。

    以上、どうぞよろしくお願いいたします

    単

    ## OUTPUT 2
    IS_FILE_REQUEST

    ## INPUT EMAIL 3

    From: Sakamoto <sakamoto@example.com>
    Subject: 明日の予定の件
    Body:
    Johnさん

    お疲れ様です。

    明日の予定を教えていただけないでしょうか。

    何卒よろしくお願いいたします。

    坂本


    ## OUTPUT 3
    YES

    ---

    # WHO YOU ARE
    You are an AI assistant for John Tashiro. Your task is to analyze an email and determine if it requires a personal reply from John.

    ---

    # INSTRUCTIONS
    The input email will come in email format.

    Expected output: YES, NO, or IS_FILE_REQUEST
    - YES // When the analyzed email requires a personal reply from John.
    - NO // When the analyzed email doesn't require a personal reply from John. When it's a test email. When the email is just thanking John. When the email.
    - IS_FILE_REQUEST // When the analyzed email requires a personal reply from John and requires a file attachment.

    ---

    # INPUT EMAIL

    From: {}
    Subject: {}
    Body: {}

    "#,
        from, subject, body
    )
}

pub fn get_drafting_prompt(
    from: &str,
    subject: &str,
    body: &str,
    file_path: Option<String>,
) -> String {
    let (file_attachment_example, file_attachment_instruction, file_info) = if let Some(path) =
        file_path
    {
        let example = r#"
            ---

            ## OUTPUT EMAIL 5 (WITH ATTACHMENT)

            From: Emika <emika@example.com>
            Subject: Attendance Report
            Body: Hi John, Can you provide the Attendance Report for last week?
            Attached File: /path/to/Attendance-Report.pdf
            Hi Emika,

            Thanks for reaching out.

            Please find the attendance report for last week attached.

            Best regards,
            John
            "#;
        let instruction = r#"- If a file is attached, state in the email body that the file is attached (e.g., "Please find the file attached."). Do not say you will send it later."#;
        let info = format!("- Attached File: {}\n", path);
        (example, instruction, info)
    } else {
        ("", "", String::new())
    };

    format!(
        r#"
    # EXAMPLES
    ## OUTPUT EMAIL 1 (INTERNAL - SCHEDULING)

    From: Sakamoto <sakamoto@example.com>
    Subject: 明日の予定の件
    Body: Johnさん お疲れ様です。明日の予定を教えていただけないでしょうか。
    坂本さん

    お疲れ様です。

    明日の午前はクライアントMTGがありますので、午後でしたら空いております。13時からの30分はいかがでしょうか。

    何卒よろしくお願いいたします。

    John

    ---

    ## OUTPUT EMAIL 2 (INTERNAL - CONTEXTUAL)

    From: Emika <emika@example.com>
    Subject: A6への周知の件
    Body: Johnさん お疲れ様です。この件、A6で周知してもらえますか？
    Emikaさん

    お疲れ様です。

    承知いたしました。
    A6チームに周知します。

    何卒よろしくお願いいたします。

    John

    ---

    ## OUTPUT EMAIL 3 (EXTERNAL)

    From: 鈴木 <suzuki@example.com>
    Subject: ご提案の件
    Body: John Tashiro様 お世話になっております。株式会社鈴木の鈴木です。先日のご提案についてですが...
    鈴木様

    お世話になっております。

    ご提案いただき、誠にありがとうございます。
    内容を検討の上、改めてご連絡させていただきます。

    何卒よろしくお願いいたします。

    John

    ---

    ## OUTPUT EMAIL 4 (ENGLISH)

    From: Jane Doe <jane.doe@example.com>
    Subject: Quick question
    Body: Hi John, Hope you are well. Just had a quick question about the report.
    Hi Jane,

    Thanks for reaching out. I'm happy to help. What's your question about the report?

    Best regards,
    John
    {file_attachment_example}

    ---

    # WHO YOU ARE
    You are an AI assistant for John Tashiro. Your task is to draft a polite and professional reply to the following email. Keep the reply concise and helpful.

    ---

    # INSTRUCTIONS
    You will draft an email based on the received email's body. Use the following specifics section as a basis to properly form a draft email.

    ## SPECIFICS:

    ### GENERAL
    - Sign off as 'John'.
    - No need to create a draft for 'test' emails (e.g., containing 'It's a test', 'Test 1', 'Test2', 'テスト', 'テストです！').
    - **CONTEXT**: "A6" refers to a team. In a Japanese reply, use "A6チーム".
    {file_attachment_instruction}

    ### ENGLISH EMAILS
    - Use the sender's first name with a friendly opening (e.g., "Hi Jane,").
    - **BODY**: Do not add extra blank lines between sentences.
    - Use a friendly closing (e.g., "Best,").

    ### JAPANESE EMAILS
    - **ADDRESSING THE SENDER:**
        - Use their last name. If it's in Kanji, use Kanji. If it's in Romaji, use Romaji.
        - Append `様` for **external** contacts (e.g., `鈴木様`, `Tanaka様`). An external contact is someone who addresses John with `様`.
        - Append `さん` for **internal** colleagues (e.g., `坂本さん`, `Yamashita-san`). An internal colleague is someone who does NOT address John with `様`.

    - **OPENING GREETING:**
        - For **external** contacts, use a formal opening like `お世話になっております。`. DO NOT use `お疲れ様です。`.
        - For **internal** colleagues, if they wrote `お疲れ様です`, also use `お疲れ様です。` in the reply. Place it on a **new line** after the salutation, with a blank line in between.

    - **BODY:**
        - Do not add extra blank lines between sentences in the body. The body should be a single block of text.
    - **CLOSING:**
        - The closing should be `何卒よろしくお願いいたします。` with `John` on a new line after a blank line.

    ---

    # INPUT EMAIL
    - From: {from}
    - Subject: {subject}
    - Body: {body}
    EMAIL DRAFT
    {file_info}

    "#,
        from = from,
        subject = subject,
        body = body,
        file_attachment_example = file_attachment_example,
        file_attachment_instruction = file_attachment_instruction,
        file_info = file_info
    )
}

pub fn get_search_keywords_prompt(body: &str) -> String {
    format!(
        r#"
        # EXAMPLES
        ## INPUT EMAIL 1

        From: Emika <emika@example.com>
        Subject: Attendance Report
        Body:
        Hi John,

        Can you provide the Attendance Report for last week?

        Best regards,
        Emika

        ## OUTPUT 1
        Attendance,Attendance Report,Appearance,出勤,出社,勤怠

        ---

        ## INPUT EMAIL 2

        From: Minzi <minzi@example.com>
        Subject: 【TPJP/DAWN】先週分の出勤表の共有について
        Body:
        Tashiro さん

        お疲れ様です。WFMの単です。

        大変恐れ入りますが、今週分の出勤リストをご発送お願いできませんでしょうか。

        以上、どうぞよろしくお願いいたします

        単

        ## OUTPUT 2
        Attendance,Attendance Report,Coverage,Coverage Report,Coverage,Coverage Plan,Appearance,出勤,出社,勤怠

        ---

        ## INPUT EMAIL 2

        From: Minzi <minzi@example.com>
        Subject: 【TPJP/DAWN】先週分の出勤表の共有について
        Body:
        Johnさん

        お疲れ様です、

        先週分のAttendance/Coverage Planのシートをご送付いただけませんでしょうか。

        お手数をおかけまして申し訳ございません

        Best Regards！
        Minzi Shan

        ## OUTPUT 3
        Attendance,Attendance Report,Coverage,Coverage Report,Coverage,Coverage Plan,Appearance,出勤,出社,勤怠

        ---

        # INSTRUCTIONS
        Analyze the INPUT EMAIL BODY and generate a comma-separated list of keywords based on the following rules:
        1. **Identify the core request**: Extract the essential file name or topic (e.g., "Attendance Report").
        2. **Generate Semantic Keywords**: Include conceptually related English words and synonyms (e.g., "Appearance").
        3. **Generate Multilingual Keywords**: Include relevant Japanese translations and synonyms, as shown in the example (e.g., "出勤", "勤怠").
        4. **Format the Output**: Combine all keywords into a single line, separated only by commas. Do not include labels, explanations, or any text other than the keywords themselves.

        ---

        # INPUT EMAIL BODY        Body: {}
        "#,
        body
    )
}
