use super::*;
fn settings(wrap: bool) -> MessageSettings {
    MessageSettings {
        sign_off: false,
        auto_wrap_commit_message: wrap,
        auto_wrap_width: 8,
    }
}
#[test]
fn subject_not_wrapped_and_body_hard_breaks_whitespace_and_long_words_retained() {
    let draft = Draft {
        subject: "long subject stays intact".into(),
        body: "one two three four\n\nlongwordwithoutspaces\n".into(),
    };
    assert_eq!(
        draft.message(&settings(true)),
        "long subject stays intact\n\none two \nthree \nfour\n\nlongwordwithoutspaces\n"
    );
    assert_eq!(
        draft.message(&settings(false)),
        "long subject stays intact\n\none two three four\n\nlongwordwithoutspaces\n"
    );
}
#[test]
fn footnotes_and_final_trailers_protected_not_middle_paragraph_trailers() {
    let draft = Draft { subject: "subject".into(), body: "Signed-off-by: Long Name\n\n[1]: https://example.invalid/long\n\nSigned-off-by: Long Name\nCloses: https://example.invalid/long".into() };
    assert_eq!(
        draft.message(&settings(true)),
        "subject\n\nSigned-off-by: \nLong \nName\n\n[1]: https://example.invalid/long\n\nSigned-off-by: Long Name\nCloses: https://example.invalid/long"
    );
}
#[test]
fn unicode_grapheme_cell_widths_and_bytes_are_preserved() {
    for (body, width, expected) in [
        ("界界 界界", 5, "界界 \n界界"),
        ("e\u{301}e\u{301} x", 4, "e\u{301}e\u{301} x"),
        ("👨‍👩‍👧‍👦 ab", 5, "👨‍👩‍👧‍👦 ab"),
        ("👨‍👩‍👧‍👦 ab", 4, "👨‍👩‍👧‍👦 \nab"),
        ("🇳🇿 🇫🇷", 4, "🇳🇿 \n🇫🇷"),
        ("❤\u{fe0f} ab", 4, "❤\u{fe0f} \nab"),
        ("界界界界", 2, "界界界界"),
        ("a\tb c", 4, "a\tb c"),
        ("a\u{a0}b c", 4, "a\u{a0}b \nc"),
    ] {
        assert_eq!(wrap(body, width), expected, "{body:?} at {width}");
        assert_eq!(wrap(body, 0), body);
    }
}
#[test]
fn source_matchers_protect_prefix_spaces_not_entire_footnote_lines() {
    assert_eq!(wrap("[1]:  one two three", 8), "[1]:  one \ntwo \nthree");
    assert_eq!(wrap("[x]:  one two three", 8), "[x]:  \none two \nthree");
    assert_eq!(wrap("[1]:\t one two three", 8), "[1]:\t one \ntwo \nthree");
    assert_eq!(
        wrap("Signed-off-by: Long Name\n\nbody", 8),
        "Signed-off-by: \nLong \nName\n\nbody"
    );
    assert_eq!(
        wrap("body\n\nSigned-off-by: Long Name\n\n", 8),
        "body\n\nSigned-off-by: Long Name\n\n"
    );
    assert_eq!(
        wrap("Closes:  \thttps://example.invalid/long text", 8),
        "Closes:  \thttps://example.invalid/long text"
    );
    assert_eq!(
        wrap("Closes:  not-url text", 8),
        "Closes:  \nnot-url \ntext"
    );
    assert_eq!(
        wrap("Σigned-off-by: Long Name", 8),
        "Σigned-off-by: \nLong \nName"
    );
}
#[test]
fn record_recall_is_full_message_not_pretty_subject_and_declines_lossy_bytes() {
    let mut record = crate::git::Commit {
        oid: "a".repeat(40),
        parents: vec![],
        author: vec![],
        timestamp: 0,
        subject: b"pretty flattened subject".to_vec(),
        message: b"raw subject\n\nbody\n\ntrailer\n".to_vec(),
    };
    assert_eq!(
        Draft::from_record(&record),
        Some(Draft {
            subject: "raw subject".into(),
            body: "body\n\ntrailer\n".into(),
        })
    );
    record.message = b"subject\n\nnon-UTF8 \xff".to_vec();
    assert!(Draft::from_record(&record).is_none());
    record.message = b"subject\n\nNUL\0body".to_vec();
    assert!(Draft::from_record(&record).is_none());
}
#[test]
fn canonical_message_split_and_empty_body() {
    assert_eq!(
        Draft::from_message("summary\n\nbody\n"),
        Draft {
            subject: "summary".into(),
            body: "body\n".into()
        }
    );
    assert_eq!(
        Draft::from_message("summary").message(&settings(true)),
        "summary"
    );
}
