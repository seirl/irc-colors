use clap::Parser;
use futures_util::StreamExt;
use irc::client::prelude::*;
use irc_colors::{Color, ColorValue, Colorize, Span, Style, Text};

const ALERT_THEME: Style = Style::new().red().underline();

#[derive(Parser)]
#[command(about = "Demo IRC bot showcasing irc-colors")]
struct Args {
    #[arg(long, default_value = "irc.libera.chat")]
    server: String,

    #[arg(long)]
    port: Option<u16>,

    #[arg(long)]
    no_tls: bool,

    #[arg(long, default_value = "irccolors-bot")]
    nick: String,

    #[arg(long, required = true, value_delimiter = ',', num_args = 1..)]
    channels: Vec<String>,
}

fn send_lines(client: &Client, target: &str, lines: &[Text]) -> irc::error::Result<()> {
    // Leave headroom inside the 512-byte IRC frame for `:nick!user@host PRIVMSG <target> :\r\n`.
    let max_payload = 385usize.saturating_sub(target.len());
    for msg in lines {
        for chunk in msg.wrap(max_payload) {
            client.send_privmsg(target, &chunk)?;
        }
    }
    Ok(())
}

fn palette_line() -> Text {
    let chips = Color::ALL.iter().map(|&c| {
        let label = format!(" {:02}:{:?} ", c.code(), c);
        match c {
            Color::White
            | Color::Yellow
            | Color::LightGreen
            | Color::LightCyan
            | Color::LightGrey => label.black().bg(c),
            _ => label.white().bg(c),
        }
    });
    "Palette: ".bold() + Text::join(chips, " ")
}

fn styles_lines() -> Vec<Text> {
    let attrs = Text::join(
        [
            "bold".bold(),
            "italic".italic(),
            "underline".underline(),
            "strikethrough".strikethrough(),
            "monospace".monospace(),
            "reverse".reverse(),
            " DANGER ".bold().white().on_red(),
        ],
        " · ".grey(),
    );

    let toggles = "red ".red()
        + "+bold ".red().bold()
        + "+underline ".red().bold().underline()
        + "back to red".red();
    let toggle_info = format!(
        " ({} wire bytes — same-color flags toggle in place without \\x0f)",
        toggles.wire_len()
    )
    .grey();

    vec![
        "Styles: ".bold() + attrs,
        "Toggles: ".bold() + toggles + toggle_info,
    ]
}

fn hsv_rainbow(step: usize, total: usize) -> (u8, u8, u8) {
    let h = (step as f32 / total.max(1) as f32) * 6.0;
    let x = ((1.0 - (h % 2.0 - 1.0).abs()) * 255.0) as u8;
    match h as u8 {
        0 => (255, x, 60),
        1 => (x, 255, 60),
        2 => (60, 255, x),
        3 => (60, x, 255),
        4 => (x, 60, 255),
        _ => (255, 60, x),
    }
}

fn rgb_line(input: &str) -> Text {
    let clean = Text::parse(input).plain();
    let sample = if clean.trim().is_empty() {
        "24-bit \\x04 RGB gradient — grapheme-safe: 👨‍👩‍👦 café ✨"
    } else {
        clean.as_str()
    };

    // Split non-space runs with `wrap(1)` so multi-codepoint grapheme clusters
    // (like ZWJ family emoji or combining accents) stay intact.
    let mut units: Vec<(String, bool)> = Vec::new();
    let mut idx = 0;
    while idx < sample.len() {
        let rest = &sample[idx..];
        if rest.starts_with(' ') {
            let spaces = rest.bytes().take_while(|&b| b == b' ').count();
            units.push((rest[..spaces].to_string(), false));
            idx += spaces;
        } else {
            let word_len = rest.find(' ').unwrap_or(rest.len());
            for g in Text::raw(&rest[..word_len]).wrap(1) {
                units.push((g.plain(), true));
            }
            idx += word_len;
        }
    }

    let colored_total = units.iter().filter(|(_, colored)| *colored).count();
    let mut out = "RGB: ".bold();
    let mut step = 0usize;
    for (unit, colored) in units {
        if colored {
            let (r, g, b) = hsv_rainbow(step, colored_total);
            out += unit.rgb(r, g, b).bold();
            step += 1;
        } else {
            out += unit;
        }
    }
    out
}

fn theme_line(input: &str) -> Text {
    let highlight = Text::parse(input).plain();
    let inner = if highlight.trim().is_empty() {
        "SPARTA".bold().light_cyan()
    } else {
        highlight.bold().light_cyan()
    };
    let themed = ("This is "
        + inner
        + " — unstyled text inherits theme, child keeps fg & unions underline!")
        .with_fallback_style(ALERT_THEME);
    "Theme: ".bold() + themed
}

fn wrap_lines(arg: &str) -> Vec<Text> {
    let trimmed = arg.trim_start();
    let (max_bytes, rest) = match trimmed.split_once(' ') {
        Some((first, r)) if first.chars().all(|c| c.is_ascii_digit()) => {
            let n = first.parse::<usize>().unwrap_or(65).clamp(16, 350);
            (n, r.trim_start())
        }
        _ if !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit()) => {
            (trimmed.parse::<usize>().unwrap_or(65).clamp(16, 350), "")
        }
        _ => (65, trimmed),
    };

    let source = if rest.is_empty() {
        " Wrapping ".bold().white().on_blue()
            + " preserves ".italic().yellow()
            + "palette, ".green()
            + "24-bit RGB, ".rgb(255, 105, 180).bold()
            + "and ZWJ emoji 👨‍👩‍👦👨‍👩‍👦 across wire-byte boundaries without losing state!"
                .light_cyan()
                .underline()
    } else {
        Text::parse(rest)
    };

    let chunks = source.wrap(max_bytes);
    let mut out = Vec::with_capacity(chunks.len() + 1);
    out.push(
        format!(
            "wrap({max_bytes}): {} total wire bytes -> {} chunks:",
            source.wire_len(),
            chunks.len()
        )
        .grey(),
    );
    let total = chunks.len();
    for (i, chunk) in chunks.into_iter().enumerate() {
        let meta = format!(
            "[{}/{} | {:>2}/{}B | {}g] ",
            i + 1,
            total,
            chunk.wire_len(),
            max_bytes,
            chunk.visible_len()
        )
        .grey();
        out.push(meta + chunk);
    }
    out
}

fn format_color(c: ColorValue) -> String {
    match c {
        ColorValue::Palette(p) => format!("{p:?}({:02})", p.code()),
        ColorValue::Rgb(r, g, b) => format!("#{r:02X}{g:02X}{b:02X}"),
    }
}

fn describe_span(span: &Span) -> Text {
    let s = span.style;
    let mut tags = Vec::new();
    if let Some(fg) = s.fg {
        tags.push(format!("fg={}", format_color(fg)));
    }
    if let Some(bg) = s.bg {
        tags.push(format!("bg={}", format_color(bg)));
    }
    for (on, name) in [
        (s.bold, "B"),
        (s.italic, "I"),
        (s.underline, "U"),
        (s.strikethrough, "S"),
        (s.monospace, "M"),
        (s.reverse, "R"),
    ] {
        if on {
            tags.push(name.to_string());
        }
    }
    let style_desc = if tags.is_empty() {
        "plain".to_string()
    } else {
        tags.join(",")
    };
    "[".grey()
        + style_desc.yellow()
        + " ".grey()
        + format!("{:?}", span.text).light_cyan()
        + "]".grey()
}

fn inspect_lines(raw_arg: &str) -> Vec<Text> {
    if raw_arg.is_empty() {
        return vec![
            "Usage: ".bold()
                + "!inspect <send colored/bold IRC text here>".yellow()
                + " — parses wire codes via Text::parse".grey(),
        ];
    }
    let parsed = Text::parse(raw_arg);
    let summary = format!(
        "Parsed {} span(s), {} graphemes, {} canonical wire bytes (input was {}B): ",
        parsed.spans().len(),
        parsed.visible_len(),
        parsed.wire_len(),
        raw_arg.len(),
    )
    .grey();
    let spans_list = Text::join(parsed.spans().iter().map(describe_span), " ");
    vec![summary + spans_list, "Round-trip: ".grey() + parsed]
}

fn strip_line(raw_arg: &str) -> Text {
    if raw_arg.is_empty() {
        return "Usage: ".bold()
            + "!strip <styled text>".yellow()
            + " — strips wire formatting via Text::parse(..).reset()".grey();
    }
    "Stripped (parse + reset): ".bold()
        + Text::parse(raw_arg)
            .reset()
            .with_fallback_style(Style::new().light_green())
        + " · Text::raw: ".bold()
        + Text::raw(raw_arg).with_fallback_style(Style::new().yellow())
}

fn help_line() -> Text {
    let cmds = [
        "!demo",
        "!palette",
        "!styles",
        "!rgb [text]",
        "!theme [text]",
        "!wrap [bytes] [text]",
        "!inspect <styled>",
        "!strip <styled>",
    ];
    "Commands: ".bold() + Text::join(cmds.iter().map(|c| c.bold().light_cyan()), " · ".grey())
}

fn handle_command(trailing: &str) -> Option<Vec<Text>> {
    let trimmed = trailing.trim_start();
    if !trimmed.starts_with('!') {
        return None;
    }
    let (cmd, arg) = match trimmed.split_once(' ') {
        Some((c, a)) => (c, a),
        None => (trimmed, ""),
    };

    match cmd {
        "!help" => Some(vec![help_line()]),
        "!palette" => Some(vec![palette_line()]),
        "!styles" => Some(styles_lines()),
        "!rgb" | "!rainbow" => Some(vec![rgb_line(arg)]),
        "!theme" => Some(vec![theme_line(arg)]),
        "!wrap" => Some(wrap_lines(arg)),
        "!inspect" => Some(inspect_lines(arg)),
        "!strip" | "!raw" => Some(vec![strip_line(arg)]),
        "!demo" => {
            let mut lines = Vec::new();
            lines.push(palette_line());
            lines.extend(styles_lines());
            lines.push(rgb_line(""));
            lines.push(theme_line(""));
            lines.extend(wrap_lines("65"));
            Some(lines)
        }
        _ => None,
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = Config {
        nickname: Some(args.nick.clone()),
        alt_nicks: vec![format!("{}_", args.nick), format!("{}__", args.nick)],
        server: Some(args.server),
        port: Some(args.port.unwrap_or(if args.no_tls { 6667 } else { 6697 })),
        use_tls: Some(!args.no_tls),
        channels: args.channels,
        ..Config::default()
    };

    let mut client = Client::from_config(config).await?;
    client.identify()?;

    let mut stream = client.stream()?;
    let mut greeting = Text::new();

    while let Some(message) = stream.next().await.transpose()? {
        print!("{message}");
        match &message.command {
            Command::JOIN(chan, _, _)
                if message.source_nickname() == Some(client.current_nickname()) =>
            {
                greeting.clear();
                greeting += "irc-colors".bold().light_cyan();
                greeting += " demo bot online — try ";
                greeting += "!demo".bold().yellow();
                greeting += " or ";
                greeting += "!help".bold().light_green();
                send_lines(&client, chan, std::slice::from_ref(&greeting))?;
            }
            Command::PRIVMSG(_, text) => {
                if let (Some(target), Some(lines)) =
                    (message.response_target(), handle_command(text))
                {
                    send_lines(&client, target, &lines)?;
                }
            }
            _ => {}
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_and_commands_produce_valid_wrapped_lines() {
        for cmd in [
            "!help",
            "!demo",
            "!palette",
            "!styles",
            "!rgb \x0304hello 👨‍👩‍👦!",
            "!theme \x0312CUSTOM",
            "!wrap 55 \x02bold and \x0304red text that wraps cleanly",
            "!inspect \x02\x0304,01styled\x0f plain",
            "!strip \x02\x0304spoofed",
        ] {
            let lines = handle_command(cmd).expect(cmd);
            assert!(!lines.is_empty(), "{cmd} returned no lines");
            for line in lines {
                for chunk in line.wrap(380) {
                    assert!(chunk.wire_len() <= 380);
                    assert!(!chunk.to_irc_string().contains(['\r', '\n', '\0']));
                }
            }
        }
        assert_eq!(rgb_line("\x0304hi").plain(), "RGB: hi");
        assert!(theme_line("\x0312SPARTA").plain().contains("This is SPARTA"));
    }
}
