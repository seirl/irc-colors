# irc-colors

Write IRC-formatted colored text in Rust, the fun way.

Implements the formatting control codes and color table from [IRC Formatting](https://modern.ircdocs.horse/formatting), with stateless span composition, round-trip parsing, and byte-accurate word/grapheme wrapping for IRC wire limits (`#![no_std]` + `alloc` compatible).

## Usage

```rust
use irc_colors::Colorize;

let msg = "Hello".bold().red() + " world".underline().blue();

// background colors, `colored`-crate style
let warn = "DANGER".bold().white().on_red();

// italics, strikethrough, monospace, and 24-bit hex color too
let fancy = "old price".strikethrough() + " " + "new price".rgb(255, 105, 180);

// plain strings and `format_args!` mix in for free
let score = 42;
let line = "Level: " + "critical".bold().red() + " (" + format_args!("{score:04}").cyan() + ")!";

// splits into IRC-safe messages by wire byte length, preserving styles across chunks
for chunk in line.wrap(400) {
    println!("{chunk}"); // ready to send as a PRIVMSG
}
```

## `irc_format!` macro (optional)

Enable the `format` feature in `Cargo.toml` to build `Text` values with `std::format!`-style interpolation (`{}`, `{0}`, `{name}`, `name = val`, and static `core::fmt` specifiers like `{:04}`, `{:.2}`, `{:?}`, `{:#x}`; dynamic `$` / `.*` width/precision counts are not supported). Styled `Text`/`Span` values keep their styles, while plain `Display` arguments strip IRC control/CTCP/NUL bytes:

```toml
[dependencies]
irc-colors = { version = "0.1", features = ["format"] }
```

```rust
use irc_colors::{irc_format, Colorize};

let nick = "alice";
let score = 42;
let line = irc_format!("User {} ({nick}) scored {score:04}!", "VIP".bold().yellow());
```

## Features

- **Chainable `Colorize` builder** on `&str`, `String`, `&String`, `fmt::Arguments` (`format_args!`), `Span`, `&Span`, `Text`, and `&Text` (16 standard IRC palette colors, extended palette codes `16..=98`, 24-bit `\x04` RGB colors, bold, italic, underline, strikethrough, monospace, reverse, and fallback themes).
- **Stateless `Span` model**: every span carries its complete resolved `Style`, so concatenating, slicing, or wrapping never loses formatting state across message boundaries.
- **Byte-accurate `Text::wrap(max_bytes)`**: accounts for control/reset code overhead on the wire, breaks cleanly on spaces and `\r\n`/`\n`/`\r`, omits empty chunks, and falls back to Unicode grapheme-cluster splits for overlong words without tearing multi-codepoint emoji or accents.
- **Round-trip `Text::parse`**: parses incoming IRC-formatted strings back into canonical `Text` for relaying, inspecting, or re-wrapping.
- **`#![no_std]` compatible** (requires `alloc`, `#![forbid(unsafe_code)]`).

## License

Licensed under the [MIT License](LICENSE).

