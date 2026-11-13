use irc_colors::{Color, ColorValue, Colorize, ExtendedColor, Text};

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
fn later_call_wins() {
    let t = "x".blue().red();
    assert_eq!(t.spans()[0].style.fg, Some(ColorValue::Palette(Color::Red)));
}

#[test]
fn fallback_style_child_wins_bg_inherits_formats_union() {
    use irc_colors::Style;
    const THEME: Style = Style::new().red().underline();
    let inner = "SPARTA".bold().blue();
    let msg = ("This is " + &inner).with_fallback_style(THEME);
    assert_eq!(
        msg.spans()[0].style.fg,
        Some(ColorValue::Palette(Color::Red))
    );
    assert!(msg.spans()[0].style.underline);
    assert_eq!(
        msg.spans()[1].style.fg,
        Some(ColorValue::Palette(Color::Blue))
    ); // child wins
    assert!(msg.spans()[1].style.bold); // kept its own
    assert!(msg.spans()[1].style.underline); // inherited via union

    let plain_fallback =
        "This is ".with_fallback_style(THEME) + (&inner).with_fallback_style(THEME);
    assert_eq!(msg, plain_fallback);
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

    // no words lost or mangled, and wrapped lines carry no trailing break-point space
    for c in &chunks[..chunks.len() - 1] {
        assert!(
            !c.plain().ends_with(' '),
            "trailing space in {:?}",
            c.plain()
        );
    }
    let rejoined = chunks
        .iter()
        .map(|c| c.plain())
        .collect::<Vec<_>>()
        .join(" ");
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
    assert!(reparsed.spans()[0].style.strikethrough);
    assert!(reparsed.spans()[1].style.monospace);
    assert!(reparsed.spans()[2].style.italic);
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
        reparsed.spans()[0].style.fg,
        Some(ColorValue::Rgb(255, 105, 180))
    );
    assert_eq!(reparsed.spans()[0].style.bg, Some(ColorValue::Rgb(0, 0, 0)));
}

#[test]
fn palette_and_hex_are_distinct_kinds() {
    let palette = "x".red();
    let hex = "x".rgb(255, 0, 0);
    assert_ne!(palette.spans()[0].style.fg, hex.spans()[0].style.fg);

    for code in 0..=15u8 {
        let color = Color::from_code(code).expect("0..=15 must map to a Color");
        assert_eq!(color.code(), code);
    }
    assert_eq!(Color::from_code(16), None);
    assert_eq!(Color::from_code(99), None);
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
fn parse_bare_x03_before_comma_digits_resets_color_without_eating_text() {
    use irc_colors::Style;

    // Bare `\x03` has no `<fg>` digits, so `,50` must stay literal text rather
    // than being swallowed as a background code (e.g. `\x0304$1\x03,500`).
    let price = Text::parse("\u{02}Total: \u{03}04$1\u{03},500\u{02}");
    assert_eq!(price.plain(), "Total: $1,500");
    assert_eq!(price.spans()[0].style, Style::new().bold());
    assert_eq!(price.spans()[1].style, Style::new().bold().red());
    assert_eq!(price.spans()[1].text, "$1");
    assert_eq!(price.spans()[2].style, Style::new().bold());
    assert_eq!(price.spans()[2].text, ",500");

    let bare_comma = Text::parse("\u{03},04text");
    assert_eq!(bare_comma.plain(), ",04text");
    assert_eq!(bare_comma.spans()[0].style, Style::default());

    // `\x04` has no `99` default-fg placeholder, so `\x04,<hex>` continues to
    // update `bg` while preserving any active `fg`.
    let kept_hex_fg = Text::parse("\u{04}0000FFhi\u{04},FF0000there");
    assert_eq!(
        kept_hex_fg.spans()[1].style.fg,
        Some(ColorValue::Rgb(0, 0, 255))
    );
    assert_eq!(
        kept_hex_fg.spans()[1].style.bg,
        Some(ColorValue::Rgb(255, 0, 0))
    );
}

#[test]
fn hex_and_palette_mixed_in_one_span_keeps_both_colors() {
    use irc_colors::Style;

    // Direct `Colorize` chain mixing palette fg with RGB bg (and vice versa).
    let direct = "x".red().on_rgb(1, 2, 3);
    assert_eq!(
        direct.spans()[0].style.fg,
        Some(ColorValue::Palette(Color::Red))
    );
    assert_eq!(direct.spans()[0].style.bg, Some(ColorValue::Rgb(1, 2, 3)));
    assert_eq!(direct.to_irc_string(), "\u{04}FF0000,010203x\u{0f}");
    let direct_reparsed = Text::parse(&direct.to_irc_string());
    assert_eq!(
        direct_reparsed.spans()[0].style.fg,
        Some(ColorValue::Rgb(255, 0, 0))
    );
    assert_eq!(
        direct_reparsed.spans()[0].style.bg,
        Some(ColorValue::Rgb(1, 2, 3))
    );

    let reverse_mix = "x".rgb(1, 2, 3).on_blue();
    assert_eq!(reverse_mix.to_irc_string(), "\u{04}010203,00007Fx\u{0f}");

    // Also reachable via fallback style composition.
    let base = "text".red(); // palette fg only
    let merged = base.with_fallback_style(Style::new().on_rgb(10, 20, 30));
    let rendered = merged.to_irc_string();
    let reparsed = Text::parse(&rendered);
    // Foreground must survive as its RGB equivalent, not vanish.
    assert_eq!(
        reparsed.spans()[0].style.fg,
        Some(ColorValue::Rgb(255, 0, 0))
    );
    assert_eq!(
        reparsed.spans()[0].style.bg,
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

    msg.clear();
    assert!(msg.is_empty());
    msg += "reused".blue();
    assert_eq!(msg, "reused".blue());
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
    let big = make(4000); // 8x the input

    // Warm up allocator/caches and take min of 3 runs to avoid OS scheduling jitter.
    let _ = small.wrap(400);
    let min_elapsed = |t: &Text| {
        (0..3)
            .map(|_| {
                let t0 = Instant::now();
                let _ = t.wrap(400);
                t0.elapsed()
            })
            .min()
            .unwrap()
    };

    let small_time = min_elapsed(&small);
    let big_time = min_elapsed(&big);

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

    assert_eq!(
        reparsed.spans()[0].style.fg,
        Some(ColorValue::Palette(Color::Red))
    );
    assert_eq!(
        reparsed.spans()[0].style.bg,
        Some(ColorValue::Palette(Color::Blue))
    );

    assert_eq!(
        reparsed.spans()[1].style.fg,
        Some(ColorValue::Palette(Color::Green))
    );
    assert_eq!(
        reparsed.spans()[1].style.bg,
        None,
        "background color should have been cleared, but was: {:?}",
        reparsed.spans()[1].style.bg
    );
}

#[test]
fn parse_does_not_swallow_comma_digit_after_foreground_color() {
    let msg = ",50 USD".red();
    let rendered = msg.to_irc_string();
    let reparsed = Text::parse(&rendered);
    assert_eq!(reparsed.plain(), ",50 USD");
    assert_eq!(
        reparsed.spans()[0].style.fg,
        Some(ColorValue::Palette(Color::Red))
    );
    assert_eq!(reparsed.spans()[0].style.bg, None);
}

#[test]
fn hex_background_only_round_trip() {
    let original = "text".on_rgb(255, 0, 0);
    let rendered = original.to_irc_string();
    let reparsed = Text::parse(&rendered);
    assert_eq!(reparsed.plain(), "text");
    assert_eq!(reparsed.spans()[0].style.fg, None);
    assert_eq!(
        reparsed.spans()[0].style.bg,
        Some(ColorValue::Rgb(255, 0, 0))
    );
}

#[test]
fn string_add_text() {
    let s = String::from("Hello, ");
    let borrowed = (&s).bold();
    assert_eq!(borrowed.plain(), "Hello, ");

    let ref_lhs = &s + "world".bold().red();
    assert_eq!(ref_lhs.plain(), "Hello, world");

    let suffix = String::from("!");
    let mut full = s + "world".bold().red() + &suffix;
    full += &suffix;
    assert_eq!(full.plain(), "Hello, world!!");
    assert!(full.spans()[1].style.bold);
}

#[test]
fn parse_comprehensive_edge_cases() {
    // 1-digit code
    let t1 = Text::parse("\u{03}4Hello");
    assert_eq!(t1.plain(), "Hello");
    assert_eq!(
        t1.spans()[0].style.fg,
        Some(ColorValue::Palette(Color::Red))
    );

    // 1-digit fg, 1-digit bg
    let t2 = Text::parse("\u{03}4,2Hello");
    assert_eq!(t2.plain(), "Hello");
    assert_eq!(
        t2.spans()[0].style.fg,
        Some(ColorValue::Palette(Color::Red))
    );
    assert_eq!(
        t2.spans()[0].style.bg,
        Some(ColorValue::Palette(Color::Blue))
    );

    // Bare reset in middle
    let t3 = Text::parse("\u{03}04Red\u{03}Plain");
    assert_eq!(t3.plain(), "RedPlain");
    assert_eq!(
        t3.spans()[0].style.fg,
        Some(ColorValue::Palette(Color::Red))
    );
    assert_eq!(t3.spans()[1].style.fg, None);

    // Hex with lowercase digits
    let t4 = Text::parse("\u{04}00ff00,0000ffText");
    assert_eq!(t4.plain(), "Text");
    assert_eq!(t4.spans()[0].style.fg, Some(ColorValue::Rgb(0, 255, 0)));
    assert_eq!(t4.spans()[0].style.bg, Some(ColorValue::Rgb(0, 0, 255)));

    // Incomplete hex digits (e.g. only 4 digits) does not eat text
    let t5 = Text::parse("\u{04}FFAAText");
    assert_eq!(t5.plain(), "FFAAText");
    assert_eq!(t5.spans()[0].style.fg, None);
}

#[test]
fn palette_background_only_emits_spec_compliant_99_and_round_trips() {
    let original = "alert".on_red();
    let rendered = original.to_irc_string();
    // Must use spec-compliant `99` default-fg placeholder rather than `\x03,04`,
    // so real IRC clients never print `,04` as literal text.
    assert_eq!(rendered, "\u{03}99,04alert\u{0f}");

    let reparsed = Text::parse(&rendered);
    assert_eq!(reparsed.plain(), "alert");
    assert_eq!(reparsed.spans()[0].style.fg, None);
    assert_eq!(
        reparsed.spans()[0].style.bg,
        Some(ColorValue::Palette(Color::Red))
    );
}

#[test]
fn foreground_to_background_only_clears_foreground() {
    let msg = "fg".red() + "bg".on_blue();
    let rendered = msg.to_irc_string();

    let reparsed = Text::parse(&rendered);
    assert_eq!(
        reparsed.spans()[0].style.fg,
        Some(ColorValue::Palette(Color::Red))
    );
    assert_eq!(reparsed.spans()[0].style.bg, None);
    assert_eq!(reparsed.spans()[1].style.fg, None);
    assert_eq!(
        reparsed.spans()[1].style.bg,
        Some(ColorValue::Palette(Color::Blue))
    );
}

#[test]
fn wrap_does_not_create_spurious_space_line_at_exact_boundary() {
    // "hello" is 5 bytes; with max_bytes=5, the separating space must not
    // force a 3-line split ["hello", " ", "world"].
    let chunks = Text::raw("hello world").wrap(5);
    let plains: Vec<String> = chunks.iter().map(|c| c.plain()).collect();
    assert_eq!(plains, vec!["hello", "world"]);
    for c in &chunks {
        assert!(c.to_irc_string().len() <= 5);
    }

    // Same when an overlong word is an exact multiple of max_bytes followed by a space.
    let chunks2 = Text::raw("abcdefghijkl world").wrap(6);
    let plains2: Vec<String> = chunks2.iter().map(|c| c.plain()).collect();
    assert_eq!(plains2, vec!["abcdef", "ghijkl", "world"]);
    for c in &chunks2 {
        assert!(c.to_irc_string().len() <= 6);
    }
}

#[test]
fn wrap_hard_split_packs_remainder_with_following_short_word() {
    // "abcdefgh" (8) split at max_bytes=6 -> "abcdef" (6) + "gh" (2), and
    // " ij" (3) fits right onto the second line ("gh ij" = 5 <= 6)!
    let chunks = Text::raw("abcdefgh ij").wrap(6);
    let plains: Vec<String> = chunks.iter().map(|c| c.plain()).collect();
    assert_eq!(plains, vec!["abcdef", "gh ij"]);
    for c in &chunks {
        assert!(c.to_irc_string().len() <= 6);
    }
}

#[test]
fn wrap_consumes_break_point_whitespace_and_preserves_words() {
    // "the quick" is 9 bytes; at max_bytes=9 the break-point space before
    // "brown" is consumed so "the quick" fits on line 1 without trailing space.
    let exact = Text::raw("the quick brown");
    let chunks = exact.wrap(9);
    let plains: Vec<String> = chunks.iter().map(|c| c.plain()).collect();
    assert_eq!(plains, vec!["the quick", "brown"]);
    for c in &chunks {
        assert!(c.to_irc_string().len() <= 9);
    }

    // Width consistency: "hello world" wraps to ["hello", "world"] at both 5 and 6.
    for max in [5, 6] {
        let hw: Vec<String> = Text::raw("hello world")
            .wrap(max)
            .iter()
            .map(|c| c.plain())
            .collect();
        assert_eq!(hw, vec!["hello", "world"]);
    }

    // Styled backgrounds/underlines don't leave a dangling styled space cell at line end.
    let bg_chunks = "hello world".underline().on_red().wrap(15);
    assert_eq!(bg_chunks.len(), 2);
    assert_eq!(bg_chunks[0].to_irc_string(), "\u{1f}\u{03}99,04hello\u{0f}");
    assert_eq!(bg_chunks[1].to_irc_string(), "\u{1f}\u{03}99,04world\u{0f}");

    // Interior multiple spaces and leading spaces stay intact when not broken across lines.
    let preserved = Text::raw("  col1   col2").wrap(40);
    assert_eq!(preserved.len(), 1);
    assert_eq!(preserved[0].plain(), "  col1   col2");

    // Leading spaces that push the first word past `max_bytes` are consumed
    // rather than tearing a fitting word in half or emitting a space-only line.
    let lead1: Vec<String> = Text::raw("    hello")
        .wrap(7)
        .iter()
        .map(|c| c.plain())
        .collect();
    assert_eq!(lead1, vec!["hello"]);

    let lead2: Vec<String> = Text::raw("  a").wrap(2).iter().map(|c| c.plain()).collect();
    assert_eq!(lead2, vec!["a"]);

    let all_spaces = Text::raw("      ").wrap(4);
    assert!(all_spaces.is_empty());

    // Multi-span words stay glued together across wraps instead of splitting at span boundaries.
    let multi_span = "prefix " + "hel".red() + "lo".blue();
    let ms_chunks = multi_span.wrap(16);
    let ms_plains: Vec<String> = ms_chunks.iter().map(|c| c.plain()).collect();
    assert_eq!(ms_plains, vec!["prefix", "hello"]);

    // Sweep every max_bytes alignment (5..=40) across plain and styled text:
    // whenever max_bytes can fit each individual word, joining chunk.plain()
    // with " " reproduces the sentence with no trailing spaces on any chunk.
    let sentence = "the quick brown fox jumps over the lazy dog";
    for max in 5..=40 {
        let plain_chunks = Text::raw(sentence).wrap(max);
        for c in &plain_chunks {
            assert!(c.to_irc_string().len() <= max);
            assert!(!c.plain().ends_with(' '));
            assert!(!c.plain().starts_with(' '));
        }
        let plain_rejoined = plain_chunks
            .iter()
            .map(|c| c.plain())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(plain_rejoined, sentence, "mismatch at max_bytes={}", max);
    }
    // Styled: longest word ("quick" / "brown" / "jumps") + bold(1) + red(3) + reset(1) = 10 bytes.
    let styled = sentence.bold().red();
    for max in 10..=45 {
        let styled_chunks = styled.wrap(max);
        for c in &styled_chunks {
            assert!(c.to_irc_string().len() <= max);
            assert!(!c.plain().ends_with(' '));
            assert!(!c.plain().starts_with(' '));
        }
        let styled_rejoined = styled_chunks
            .iter()
            .map(|c| c.plain())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            styled_rejoined, sentence,
            "mismatch (styled) at max_bytes={}",
            max
        );
    }
}

#[test]
fn crlf_and_cr_are_split_cleanly_including_across_spans() {
    use irc_colors::{Span, Style};

    let msg = "line1\r\nline2\rline3\nline4".bold().red();
    let lines = msg.split_lines();
    let plains: Vec<String> = lines.iter().map(|l| l.plain()).collect();
    assert_eq!(plains, vec!["line1", "line2", "line3", "line4"]);
    for l in &lines {
        let wire = l.to_irc_string();
        assert!(!wire.contains('\r'));
        assert!(!wire.contains('\n'));
    }

    // `\r\n` split across two spans (even with an empty span collected in between)
    // must still count as a single line break, not two.
    let cross_span: Text = [
        Span::new("first\r", Style::new().red()),
        Span::new("", Style::new().green()),
        Span::new("\nsecond", Style::new().blue()),
    ]
    .into_iter()
    .collect();
    let cross_lines = cross_span.split_lines();
    let cross_plains: Vec<String> = cross_lines.iter().map(|l| l.plain()).collect();
    assert_eq!(cross_plains, vec!["first", "second"]);

    // Regression test: `to_irc_string()` itself must stay lossless and agree
    // with `plain()` rather than silently deleting `\r` behind `plain()`'s back.
    let raw_unsplit = "keep\rme".red();
    assert_eq!(
        Text::parse(&raw_unsplit.to_irc_string()).plain(),
        raw_unsplit.plain()
    );
}

#[test]
fn empty_spans_do_not_break_digit_disambiguation() {
    use irc_colors::{Span, Style};
    let tricky: Text = [
        Span::new("hi", Style::new().red()),
        Span::new("", Style::default()),
        Span::new("5 apples", Style::default()),
    ]
    .into_iter()
    .collect();
    let rendered = tricky.to_irc_string();
    let reparsed = Text::parse(&rendered);
    assert_eq!(reparsed.plain(), "hi5 apples");
}

#[test]
fn style_transitions_reset_with_x0f() {
    let to_plain = "fancy".bold().italic().underline().strikethrough().red() + "123";
    let rendered = to_plain.to_irc_string();
    assert!(
        rendered.ends_with("\u{0f}123"),
        "rendered was {:?}",
        rendered
    );
    assert_eq!(Text::parse(&rendered), to_plain);

    let overlap = "a".bold().red() + "b".bold().blue();
    assert_eq!(
        overlap.to_irc_string(),
        "\u{02}\u{03}04a\u{0f}\u{02}\u{03}02b\u{0f}"
    );
    assert_eq!(Text::parse(&overlap.to_irc_string()), overlap);
}

#[test]
fn parse_stateful_toggles_and_inherited_background() {
    use irc_colors::Style;

    // Raw IRC from other clients toggles individual flags off without `\x0f`.
    let toggled = Text::parse(
        "\u{02}b\u{02}\u{1d}i\u{1d}\u{1f}u\u{1f}\u{1e}s\u{1e}\u{11}m\u{11}\u{16}r\u{16}p",
    );
    assert_eq!(toggled.plain(), "biusmrp");
    assert_eq!(toggled.spans()[0].style, Style::new().bold());
    assert_eq!(toggled.spans()[1].style, Style::new().italic());
    assert_eq!(toggled.spans()[2].style, Style::new().underline());
    assert_eq!(toggled.spans()[3].style, Style::new().strikethrough());
    assert_eq!(toggled.spans()[4].style, Style::new().monospace());
    assert_eq!(toggled.spans()[5].style, Style::new().reverse());
    assert_eq!(toggled.spans()[6].style, Style::default());

    // Foreground-only `\x03<fg>` / `\x04<hex>` preserves an active background,
    // while bare `\x03` / `\x04` clears both colors without clearing bold.
    let inherited_pal = Text::parse("\u{02}\u{03}04,02a\u{03}03b\u{03}c");
    assert_eq!(
        inherited_pal.spans()[0].style,
        Style::new().bold().red().on_blue()
    );
    assert_eq!(
        inherited_pal.spans()[1].style,
        Style::new().bold().green().on_blue()
    );
    assert_eq!(inherited_pal.spans()[2].style, Style::new().bold());

    let inherited_hex = Text::parse("\u{04}FF0000,0000FFa\u{04}00FF00b\u{04}c");
    assert_eq!(
        inherited_hex.spans()[0].style,
        Style::new().rgb(255, 0, 0).on_rgb(0, 0, 255)
    );
    assert_eq!(
        inherited_hex.spans()[1].style,
        Style::new().rgb(0, 255, 0).on_rgb(0, 0, 255)
    );
    assert_eq!(inherited_hex.spans()[2].style, Style::default());
}

#[test]
fn reverse_round_trip() {
    use irc_colors::Style;

    let direct = "inverted".reverse();
    assert_eq!(direct.to_irc_string(), "\u{16}inverted\u{0f}");
    assert_eq!(Text::parse(&direct.to_irc_string()), direct);

    let warn = Style::new().bold().reverse().grey().on_light_grey();
    let msg = "inverted".with_style(warn);
    let wire = msg.to_irc_string();
    assert!(wire.contains('\u{16}'));

    let reparsed = Text::parse(&wire);
    assert_eq!(reparsed, msg);
}

#[test]
fn colorize_reset_clears_style_and_coalesces_spans() {
    let styled = "Hello ".bold().red() + "world".underline().blue();
    assert_eq!(styled.spans().len(), 2);

    let cleared = styled.reset();
    assert_eq!(cleared, Text::raw("Hello world"));
    assert_eq!(cleared.spans().len(), 1);
    assert_eq!(cleared.to_irc_string(), "Hello world");
}

#[test]
fn visible_len_counts_grapheme_clusters() {
    assert_eq!(Text::new().visible_len(), 0);
    // ZWJ family sequence (1 grapheme, 11 UTF-8 bytes) + combining accent "e\u{0301}" (1 grapheme) + "!" (1 grapheme)
    let msg = "👨\u{200d}👩\u{200d}👦".bold().red() + "e\u{0301}!".blue();
    assert_eq!(msg.visible_len(), 3);
}

#[test]
fn randomized_round_trip_and_wrap_invariants() {
    use irc_colors::{Span, Style};

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

#[test]
fn spans_are_canonical_and_strip_embedded_format_bytes() {
    use irc_colors::{Span, Style};

    const EMPTY: Text = Text::new();
    assert!(EMPTY.is_empty());
    assert!(Text::raw("").is_empty());
    assert!(Text::from(Span::new("", Style::new().red())).is_empty());
    assert!(Text::raw("\u{02}\u{0f}").is_empty());

    // Collecting adjacent matching spans and empty spans produces canonical equality.
    let built: Text = [
        Span::new("hel", Style::new().red()),
        Span::new("", Style::new().blue()),
        Span::new("lo", Style::new().red()),
    ]
    .into_iter()
    .collect();
    assert_eq!(built, "hello".red());
    assert_eq!(built.spans().len(), 1);

    // Embedded IRC control bytes inside untrusted plain text cannot break out of outer styles.
    let untrusted = "safe\u{0f}not-plain\u{02}not-bold";
    let msg = ("Alert: " + Text::raw(untrusted)).red();
    assert_eq!(msg.plain(), "Alert: safenot-plainnot-bold");
    assert_eq!(Text::parse(&msg.to_irc_string()), msg);
}

#[test]
fn wrap_omits_empty_chunks() {
    assert!(Text::new().wrap(400).is_empty());
    assert!(Text::raw("").wrap(400).is_empty());
    assert!(Text::raw("\n\r\n\r").wrap(400).is_empty());

    let trailing_newline = "hello\n\nworld\r\n".bold().red().wrap(400);
    let plains: Vec<String> = trailing_newline.iter().map(|c| c.plain()).collect();
    assert_eq!(plains, vec!["hello", "world"]);
    for c in &trailing_newline {
        assert!(!c.is_empty());
    }

    // Spaces that actually fit within max_bytes remain a valid non-empty line.
    let fitting_spaces = Text::raw("   ").wrap(10);
    assert_eq!(fitting_spaces, vec![Text::raw("   ")]);
}

#[test]
fn const_style_builders() {
    use irc_colors::Style;

    const BASE: Style = Style::new().italic().on_rgb(1, 2, 3);
    const WARN: Style = Style::new()
        .bold()
        .underline()
        .strikethrough()
        .monospace()
        .reverse()
        .yellow()
        .on_black()
        .with_fallback(BASE);
    const _: () = {
        assert!(WARN.bold && WARN.italic && WARN.underline && WARN.strikethrough);
        assert!(WARN.monospace && WARN.reverse);
    };
    assert_eq!(WARN.fg, Some(ColorValue::Palette(Color::Yellow)));
    assert_eq!(WARN.bg, Some(ColorValue::Palette(Color::Black)));

    const INHERITED_BG: Style = Style::new().rgb(10, 20, 30).with_fallback(BASE);
    assert_eq!(INHERITED_BG.fg, Some(ColorValue::Rgb(10, 20, 30)));
    assert_eq!(INHERITED_BG.bg, Some(ColorValue::Rgb(1, 2, 3)));

    const EXT: Style = Style::new()
        .extended(ExtendedColor::ALL[0])
        .on_extended(ExtendedColor::ALL[82]);
    assert_eq!(EXT.fg, ColorValue::from_code(16));
    assert_eq!(EXT.bg, ColorValue::from_code(98));
}

#[test]
fn tuple_rgb_conversion() {
    let tuple_colored = "rgb".fg((255, 105, 180)).bg((0, 0, 0));
    assert_eq!(tuple_colored, "rgb".rgb(255, 105, 180).on_rgb(0, 0, 0));
}

#[test]
fn borrowed_text_and_span_ops() {
    use irc_colors::{Span, Style};

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
fn nul_and_ctcp_bytes_are_stripped() {
    assert!(Text::raw("\0").is_empty());
    assert_eq!(Text::raw("hel\0lo").plain(), "hello");
    assert_eq!(("hel\0lo".red()).to_irc_string(), "\u{03}04hello\u{0f}");
    assert_eq!(Text::parse("\u{02}hel\0lo\u{0f}"), "hello".bold());

    assert!(Text::raw("\u{01}").is_empty());
    assert_eq!(
        Text::raw("\u{01}ACTION slaps everyone\u{01}").plain(),
        "ACTION slaps everyone"
    );
    assert_eq!(
        String::from("\u{01}VERSION\u{01}").bold().to_irc_string(),
        "\u{02}VERSION\u{0f}"
    );
    assert_eq!(
        Text::parse("\u{01}ACTION \u{02}waves\u{0f}\u{01}"),
        "ACTION " + "waves".bold()
    );
}

#[test]
fn color_all_matches_codes() {
    assert_eq!(Color::ALL.len(), 16);
    for (idx, color) in Color::ALL.into_iter().enumerate() {
        assert_eq!(color.code() as usize, idx);
        assert_eq!(Color::from_code(idx as u8), Some(color));
    }
}

#[test]
fn cow_works_with_raw_and_deref_colorize() {
    use std::borrow::Cow;

    let borrowed: Cow<'_, str> = Cow::Borrowed("hello");
    let owned: Cow<'_, str> = Cow::Owned(String::from(" world"));

    assert_eq!(Text::raw(Cow::Borrowed("\u{02}\u{0f}")), Text::new());
    assert_eq!(Text::raw(owned.clone()).plain(), " world");
    // Without `From<Cow<str>>`, both `cow.red()` and `(&cow).red()` work via deref to `&str`
    // without moving `cow`.
    assert_eq!(borrowed.red(), (&borrowed).red());
    assert_eq!((owned.bold() + &*borrowed).plain(), " worldhello");
}

#[test]
fn same_color_transitions_toggle_in_place() {
    let nested_color = ("a " + "b".bold() + " c").red();
    assert_eq!(
        nested_color.to_irc_string(),
        "\u{03}04a \u{02}b\u{02} c\u{0f}"
    );
    assert_eq!(Text::parse(&nested_color.to_irc_string()), nested_color);

    let nested_plain = ("a " + "b".italic() + " c").bold();
    assert_eq!(
        nested_plain.to_irc_string(),
        "\u{02}a \u{1d}b\u{1d} c\u{0f}"
    );
    assert_eq!(Text::parse(&nested_plain.to_irc_string()), nested_plain);

    // Comma+digit on a toggled span stays safe both inline and across a wrap break.
    let comma_toggle = ("hello " + ",5".bold()).red();
    assert_eq!(comma_toggle.to_irc_string(), "\u{03}04hello \u{02},5\u{0f}");
    assert_eq!(Text::parse(&comma_toggle.to_irc_string()), comma_toggle);

    let wrapped = comma_toggle.wrap(10);
    assert_eq!(wrapped.len(), 2);
    assert_eq!(wrapped[0].to_irc_string(), "\u{03}04hello\u{0f}");
    assert_eq!(
        wrapped[1].to_irc_string(),
        "\u{02}\u{03}04\u{02}\u{02},5\u{0f}"
    );
    assert_eq!(Text::parse(&wrapped[1].to_irc_string()), ",5".bold().red());
}

#[test]
fn text_join_separates_items_and_coalesces_styles() {
    assert_eq!(Text::join(std::iter::empty::<Text>(), ", "), Text::new());
    assert_eq!(Text::join(["only".red()], ", "), "only".red());

    let nicks = ["alice", "bob", "carol"];
    let joined = Text::join(nicks.iter().map(|n| n.bold()), ", ".grey());
    assert_eq!(
        joined,
        "alice".bold() + ", ".grey() + "bob".bold() + ", ".grey() + "carol".bold()
    );

    // Adjacent matching styles across items and separators still coalesce into one span.
    let same_style = Text::join(["a".red(), "b".red(), "c".red()], "-".red());
    assert_eq!(same_style, "a-b-c".red());
    assert_eq!(same_style.spans().len(), 1);

    // Works with borrowed `&Text` slices and empty middle items too.
    let items = [Text::raw("x"), Text::new(), Text::raw("z")];
    assert_eq!(Text::join(&items, "|"), Text::raw("x||z"));
}

#[test]
fn extended_colors_round_trip_and_mix_cleanly() {
    // Boundary checks on `ExtendedColor::new`, `ExtendedColor::ALL`, and `ColorValue::from_code`.
    for code in 0..=u8::MAX {
        match code {
            0..=15 => {
                assert_eq!(ExtendedColor::new(code), None);
                let cv = ColorValue::from_code(code).unwrap();
                assert_eq!(cv, ColorValue::Palette(Color::from_code(code).unwrap()));
                assert_eq!(cv.code(), Some(code));
            }
            16..=98 => {
                let ext = ExtendedColor::new(code).unwrap();
                assert_eq!(ext.code(), code);
                assert_eq!(ExtendedColor::ALL[(code - 16) as usize], ext);
                let cv = ColorValue::from_code(code).unwrap();
                assert_eq!(cv, ColorValue::Extended(ext));
                assert_eq!(cv.code(), Some(code));
            }
            99..=u8::MAX => {
                assert_eq!(ExtendedColor::new(code), None);
                assert_eq!(ColorValue::from_code(code), None);
            }
        }
    }
    assert_eq!(ColorValue::Rgb(1, 2, 3).code(), None);

    // Spot-check canonical RGB entries from `modern.ircdocs.horse/formatting#colors-16-98`.
    for (code, rgb) in [
        (16, (0x47, 0x00, 0x00)),
        (21, (0x00, 0x47, 0x2c)),
        (51, (0xb5, 0x00, 0x6b)),
        (52, (0xff, 0x00, 0x00)),
        (58, (0x00, 0xff, 0xff)),
        (87, (0xff, 0x94, 0xd3)),
        (88, (0x00, 0x00, 0x00)),
        (98, (0xff, 0xff, 0xff)),
    ] {
        let ext = ExtendedColor::new(code).unwrap();
        assert_eq!(ext.to_rgb(), rgb);
        assert_eq!(ColorValue::from(ext).to_rgb(), rgb);
    }

    // Every extended color `16..=98` round-trips as fg, bg-only (`\x0399,<code>`), and fg+bg,
    // including before leading digits and `,digits`.
    for ext in ExtendedColor::ALL {
        let fg_only = "5,13".fg(ext);
        assert_eq!(
            fg_only.to_irc_string(),
            format!("\u{03}{:02}5,13\u{0f}", ext.code())
        );
        assert_eq!(Text::parse(&fg_only.to_irc_string()), fg_only);

        let comma_text = ",13".fg(ext);
        assert_eq!(Text::parse(&comma_text.to_irc_string()), comma_text);

        let bg_only = "5".bg(ext);
        assert_eq!(
            bg_only.to_irc_string(),
            format!("\u{03}99,{:02}5\u{0f}", ext.code())
        );
        assert_eq!(Text::parse(&bg_only.to_irc_string()), bg_only);
    }

    // Mixing `Palette` and `Extended` stays in native `\x03` form without promoting to `\x04`.
    let c20 = ExtendedColor::new(20).unwrap();
    let pal_ext = "hi".red().bg(c20);
    assert_eq!(pal_ext.to_irc_string(), "\u{03}04,20hi\u{0f}");
    assert_eq!(Text::parse(&pal_ext.to_irc_string()), pal_ext);

    let ext_pal = "hi".fg(c20).on_white();
    assert_eq!(ext_pal.to_irc_string(), "\u{03}20,00hi\u{0f}");
    assert_eq!(Text::parse(&ext_pal.to_irc_string()), ext_pal);

    // Mixing `Extended` with `Rgb` promotes the extended side via `to_rgb()` to `\x04` hex.
    let c16 = ExtendedColor::new(16).unwrap();
    let ext_rgb = "x".fg(c16).on_rgb(1, 2, 3);
    assert_eq!(ext_rgb.to_irc_string(), "\u{04}470000,010203x\u{0f}");
    assert_eq!(
        Text::parse(&ext_rgb.to_irc_string()),
        "x".rgb(0x47, 0x00, 0x00).on_rgb(1, 2, 3)
    );

    // Relaying `\x0316`..`\x0398` preserves extended colors while `\x0399` resets the channel.
    let relayed = Text::parse("\u{03}16,88dark\u{03}98light\u{03}99,99plain");
    assert_eq!(
        relayed.spans()[0].style.fg,
        Some(ColorValue::Extended(ExtendedColor::new(16).unwrap()))
    );
    assert_eq!(
        relayed.spans()[0].style.bg,
        Some(ColorValue::Extended(ExtendedColor::new(88).unwrap()))
    );
    assert_eq!(
        relayed.spans()[1].style.fg,
        Some(ColorValue::Extended(ExtendedColor::new(98).unwrap()))
    );
    assert_eq!(
        relayed.spans()[1].style.bg,
        Some(ColorValue::Extended(ExtendedColor::new(88).unwrap()))
    );
    assert_eq!(relayed.spans()[2].style.fg, None);
    assert_eq!(relayed.spans()[2].style.bg, None);
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
        use irc_colors::{Span, Style};
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
