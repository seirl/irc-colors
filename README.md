# irc-color

Write IRC-formatted colored text in Rust, the fun way.

Implements the formatting control codes and color table from [IRC Formatting](https://modern.ircdocs.horse/formatting), with stateless span composition, round-trip parsing, and byte-accurate word/grapheme wrapping for IRC wire limits (`#![no_std]` + `alloc` compatible).

## Usage

```rust
use irc_color::Colorize;

let msg = "Hello".bold().red() + " world".underline().blue();

// background colors, `colored`-crate style
let warn = "DANGER".bold().white().on_red();

// italics, strikethrough, monospace, and 24-bit hex color too
let fancy = "old price".strikethrough() + " " + "new price".rgb(255, 105, 180);

// plain strings mix in for free
let line = "Level: " + "critical".bold().red() + "!";

// splits into IRC-safe messages by wire byte length, preserving styles across chunks
for chunk in line.wrap(400) {
    println!("{chunk}"); // ready to send as a PRIVMSG
}
```

## Features

- **Chainable `Colorize` builder** on `&str`, `String`, `&String`, and `Text` (16 standard IRC palette colors, 24-bit `\x04` RGB colors, bold, italic, underline, strikethrough, monospace, reverse, and fallback themes).
- **Stateless `Span` model**: every span carries its complete resolved `Style`, so concatenating, slicing, or wrapping never loses formatting state across message boundaries.
- **Byte-accurate `Text::wrap(max_bytes)`**: accounts for control/reset code overhead on the wire, breaks cleanly on whitespace and `\r\n`/`\n`/`\r`, and falls back to Unicode grapheme-cluster splits for overlong words without tearing multi-codepoint emoji or accents.
- **Round-trip `Text::parse`**: parses incoming IRC-formatted strings back into canonical `Text`.
- **`#![no_std]` compatible** (requires `alloc`).
