mod command;
mod dump_png;
mod dump_txt;
mod encode_term;
mod key;
mod proc;

use std::ffi::OsString;
use std::time::Duration;

use anyhow::{bail, Result};
use clap::{arg, command};
use proc::Proc;
use tokio::io::AsyncBufReadExt;

use crate::command::Command;
use crate::dump_png::dump_png;
use crate::dump_txt::dump_txt;

#[tokio::main]
async fn main() -> () {
  env_logger::builder()
    .format_timestamp(None)
    .filter_level(log::LevelFilter::Info)
    .init();

  match run_cli().await {
    Ok(()) => (),
    Err(err) => {
      log::error!("{}", err.to_string());
      std::process::exit(1);
    }
  };

  std::process::exit(0);
}

async fn run_cli() -> anyhow::Result<()> {
  let matches = command!()
    .arg(arg!(<script> "Command to run"))
    .get_matches();

  let script = matches.value_of("script").unwrap();

  main_loop(script).await?;

  Ok(())
}

struct State {
  proc: Option<Proc>,
}

impl State {
  fn new() -> Self {
    State { proc: None }
  }

  fn proc(&mut self) -> Result<&mut Proc> {
    if let Some(proc) = &mut self.proc {
      Ok(proc)
    } else {
      bail!("Process has not been started");
    }
  }

  fn start_prog(&mut self, args: Vec<String>) -> Result<()> {
    if let Some(_) = self.proc {
      bail!("Process was already started");
    }
    let args = args
      .into_iter()
      .map(|arg| OsString::from(arg))
      .collect::<Vec<_>>();
    let proc = Proc::start(args)?;
    self.proc = Some(proc);
    Ok(())
  }
}

async fn main_loop(script: &str) -> Result<()> {
  let mut state = State::new();

  let script = tokio::fs::File::open(script).await?;
  let mut reader = tokio::io::BufReader::new(script);
  let mut buf = String::new();
  loop {
    buf.clear();
    match reader.read_line(&mut buf).await {
      Ok(len) => {
        if len == 0 {
          break;
        }
        let cmd = Command::parse(buf.as_str())?;
        if let Some(cmd) = cmd {
          log::info!("CMD: {:?}", cmd);
          match cmd {
            Command::Start(args) => state.start_prog(args)?,
            Command::SendKeys(keys) => {
              let proc = state.proc()?;
              for key in keys {
                proc.send_key(&key).await;
              }
            }
            Command::Kill => state.proc()?.killer.kill()?,
            Command::Wait => state.proc()?.wait().await?,

            Command::WaitText { text, timeout } => {
              let vt = &state.proc()?.vt;
              tokio::time::timeout(timeout, async {
                loop {
                  let vt = vt.lock().await;
                  if vt.screen().contents().contains(text.as_str()) {
                    break ();
                  }
                  tokio::time::sleep(Duration::from_millis(50)).await;
                }
              })
              .await?;
            }

            Command::Sleep(delay) => tokio::time::sleep(delay).await,
            Command::Print(msg) => println!("PRINT: {}", msg),
            Command::DumpPng(path) => {
              let proc = state.proc()?;
              let vt = proc.vt.lock().await;
              let screen = vt.screen();
              dump_png(screen, path.as_str())?;
            }
            Command::DumpTxt(path) => {
              let proc = state.proc()?;
              let vt = proc.vt.lock().await;
              let screen = vt.screen();
              dump_txt(screen, path.as_str())?;
            }
          }
        }
      }
      Err(err) => return Err(anyhow::anyhow!(err)),
    }
  }
  Ok(())
}
