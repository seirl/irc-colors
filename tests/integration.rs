use irc_colors::{Color, ColorValue, Colorize, ExtendedColor, Span, Style, Text};

#[test]
fn chain_and_concat() {
    let msg = "Hello".bold().red() + " world".underline().blue();
    let rendered = msg.to_irc_string();
    assert_eq!(
        rendered,
        "\u{02}\u{03}04Hello\u{0f}\u{1f}\u{03}02 world\u{0f}"
    );
    assert_eq!(msg.plain(), "Hello world");
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
fn borrowed_text_and_span_ops() {
    let a = "Hello".red();
    let b = " world".blue();
    let mut combined = &a + &b;
    combined += &"! ".bold();
    let full = String::from(">> ") + &combined + "end";
    assert_eq!(full.plain(), ">> Hello world! end");
    assert_eq!((&a).bold(), "Hello".red().bold());

    let borrowed_spans: Vec<&Span> = (&full).into_iter().collect();
    assert_eq!(borrowed_spans, full.spans().iter().collect::<Vec<_>>());

    // `&Span` round-trips through `FromIterator`, `Extend`, `+=`, `+`, and `Colorize`.
    let recloned: Text = (&full).into_iter().collect();
    assert_eq!(recloned, full);
    let mut extended = Text::new();
    extended.extend(&b);
    let bang = Span::new("!", Style::new().bold());
    extended += &bang;
    assert_eq!(extended, &b + &bang);
    assert_eq!((&a.spans()[0]).bold(), "Hello".red().bold());

    let owned_spans = full.clone().into_spans();
    let iter_spans: Vec<Span> = full.into_iter().collect();
    assert_eq!(owned_spans, iter_spans);
}

#[test]
fn randomized_round_trip_and_wrap_invariants() {
    // Simple deterministic xorshift64 PRNG so no extra dev-dependency is needed.
    let mut seed: u64 = 0xDEAD_BEEF_CAFE_BABEu64;
    let mut next_u32 = || -> u32 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 32) as u32
    };

    let sample_fragments = [
        "hello",
        " ",
        "123",
        ",04",
        ",FF00AA",
        "0",
        ",",
        "word ",
        "👨\u{200d}👩\u{200d}👦",
        "é",
        " \u{0301}",
        "line\r\nbreak",
        "first\r",
        "\nsecond",
        "\u{02}raw\u{03}04",
        "nul\0byte",
        "ctcp\u{01}tag",
        "superlongfragwithoutspaces",
    ];

    let strip_breaks = |s: &str| -> String {
        s.chars()
            .filter(|&c| !matches!(c, ' ' | '\r' | '\n'))
            .collect()
    };

    for case in 0..500 {
        let span_count = (next_u32() % 8 + 1) as usize;
        let mut t = Text::new();

        for _ in 0..span_count {
            let frag = sample_fragments[(next_u32() as usize) % sample_fragments.len()];
            let fg = match next_u32() % 5 {
                0 => None,
                1 => Color::from_code((next_u32() % 16) as u8).map(ColorValue::Palette),
                2 => ExtendedColor::new(16 + (next_u32() % 83) as u8).map(ColorValue::Extended),
                3 => ColorValue::from_code((next_u32() % 100) as u8),
                _ => Some(ColorValue::Rgb(
                    next_u32() as u8,
                    (next_u32() >> 8) as u8,
                    (next_u32() >> 16) as u8,
                )),
            };
            let bg = match next_u32() % 5 {
                0 | 1 => None,
                2 => Color::from_code((next_u32() % 16) as u8).map(ColorValue::Palette),
                3 => ExtendedColor::new(16 + (next_u32() % 83) as u8).map(ColorValue::Extended),
                _ => Some(ColorValue::Rgb(
                    next_u32() as u8,
                    (next_u32() >> 8) as u8,
                    (next_u32() >> 16) as u8,
                )),
            };
            let bits = next_u32();
            let style = Style {
                fg,
                bg,
                bold: bits & 1 != 0,
                italic: bits & 2 != 0,
                underline: bits & 4 != 0,
                strikethrough: bits & 8 != 0,
                monospace: bits & 16 != 0,
                reverse: bits & 32 != 0,
            };
            t += Text::from(Span::new(frag, style));
        }

        // Mixed palette + RGB in a single span promotes the palette side to its
        // canonical RGB value on the wire; all other styles round-trip identically.
        let normalize_wire = |mut s: Style| {
            let any_rgb = matches!(s.fg, Some(ColorValue::Rgb(..)))
                || matches!(s.bg, Some(ColorValue::Rgb(..)));
            if any_rgb {
                s.fg = s.fg.map(|c| {
                    let (r, g, b) = c.to_rgb();
                    ColorValue::Rgb(r, g, b)
                });
                s.bg = s.bg.map(|c| {
                    let (r, g, b) = c.to_rgb();
                    ColorValue::Rgb(r, g, b)
                });
            }
            s
        };

        let wire = t.to_irc_string();
        assert_eq!(t.wire_len(), wire.len());
        let reparsed = Text::parse(&wire);
        assert_eq!(
            reparsed,
            (&t).styled(normalize_wire),
            "round-trip mismatch on case {}: wire={:?}",
            case,
            wire
        );

        let expected_non_break = strip_breaks(&t.plain());
        for max_bytes in [22usize, 36, 45, 120] {
            let chunks = t.wrap(max_bytes);
            let mut wrapped_plain = String::new();
            for chunk in chunks {
                assert!(!chunk.is_empty());
                let cw = chunk.to_irc_string();
                assert_eq!(chunk.wire_len(), cw.len());
                assert!(
                    !cw.contains('\0')
                        && !cw.contains('\u{01}')
                        && !cw.contains('\r')
                        && !cw.contains('\n')
                );
                // Only when a single grapheme + its own required setup/reset
                // codes exceeds `max_bytes` is a 1-grapheme chunk allowed to
                // exceed `max_bytes` (rather than tearing UTF-8 / ZWJ sequences).
                if chunk.visible_len() > 1 {
                    assert!(
                        cw.len() <= max_bytes,
                        "wrap({}) exceeded max_bytes ({} > {}): {:?}",
                        max_bytes,
                        cw.len(),
                        max_bytes,
                        cw
                    );
                }
                assert_eq!(Text::parse(&cw), (&chunk).styled(normalize_wire));
                wrapped_plain.push_str(&chunk.plain());
            }
            assert_eq!(
                strip_breaks(&wrapped_plain),
                expected_non_break,
                "wrap({}) lost or altered plain text on case {}",
                max_bytes,
                case
            );
        }
    }
}

#[cfg(feature = "format")]
mod format_macro {
    use super::*;
    use irc_colors::irc_format;

    #[test]
    fn irc_format_literals_escapes_and_raw_strings() {
        assert_eq!(irc_format!(""), Text::new());
        assert!(irc_format!("\x02\x0f\0\x01").is_empty());

        let plain = irc_format!("Hello, {{world}}!");
        assert_eq!(plain, Text::raw("Hello, {world}!"));
        assert_eq!(plain.spans().len(), 1);

        let escaped = irc_format!(
            "a\nb\tc\\d\"e'f\x41\u{1F600}\u{1F_600} \
             continued"
        );
        assert_eq!(escaped.plain(), "a\nb\tc\\d\"e'fA😀😀 continued");

        let raw0 = irc_format!(r"raw \n {{literal}} {}", "ok".green());
        assert_eq!(raw0, r"raw \n {literal} " + "ok".green());

        let raw2 = irc_format!(r##"quotes " and "# inside {}"##, 7);
        assert_eq!(raw2.plain(), "quotes \" and \"# inside 7");
    }

    #[test]
    fn irc_format_preserves_styles_and_coalesces_plain_spans() {
        use std::borrow::Cow;

        let nick = String::from("alice");
        let score = 42u32;
        let badge = "VIP".bold().yellow();
        let borrowed_badge = &badge;
        let double_ref = &borrowed_badge;
        let mut mut_text = "mut".italic();
        let mut_ref = &mut mut_text;
        let span = Span::new("span\x02clean", Style::new().cyan());
        let cow: Cow<'_, str> = Cow::Borrowed("cow");

        let msg = irc_format!(
            "User {nick} [{}] ({}/{}/{}/{}/{}/{cow}) scored {score}!",
            "admin".red(),
            badge,
            borrowed_badge,
            double_ref,
            mut_ref,
            &span,
        );
        assert_eq!(
            msg,
            "User alice ["
                + "admin".red()
                + "] ("
                + "VIP".bold().yellow()
                + "/"
                + "VIP".bold().yellow()
                + "/"
                + "VIP".bold().yellow()
                + "/"
                + "mut".italic()
                + "/"
                + "spanclean".cyan()
                + "/cow) scored 42!"
        );
        // `nick` and `badge` were borrowed, not moved!
        assert_eq!(nick, "alice");
        assert_eq!(badge, "VIP".bold().yellow());

        // Purely plain `Display` arguments coalesce into a single `Span`.
        let coalesced = irc_format!("Hello {nick}, {score} items ({})", true);
        assert_eq!(coalesced.spans().len(), 1);
        assert_eq!(coalesced.plain(), "Hello alice, 42 items (true)");
    }

    #[test]
    fn irc_format_sanitizes_untrusted_display_inputs_everywhere() {
        let evil = "\0\x01ACTION \x02bold\x0304red\x04FF0000hex\x0fplain\x01";
        let evil_string = String::from(evil);
        let real = "trusted".bold().green();

        // Implicit capture, positional, named, and Debug (`{:?}`) must all strip control bytes.
        let msg = irc_format!("<{evil}> {} | {named} | {real}", evil_string, named = evil,);
        let clean = Text::raw(evil).plain();
        assert_eq!(clean, "ACTION bold04redFF0000hexplain");
        assert_eq!(
            msg,
            format!("<{clean}> {clean} | {clean} | ") + "trusted".bold().green()
        );
        assert!(!msg.to_irc_string().contains(['\0', '\x01']));

        // Outer `.with_fallback_style` / `.red()` cannot be broken out of by `\x0f` in untrusted input.
        let outer = irc_format!("Alert: {evil}!").red();
        assert_eq!(outer, format!("Alert: {clean}!").red());
        assert_eq!(outer.spans().len(), 1);
        assert_eq!(Text::parse(&outer.to_irc_string()), outer);
    }

    #[test]
    fn irc_format_positional_named_eval_once_and_raw_idents() {
        use std::cell::Cell;

        let calls = Cell::new(0usize);
        let bump = || {
            calls.set(calls.get() + 1);
            "once".bold().blue()
        };

        let r#type = "kind";
        let shadowed = "outer";
        let msg = irc_format!(
            "{1} -> {0} / {0} ({type}, {match}, {shadowed})",
            bump(),
            10,
            r#match = "arm".yellow(),
            shadowed = "inner".red(),
        );
        assert_eq!(shadowed, "outer");
        assert_eq!(calls.get(), 1);
        assert_eq!(
            msg,
            "10 -> "
                + "once".bold().blue()
                + " / "
                + "once".bold().blue()
                + " (kind, "
                + "arm".yellow()
                + ", "
                + "inner".red()
                + ")"
        );
    }

    #[test]
    fn irc_format_all_core_fmt_specifiers() {
        let n = 42i32;
        let hex_val = 0xfeedu32;
        let float_val = 3.14159f64;
        let ptr: *const u8 = 0x1234usize as *const u8;

        let specs = irc_format!(
            "{n:04}|{n:+05}|{hex_val:x}|{hex_val:#X}|{n:b}|{n:#o}|{float_val:.2}|{float_val:.1e}|{float_val:.1E}|{ptr:p}|{:?}|{:x?}|{:X?}|{:}",
            ("a", 10),
            [15u8],
            [15u8],
            "trivial".red(),
        );
        let expected_prefix = format!(
            "{n:04}|{n:+05}|{hex_val:x}|{hex_val:#X}|{n:b}|{n:#o}|{float_val:.2}|{float_val:.1e}|{float_val:.1E}|{ptr:p}|{:?}|{:x?}|{:X?}|",
            ("a", 10),
            [15u8],
            [15u8],
        );
        assert_eq!(specs, expected_prefix + "trivial".red());

        // Alignment and `$`/`*` fill characters (static widths).
        let aligned = irc_format!("[{:>6.2}] [{:^5}] [{:$<4}] [{:*>4}]", "abcde", "hi", "a", 9);
        assert_eq!(
            aligned.plain(),
            format!("[{:>6.2}] [{:^5}] [{:$<4}] [{:*>4}]", "abcde", "hi", "a", 9)
        );
        // Control char used as fill is stripped by `SanitizingWriter`.
        assert_eq!(irc_format!("{:\x02>6}", 42), Text::raw("42"));
    }

    #[test]
    fn irc_format_inside_outer_macro_rules() {
        macro_rules! log_line {
            ($tag:expr, $fmt:literal $(, $arg:expr)* $(,)?) => {
                irc_format!("[{}] {}", $tag.bold().cyan(), irc_format!($fmt $(, $arg)*))
            };
        }

        let count = 3;
        let line = log_line!("INFO", "processed {count} items: {}", "ok".light_green(),);
        assert_eq!(
            line,
            "[" + "INFO".bold().cyan() + "] processed 3 items: " + "ok".light_green()
        );
        assert_eq!(irc_format!("eq={}", count == 3).plain(), "eq=true");
    }
}
