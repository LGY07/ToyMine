use std::sync::LazyLock;
use tailspin::Highlighter;
use tailspin::config::*;
use tailspin::style::{Color, Style};

pub static HIGHLIGHTER: LazyLock<Highlighter> = LazyLock::new(|| {
    let mut builder = Highlighter::builder();

    // ===== 时间 =====
    builder.with_date_time_highlighters(DateTimeConfig {
        date: Style {
            fg: Some(Color::Green),
            ..Default::default()
        },
        time: Style {
            fg: Some(Color::BrightGreen),
            ..Default::default()
        },
        zone: Style {
            fg: Some(Color::BrightBlack),
            ..Default::default()
        },
        separator: Style {
            fg: Some(Color::BrightBlack),
            ..Default::default()
        },
    });

    // ===== UUID =====
    builder.with_uuid_highlighter(UuidConfig {
        number: Style {
            fg: Some(Color::BrightMagenta),
            ..Default::default()
        },
        letter: Style {
            fg: Some(Color::BrightMagenta),
            ..Default::default()
        },
        dash: Style {
            fg: Some(Color::BrightBlack),
            ..Default::default()
        },
    });

    // ===== URL =====
    builder.with_url_highlighter(UrlConfig {
        http: Style {
            fg: Some(Color::Blue),
            underline: true,
            ..Default::default()
        },
        https: Style {
            fg: Some(Color::Blue),
            underline: true,
            ..Default::default()
        },
        host: Style {
            fg: Some(Color::BrightBlue),
            ..Default::default()
        },
        path: Style {
            fg: Some(Color::Cyan),
            ..Default::default()
        },
        query_params_key: Style {
            fg: Some(Color::Yellow),
            ..Default::default()
        },
        query_params_value: Style {
            fg: Some(Color::Green),
            ..Default::default()
        },
        symbols: Style {
            fg: Some(Color::Blue),
            ..Default::default()
        },
    });

    // ===== IPv4 =====
    builder.with_ip_v4_highlighter(IpV4Config {
        number: Style {
            fg: Some(Color::BrightBlue),
            ..Default::default()
        },
        separator: Style {
            fg: Some(Color::Blue),
            ..Default::default()
        },
    });

    // ===== IPv6 =====
    builder.with_ip_v6_highlighter(IpV6Config {
        number: Style {
            fg: Some(Color::BrightBlue),
            ..Default::default()
        },
        letter: Style {
            fg: Some(Color::BrightBlue),
            ..Default::default()
        },
        separator: Style {
            fg: Some(Color::Blue),
            ..Default::default()
        },
    });

    // ===== 引号内容 =====
    builder.with_quote_highlighter(QuotesConfig {
        quotes_token: '"',
        style: Style {
            fg: Some(Color::Yellow),
            ..Default::default()
        },
    });

    // ===== 日志等级关键字 =====
    builder.with_keyword_highlighter(vec![
        KeywordConfig {
            words: vec!["ERROR".into(), "FATAL".into()],
            style: Style {
                fg: Some(Color::BrightRed),
                bold: true,
                ..Default::default()
            },
        },
        KeywordConfig {
            words: vec!["WARN".into()],
            style: Style {
                fg: Some(Color::Yellow),
                bold: true,
                ..Default::default()
            },
        },
        KeywordConfig {
            words: vec!["INFO".into()],
            style: Style {
                fg: Some(Color::BrightBlue),
                ..Default::default()
            },
        },
        KeywordConfig {
            words: vec!["DEBUG".into()],
            style: Style {
                fg: Some(Color::BrightBlack),
                ..Default::default()
            },
        },
        KeywordConfig {
            words: vec!["TRACE".into()],
            style: Style {
                fg: Some(Color::BrightBlack),
                italic: true,
                ..Default::default()
            },
        },
    ]);

    // ===== JSON =====
    builder.with_json_highlighter(JsonConfig::default());

    // ===== 指针 =====
    builder.with_pointer_highlighter(PointerConfig {
        number: Style {
            fg: Some(Color::BrightBlack),
            ..Default::default()
        },
        letter: Style {
            fg: Some(Color::BrightBlack),
            ..Default::default()
        },
        separator: Style {
            fg: Some(Color::BrightBlack),
            ..Default::default()
        },
        separator_token: 'x',
        x: Style {
            fg: Some(Color::BrightBlack),
            ..Default::default()
        },
    });

    // ===== 数字 =====
    builder.with_number_highlighter(NumberConfig {
        style: Style {
            fg: Some(Color::BrightCyan),
            ..Style::default()
        },
    });

    builder.build().expect("Failed to build highlighter")
});
