use std::time::Duration;

use anyhow::{bail, Result};

use crate::key::Key;

#[derive(Debug)]
pub enum Command {
  Start(Vec<String>),
  SendKeys(Vec<Key>),
  Kill,
  Wait,

  WaitText { text: String, timeout: Duration },

  Sleep(Duration),
  Print(String),
  DumpPng(String),
  DumpTxt(String),
}

impl Command {
  pub fn parse(text: &str) -> Result<Option<Self>> {
    let mut parser = CommandParser::new(text);
    parser.parse()
  }
}

struct CommandParser<'a> {
  text: &'a [u8],
  pos: usize,
}

impl<'inst> CommandParser<'inst> {
  pub fn new<'a>(text: &'a str) -> CommandParser<'a> {
    CommandParser::<'a> {
      text: text.as_bytes(),
      pos: 0,
    }
  }

  pub fn parse(&mut self) -> Result<Option<Command>> {
    match self.next_token()? {
      Token::Ident(ident) => match ident.as_str() {
        "start" => {
          let mut args = Vec::new();
          let cmd = loop {
            match self.next_token()? {
              Token::String(arg) => args.push(arg),
              Token::Eof => {
                if args.len() == 0 {
                  bail!("The 'start' command expects at least one argument")
                } else {
                  break Command::Start(args);
                }
              }
              _ => bail!("The 'start' command accepts strings only"),
            }
          };
          Ok(Some(cmd))
        }
        "send_keys" => {
          let mut keys = Vec::new();
          loop {
            match self.next_token()? {
              Token::String(str) => {
                for ch in str.chars() {
                  keys.push(Key::from_char(ch));
                }
              }
              Token::Key(key) => keys.push(key),
              Token::Eof => break,
              _ => {
                bail!("The 'send_keys' command accepts strings and keys only")
              }
            }
          }
          Ok(Some(Command::SendKeys(keys)))
        }
        "kill" => Ok(Some(Command::Kill)),
        "wait" => Ok(Some(Command::Wait)),

        "wait_text" => {
          let mut text = None;
          let mut timeout = Duration::from_secs(1);
          loop {
            match self.next_token()? {
              Token::String(s) => {
                if text != None {
                  bail!("The 'wait_text' command expects only one string");
                }
                text = Some(s);
              }
              Token::Arg(arg) => match arg.as_str() {
                "timeout" => match self.next_token()? {
                  Token::Duration(t) => timeout = t,
                  _ => bail!("The 'timeout' arg expects a duration"),
                },
                _ => bail!("Unexpected argument '{}'", arg),
              },
              Token::Eof => break,
              _ => bail!("The 'wait_text' command expects a string"),
            }
          }
          let text = if let Some(text) = text {
            text
          } else {
            bail!("The 'wait_text' command expects a string")
          };
          Ok(Some(Command::WaitText { text, timeout }))
        }

        "sleep" => {
          let dur = match self.next_token()? {
            Token::Duration(dur) => dur,
            _ => bail!("Expected duration"),
          };
          Ok(Some(Command::Sleep(dur)))
        }
        "print" => {
          let msg = match self.next_token()? {
            Token::String(file) => file,
            _ => bail!("Expected string"),
          };
          Ok(Some(Command::Print(msg)))
        }
        "dump_png" => {
          let file = match self.next_token()? {
            Token::String(file) => file,
            _ => bail!("Expected string"),
          };
          Ok(Some(Command::DumpPng(file)))
        }
        "dump_txt" => {
          let file = match self.next_token()? {
            Token::String(file) => file,
            _ => bail!("Expected string"),
          };
          Ok(Some(Command::DumpTxt(file)))
        }
        cmd => bail!("Unknown command: {}", cmd),
      },
      Token::String(_)
      | Token::Arg(_)
      | Token::Int(_)
      | Token::Duration(_)
      | Token::Key(_) => {
        bail!("Expected command identifier")
      }
      Token::Eof => Ok(None),
    }
  }

  fn next_token(&mut self) -> Result<Token> {
    self.skip_spaces();

    let ch = self.peek_char();
    if ch == '"' {
      let s = self.take_string()?;
      Ok(Token::String(s))
    } else if ch.is_ascii_alphabetic() {
      let s = self.take_ident()?;
      let tok = if self.peek_char() == ':' {
        self.pos += 1;
        Token::Arg(s)
      } else {
        Token::Ident(s)
      };
      Ok(tok)
    } else if ch == '<' {
      let key = self.take_key()?;
      Ok(Token::Key(key))
    } else if ch.is_digit(10) {
      let num = self.take_number()?;
      let tok = if self.peek_char().is_ascii_alphabetic() {
        match self.take_ident()?.as_str() {
          "ms" => Token::Duration(Duration::from_millis(num as u64)),
          "s" => Token::Duration(Duration::from_secs(num as u64)),
          suffix => bail!("Unknown number suffix: {}", suffix),
        }
      } else {
        Token::Int(num)
      };
      Ok(tok)
    } else if ch == '#' {
      Ok(Token::Eof)
    } else if ch == '\0' {
      Ok(Token::Eof)
    } else {
      bail!("Unexpected char: {}", ch)
    }
  }

  fn peek_char(&self) -> char {
    let s = self.text.get(self.pos).unwrap_or(&0);
    *s as char
  }

  fn take_char(&mut self) -> char {
    if let Some(ch) = self.text.get(self.pos) {
      self.pos += 1;
      *ch as char
    } else {
      '\0'
    }
  }

  fn expect_char(&mut self, ch: char) -> Result<()> {
    if self.peek_char() == ch {
      self.pos += 1;
      Ok(())
    } else {
      bail!("Expected char: {} (got {})", ch, self.peek_char());
    }
  }

  fn skip_spaces(&mut self) {
    loop {
      let ch = self.peek_char();
      if !ch.is_whitespace() || ch == '\0' {
        return;
      }
      self.pos += 1;
    }
  }

  fn take_string(&mut self) -> Result<String> {
    let mut buf = String::new();
    self.expect_char('"')?;
    loop {
      match self.take_char() {
        '"' => break,
        '\0' => bail!("Unclosed string literal"),
        '\\' => match self.take_char() {
          '\\' => buf.push('\\'),
          '"' => buf.push('"'),
          'n' => buf.push('\n'),
          't' => buf.push('\t'),
          ch => bail!("Unexpected escape character: \\{}", ch),
        },
        ch => buf.push(ch),
      }
    }
    Ok(buf)
  }

  fn take_key(&mut self) -> Result<Key> {
    let mut buf = String::new();
    loop {
      match self.take_char() {
        '>' => {
          buf.push('>');
          break;
        }
        '\0' => bail!("Unclosed key literal"),
        ch => buf.push(ch),
      }
    }
    let key = Key::parse(buf.as_str())?;
    Ok(key)
  }

  fn take_ident(&mut self) -> Result<String> {
    let mut buf = String::new();
    loop {
      let ch = self.peek_char();
      if ch.is_ascii_alphanumeric() || ch == '_' {
        buf.push(ch);
        self.pos += 1;
      } else {
        break;
      }
    }
    Ok(buf)
  }

  fn take_number(&mut self) -> Result<u32> {
    let mut buf = 0;
    loop {
      let ch = self.peek_char();
      if let Some(digit) = ch.to_digit(10) {
        self.pos += 1;
        buf *= 10;
        buf += digit;
      } else {
        break;
      }
    }
    Ok(buf)
  }
}

enum Token {
  Ident(String),
  String(String),
  Arg(String),
  Int(u32),
  Duration(Duration),
  Key(Key),
  Eof,
}
