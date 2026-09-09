use irc_color::{Color, ColorValue, Colorize, Text};

#[test]
fn chain_and_concat() {
    let msg = "Hello".bold().red() + " world".underline().blue();
    let rendered = msg.to_irc_string();
    println!("{:?}", rendered);
    assert_eq!(msg.plain(), "Hello world");
}

#[test]
fn later_call_wins() {
    let t = "x".blue().red();
    assert_eq!(t.spans[0].style.fg, Some(ColorValue::Palette(Color::Red)));
}

#[test]
fn fallback_style_child_wins_bg_inherits_formats_union() {
    use irc_color::Style;
    let inner = "SPARTA".bold().blue();
    let theme = Style::new().red().underline();
    let msg = ("This is " + inner.clone()).with_fallback_style(theme);
    assert_eq!(msg.spans[0].style.fg, Some(ColorValue::Palette(Color::Red)));
    assert!(msg.spans[0].style.underline);
    assert_eq!(
        msg.spans[1].style.fg,
        Some(ColorValue::Palette(Color::Blue))
    ); // child wins
    assert!(msg.spans[1].style.bold); // kept its own
    assert!(msg.spans[1].style.underline); // inherited via union

    let msg2 = Text::with_fallback(theme, "This is " + inner);
    assert_eq!(msg, msg2);
}

#[test]
fn wrap_respects_max_bytes_and_preserves_words() {
    let long = "the quick brown fox jumps over the lazy dog ".repeat(5);
    let styled = long.as_str().bold().red();
    let chunks = styled.wrap(60);

    for c in &chunks {
        assert!(
            c.to_irc_string().len() <= 60,
            "chunk too long: {:?}",
            c.to_irc_string()
        );
    }

    // no words lost or mangled
    let rejoined: String = chunks.iter().map(|c| c.plain()).collect();
    assert_eq!(
        rejoined.split_whitespace().collect::<Vec<_>>(),
        long.split_whitespace().collect::<Vec<_>>()
    );
}

#[test]
fn parse_round_trip() {
    let original = "Hello".bold().red() + " world".underline().blue();
    let rendered = original.to_irc_string();
    let reparsed = Text::parse(&rendered);
    assert_eq!(reparsed.plain(), original.plain());
    assert_eq!(reparsed.to_irc_string(), rendered);
}

#[test]
fn new_format_codes_toggle_and_round_trip() {
    let original = "old".strikethrough() + "code".monospace() + "note".italic();
    let rendered = original.to_irc_string();
    assert!(rendered.contains('\u{1e}')); // strikethrough
    assert!(rendered.contains('\u{11}')); // monospace
    assert!(rendered.contains('\u{1d}')); // italic

    let reparsed = Text::parse(&rendered);
    assert_eq!(reparsed.plain(), original.plain());
    assert!(reparsed.spans[0].style.strikethrough);
    assert!(reparsed.spans[1].style.monospace);
    assert!(reparsed.spans[2].style.italic);
}

#[test]
fn hex_color_round_trip() {
    let original = "hot pink".rgb(255, 105, 180).on_rgb(0, 0, 0);
    let rendered = original.to_irc_string();
    assert!(rendered.contains('\u{04}'));
    assert!(rendered.contains("FF69B4"));
    assert!(rendered.contains("000000"));

    let reparsed = Text::parse(&rendered);
    assert_eq!(reparsed.plain(), "hot pink");
    assert_eq!(
        reparsed.spans[0].style.fg,
        Some(ColorValue::Rgb(255, 105, 180))
    );
    assert_eq!(reparsed.spans[0].style.bg, Some(ColorValue::Rgb(0, 0, 0)));
}

#[test]
fn palette_and_hex_are_distinct_kinds() {
    let palette = "x".red();
    let hex = "x".rgb(255, 0, 0);
    assert_ne!(palette.spans[0].style.fg, hex.spans[0].style.fg);
}

#[test]
fn hard_split_overlong_word() {
    let word = "a".repeat(200);
    let styled = word.as_str().bold();
    let chunks = styled.wrap(50);
    assert!(chunks.len() > 1);
    for c in &chunks {
        assert!(c.to_irc_string().len() <= 50);
    }
    let rejoined: String = chunks.iter().map(|c| c.plain()).collect();
    assert_eq!(rejoined, word);
}

// -- regression tests for reviewer-reported bugs -----------------------------

#[test]
fn parse_does_not_swallow_a_literal_comma_after_a_color_code() {
    // A real bug: "\x0304, but wait..." used to parse the comma as the
    // start of a background spec, fail to find digits, and drop it.
    let parsed = Text::parse("\u{03}04, but wait...");
    assert_eq!(parsed.plain(), ", but wait...");
}

#[test]
fn parse_supports_background_only_color_spec() {
    // \x03,04 (bare fg, valid bg digits) is a real form our own serializer
    // can emit when fg is None but bg is set - must round-trip correctly.
    let parsed = Text::parse("\u{03},04text");
    assert_eq!(parsed.plain(), "text");
    assert_eq!(parsed.spans[0].style.fg, None);
    assert_eq!(
        parsed.spans[0].style.bg,
        Some(ColorValue::Palette(Color::Red))
    );
}

#[test]
fn hex_and_palette_mixed_in_one_span_keeps_both_colors() {
    use irc_color::Style;
    // Only reachable via fallback style composition, not the ordinary builder.
    let base = "text".red(); // palette fg only
    let merged = base.with_fallback_style(Style::new().on_rgb(10, 20, 30));
    let rendered = merged.to_irc_string();
    let reparsed = Text::parse(&rendered);
    // Foreground must survive as its RGB equivalent, not vanish.
    assert_eq!(reparsed.spans[0].style.fg, Some(ColorValue::Rgb(255, 0, 0)));
    assert_eq!(
        reparsed.spans[0].style.bg,
        Some(ColorValue::Rgb(10, 20, 30))
    );
}

#[test]
fn wrap_never_tears_a_grapheme_cluster() {
    let family = "👨\u{200d}👩\u{200d}👦"; // one grapheme, three chars/emoji joined by ZWJ
    let text = Text::raw(family.repeat(3));
    for max in [1, 4, 8, 20, 100] {
        let chunks = text.wrap(max);
        let rejoined: String = chunks.iter().map(|c| c.plain()).collect();
        assert_eq!(rejoined, family.repeat(3), "corrupted at max_bytes={}", max);
        // every emoji must appear whole in exactly one chunk
        for chunk in &chunks {
            let plain = chunk.plain();
            let mut rest = plain.as_str();
            while let Some(pos) = rest.find('\u{200d}') {
                // a ZWJ must always have a char immediately before and after it
                // within the SAME chunk - i.e. never at a chunk boundary edge in
                // a way that would separate it from its neighbors.
                assert!(pos > 0 && pos + '\u{200d}'.len_utf8() < rest.len());
                rest = &rest[pos + '\u{200d}'.len_utf8()..];
            }
        }
    }
}

#[test]
fn add_assign_builds_incrementally() {
    let mut msg = Text::new();
    msg += "Hello".bold().red();
    msg += " ";
    msg += "world".to_string();
    assert_eq!(msg.plain(), "Hello world");
}

#[test]
fn wrap_scales_linearly_not_quadratically() {
    // Not a strict perf assertion (too flaky in CI), just a sanity check
    // that doubling the input doesn't blow up disproportionately.
    use std::time::Instant;
    let make = |n: usize| {
        "the quick brown fox jumps over the lazy dog "
            .repeat(n)
            .as_str()
            .bold()
            .red()
    };

    let small = make(500);
    let t0 = Instant::now();
    let _ = small.wrap(400);
    let small_time = t0.elapsed();

    let big = make(4000); // 8x the input
    let t0 = Instant::now();
    let _ = big.wrap(400);
    let big_time = t0.elapsed();

    // Quadratic growth would be ~64x; allow generous slack but catch a real regression.
    assert!(
        big_time.as_secs_f64() < small_time.as_secs_f64() * 20.0 + 0.05,
        "small={:?} big={:?} - looks worse than linear",
        small_time,
        big_time
    );
}

#[test]
fn background_is_cleared_when_next_span_has_foreground_only() {
    let msg = "colored bg".red().on_blue() + " no bg".green();
    let rendered = msg.to_irc_string();
    let reparsed = Text::parse(&rendered);

    assert_eq!(reparsed.spans[0].style.fg, Some(ColorValue::Palette(Color::Red)));
    assert_eq!(reparsed.spans[0].style.bg, Some(ColorValue::Palette(Color::Blue)));

    assert_eq!(reparsed.spans[1].style.fg, Some(ColorValue::Palette(Color::Green)));
    assert_eq!(
        reparsed.spans[1].style.bg,
        None,
        "background color should have been cleared, but was: {:?}",
        reparsed.spans[1].style.bg
    );
}

#[test]
fn parse_does_not_swallow_comma_digit_after_foreground_color() {
    let msg = ",50 USD".red();
    let rendered = msg.to_irc_string();
    let reparsed = Text::parse(&rendered);
    assert_eq!(reparsed.plain(), ",50 USD");
    assert_eq!(reparsed.spans[0].style.fg, Some(ColorValue::Palette(Color::Red)));
    assert_eq!(reparsed.spans[0].style.bg, None);
}

#[test]
fn hex_background_only_round_trip() {
    let original = "text".on_rgb(255, 0, 0);
    let rendered = original.to_irc_string();
    let reparsed = Text::parse(&rendered);
    assert_eq!(reparsed.plain(), "text");
    assert_eq!(reparsed.spans[0].style.fg, None);
    assert_eq!(reparsed.spans[0].style.bg, Some(ColorValue::Rgb(255, 0, 0)));
}

#[test]
fn string_add_text() {
    let s = String::from("Hello, ");
    let styled = s + "world".bold().red();
    assert_eq!(styled.plain(), "Hello, world");
    assert!(styled.spans[1].style.bold);
}

#[test]
fn parse_comprehensive_edge_cases() {
    // 1-digit code
    let t1 = Text::parse("\u{03}4Hello");
    assert_eq!(t1.plain(), "Hello");
    assert_eq!(t1.spans[0].style.fg, Some(ColorValue::Palette(Color::Red)));

    // 1-digit fg, 1-digit bg
    let t2 = Text::parse("\u{03}4,2Hello");
    assert_eq!(t2.plain(), "Hello");
    assert_eq!(t2.spans[0].style.fg, Some(ColorValue::Palette(Color::Red)));
    assert_eq!(t2.spans[0].style.bg, Some(ColorValue::Palette(Color::Blue)));

    // Bare reset in middle
    let t3 = Text::parse("\u{03}04Red\u{03}Plain");
    assert_eq!(t3.plain(), "RedPlain");
    assert_eq!(t3.spans[0].style.fg, Some(ColorValue::Palette(Color::Red)));
    assert_eq!(t3.spans[1].style.fg, None);

    // Hex with lowercase digits
    let t4 = Text::parse("\u{04}00ff00,0000ffText");
    assert_eq!(t4.plain(), "Text");
    assert_eq!(t4.spans[0].style.fg, Some(ColorValue::Rgb(0, 255, 0)));
    assert_eq!(t4.spans[0].style.bg, Some(ColorValue::Rgb(0, 0, 255)));

    // Incomplete hex digits (e.g. only 4 digits) does not eat text
    let t5 = Text::parse("\u{04}FFAAText");
    assert_eq!(t5.plain(), "FFAAText");
    assert_eq!(t5.spans[0].style.fg, None);
}
