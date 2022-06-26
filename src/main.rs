#![feature(async_closure)]

mod dump_png;
mod dump_txt;
mod encode_term;
mod key;
mod lua_utils;
mod proc;

use std::time::Duration;

use anyhow::Result;
use clap::{arg, command};
use lua_utils::to_lua_err;
use mlua::{Lua, LuaSerdeExt};
use proc::{LuaProc, Proc, ProcConfig};
use tokio::io::AsyncReadExt;

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

  run_lua(script).await?;

  Ok(())
}

async fn run_lua(script: &str) -> Result<()> {
  let lua = Lua::new();

  let vt = lua.create_table()?;

  let start =
    lua.create_function(|lua, (cmd, cfg_val): (String, mlua::Value)| {
      let cfg: Option<ProcConfig> = lua.from_value(cfg_val)?;
      let cfg = cfg.unwrap_or_default();
      let proc = Proc::shell(cmd.as_str(), &cfg).map_err(to_lua_err)?;
      let proc = LuaProc::new(proc);
      Ok(proc)
    })?;
  vt.set("start", start)?;

  let sleep = lua.create_async_function(async move |_, millis: u64| {
    tokio::time::sleep(Duration::from_millis(millis)).await;
    Ok(())
  })?;
  vt.set("sleep", sleep)?;

  lua.globals().set("vt", vt)?;

  let mut script = tokio::fs::File::open(script).await?;
  let mut src = String::new();
  script.read_to_string(&mut src).await?;
  lua.load(src.as_str()).exec_async().await?;

  Ok(())
}
