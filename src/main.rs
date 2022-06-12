mod command;
mod dump_png;
mod proc;

use std::ffi::OsString;

use anyhow::{bail, Result};
use clap::{arg, command};
use proc::Proc;
use tokio::io::AsyncBufReadExt;

use crate::command::Command;
use crate::dump_png::dump_png;

#[tokio::main]
async fn main() -> () {
  env_logger::builder()
    .format_timestamp(None)
    .filter_level(log::LevelFilter::Info)
    .init();

  match run_cli().await {
    Ok(()) => (),
    Err(err) => log::error!("{}", err.to_string()),
  };

  std::process::exit(0);
}

async fn run_cli() -> anyhow::Result<()> {
  let matches = command!()
    .arg(arg!(-c --config [PATH] "Config path [default: mprocs.yaml]"))
    .arg(arg!(-s --server [PATH] "Remote control server address. Example: 127.0.0.1:4050."))
    .arg(arg!(--ctl [JSON] "Send json encoded command to running mprocs"))
    .arg(arg!(<script> "Command to run"))
    .trailing_var_arg(true)
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
            Command::Kill => state.proc()?.killer.kill()?,
            Command::Wait => state.proc()?.wait().await?,
            Command::Sleep(delay) => tokio::time::sleep(delay).await,
            Command::Print(msg) => println!("PRINT: {}", msg),
            Command::DumpPng(path) => {
              let proc = state.proc()?;
              let vt = proc.vt.lock().await;
              let screen = vt.screen();
              dump_png(screen, path.as_str())?;
            }
          }
        }
      }
      Err(err) => return Err(anyhow::anyhow!(err)),
    }
  }
  Ok(())
}
