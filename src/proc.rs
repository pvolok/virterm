use std::{collections::HashMap, io::Write, sync::Arc, time::Duration};

use anyhow::{bail, Result};
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use mlua::{Lua, LuaSerdeExt, UserData, Value};
use portable_pty::{ChildKiller, MasterPty, PtySize};
use serde::Deserialize;

use crate::{
  dump_png::dump_png,
  dump_txt::dump_txt,
  encode_term::{encode_key, KeyCodeEncodeModes},
  key::Key,
  lua_utils::to_lua_err,
  mouse::MouseAction,
};

pub struct Proc {
  pub pid: i32,
  pub master: Box<dyn MasterPty + Send>,
  pub killer: Box<dyn ChildKiller + Send + Sync>,
  pub wait:
    Option<tokio::sync::oneshot::Receiver<Result<portable_pty::ExitStatus>>>,

  pub vt: Arc<std::sync::Mutex<vt100::Parser>>,
}

#[derive(Debug, Deserialize)]
pub struct ProcConfig {
  #[serde(default = "default_width")]
  pub width: u16,
  #[serde(default = "default_height")]
  pub height: u16,
  pub cwd: Option<String>,
  pub env: Option<HashMap<String, Option<String>>>,
  pub clear_env: Option<bool>,
}

impl Default for ProcConfig {
  fn default() -> Self {
    Self {
      width: default_width(),
      height: default_height(),
      cwd: None,
      env: None,
      clear_env: None,
    }
  }
}

fn default_width() -> u16 {
  80
}
fn default_height() -> u16 {
  30
}

#[derive(Debug, Deserialize)]
pub struct ResizeConfig {
  pub width: u16,
  pub height: u16,
}

#[derive(Debug, Deserialize)]
pub struct ClickParams {
  x: u16,
  y: u16,
  #[serde(default = "default_click_button")]
  button: ClickButton,
}

#[derive(Debug, Deserialize)]
pub enum ClickButton {
  #[serde(rename = "left")]
  Left,
  #[serde(rename = "right")]
  Right,
  #[serde(rename = "middle")]
  Middle,
}

fn default_click_button() -> ClickButton {
  ClickButton::Left
}

#[derive(Debug, Deserialize)]
pub struct ScrollParams {
  x: u16,
  y: u16,
  dir: ScrollDir,
}

#[derive(Debug, Deserialize)]
pub enum ScrollDir {
  #[serde(rename = "up")]
  Up,
  #[serde(rename = "down")]
  Down,
}

impl Proc {
  pub fn shell(shell: &str, cfg: &ProcConfig) -> Result<Self> {
    Self::start(portable_pty::CommandBuilder::from_shell(shell), cfg)
  }

  pub fn start(
    mut cmd: portable_pty::CommandBuilder,
    cfg: &ProcConfig,
  ) -> Result<Self> {
    if let Some(cwd) = &cfg.cwd {
      cmd.cwd(cwd);
    } else {
      cmd.cwd(std::env::current_dir()?.as_os_str());
    }
    match cfg.clear_env {
      Some(true) => cmd.env_clear(),
      _ => (),
    }
    if let Some(env) = &cfg.env {
      for (k, v) in env {
        if let Some(v) = v {
          cmd.env(k, v);
        } else {
          cmd.env_remove(k);
        }
      }
    }

    let pair =
      portable_pty::native_pty_system().openpty(portable_pty::PtySize {
        rows: cfg.height,
        cols: cfg.width,
        pixel_width: 0,
        pixel_height: 0,
      })?;
    let mut child = pair.slave.spawn_command(cmd)?;
    let pid = child.process_id().map(|i| i as i32).unwrap_or(-1);
    let killer = child.clone_killer();

    let (wait_send, wait) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
      let result = child.wait().map_err(anyhow::Error::from);
      let _r = wait_send.send(result);
    });

    let vt = vt100::Parser::new(cfg.height, cfg.width, 100);
    let vt = Arc::new(std::sync::Mutex::new(vt));

    let mut reader = pair.master.try_clone_reader().unwrap();

    {
      let vt = vt.clone();
      tokio::task::spawn_blocking(move || {
        let mut buf = [0; 4 * 1024];
        loop {
          match reader.read(&mut buf[..]) {
            Ok(count) => {
              if count > 0 {
                vt.clone().lock().unwrap().process(&buf[..count]);
              } else {
                std::thread::sleep(std::time::Duration::from_millis(10));
              }
            }
            _ => break,
          }
        }
      });
    }

    let proc = Proc {
      pid,
      master: pair.master,
      killer,
      wait: Some(wait),

      vt,
    };

    Ok(proc)
  }

  pub fn send_key(&mut self, key: &Key) {
    let application_cursor_keys =
      self.lock_vt().unwrap().screen().application_cursor();
    let encoder = encode_key(
      key,
      KeyCodeEncodeModes {
        enable_csi_u_key_encoding: false,
        application_cursor_keys,
        newline_mode: false,
      },
    );
    match encoder {
      Ok(encoder) => {
        self.master.write_all(encoder.as_bytes()).unwrap();
      }
      Err(_) => {
        log::warn!("Failed to encode key: {}", key.to_string());
      }
    }
  }

  pub fn send_mouse(&mut self, mouse: &MouseAction) -> Result<()> {
    self.master.write_all(mouse.encode()?.as_bytes())?;
    Ok(())
  }

  #[cfg(windows)]
  pub fn send_signal(&mut self, _sig: libc::c_int) {
    ()
  }

  #[cfg(not(windows))]
  pub fn send_signal(&mut self, sig: libc::c_int) {
    unsafe { libc::kill(self.pid, sig) };
  }

  pub async fn wait(&mut self) -> Result<()> {
    if let Some(wait) = self.wait.take() {
      match wait.await? {
        Ok(status) if status.success() => {
          log::info!("Process returned ok")
        }
        Ok(_) => log::info!("Process returned error"),
        Err(err) => log::info!("wait(): Error: {}", err),
      }
      Ok(())
    } else {
      bail!("Can't wait the process more than once");
    }
  }

  pub async fn resize(&mut self, opts: ResizeConfig) -> Result<()> {
    self.lock_vt()?.set_size(opts.height, opts.width);
    self.master.resize(PtySize {
      cols: opts.width,
      rows: opts.height,
      pixel_width: 0,
      pixel_height: 0,
    })?;
    Ok(())
  }

  fn lock_vt(
    &self,
  ) -> Result<std::sync::MutexGuard<vt100::Parser>, mlua::Error> {
    self
      .vt
      .lock()
      .map_err(|e| mlua::Error::external(e.to_string()))
  }
}

#[derive(Clone)]
pub struct LuaProc(Arc<std::sync::Mutex<Proc>>);

impl LuaProc {
  pub fn new(proc: Proc) -> Self {
    LuaProc(Arc::new(std::sync::Mutex::new(proc)))
  }

  fn lock(&self) -> Result<std::sync::MutexGuard<Proc>, mlua::Error> {
    self
      .0
      .lock()
      .map_err(|e| mlua::Error::external(e.to_string()))
  }
}

impl UserData for LuaProc {
  fn add_fields<'lua, F: mlua::UserDataFields<'lua, Self>>(_fields: &mut F) {}

  fn add_methods<'lua, M: mlua::UserDataMethods<'lua, Self>>(methods: &mut M) {
    // pid()
    methods.add_method("pid", |_, proc, ()| {
      let pid = proc.lock()?.pid;
      Ok(pid)
    });

    // cell()
    #[derive(Deserialize)]
    struct CellOpts {
      x: u16,
      y: u16,
    }
    methods.add_method("cell", |lua, proc, opts: Value| {
      let opts: CellOpts = lua.from_value(opts)?;
      let cell = match proc
        .lock()?
        .lock_vt()?
        .screen()
        .cell(opts.y, opts.x)
        .cloned()
      {
        Some(cell) => cell,
        None => return Ok(Value::Nil),
      };
      let info = lua.create_table()?;
      info.set("content", cell.contents())?;
      info.set("fg", from_vt_color(lua, cell.fgcolor())?)?;
      info.set("bg", from_vt_color(lua, cell.bgcolor())?)?;
      info.set("bold", cell.bold())?;
      info.set("italic", cell.italic())?;
      info.set("underline", cell.underline())?;
      info.set("inverse", cell.inverse())?;
      info.set("wide", cell.is_wide())?;
      Ok(Value::Table(info))
    });

    // contents()
    methods.add_method("contents", |_, proc, ()| {
      let contents = proc.lock()?.lock_vt()?.screen().contents();
      Ok(contents)
    });

    // contents_hex()
    methods.add_method("contents_hex", |_, proc, ()| {
      let contents = proc.lock()?.lock_vt()?.screen().contents();
      let mut buf = String::new();
      for ch in contents.chars() {
        if ch == '\n' || ch == '\r' {
          buf.push(ch);
        } else {
          let num = ch as u32;
          let s = if num <= 255 {
            format!(" {:02x}", num)
          } else {
            format!(" {:x}", num)
          };
          buf.push_str(s.as_str());
        }
      }
      Ok(buf)
    });

    // send_str
    methods.add_method("send_str", |_, proc, str: String| {
      log::info!("send_str(): {}", str);
      let mut proc = proc.lock()?;
      proc.master.write_all(str.as_bytes()).map_err(to_lua_err)?;
      Ok(())
    });

    // send_key()
    methods.add_async_method("send_key", async move |_, proc, key: String| {
      log::info!("send_key(): {}", key);
      let key = Key::parse(key.as_str()).map_err(to_lua_err)?;
      let mut proc = proc.lock()?;
      proc.send_key(&key);
      Ok(())
    });

    // click()
    methods.add_method("click", |lua, proc, opts: Value| {
      let opts: ClickParams = lua.from_value(opts).map_err(to_lua_err)?;
      let btn = match opts.button {
        ClickButton::Left => MouseButton::Left,
        ClickButton::Right => MouseButton::Right,
        ClickButton::Middle => MouseButton::Middle,
      };
      let action = MouseAction(MouseEvent {
        kind: MouseEventKind::Down(btn),
        row: opts.y,
        column: opts.x,
        modifiers: KeyModifiers::NONE,
      });
      proc.lock()?.send_mouse(&action).map_err(to_lua_err)?;
      Ok(())
    });

    // scroll()
    methods.add_method("scroll", |lua, proc, opts: Value| {
      let opts: ScrollParams = lua.from_value(opts).map_err(to_lua_err)?;
      let kind = match opts.dir {
        ScrollDir::Up => MouseEventKind::ScrollUp,
        ScrollDir::Down => MouseEventKind::ScrollDown,
      };
      let action = MouseAction(MouseEvent {
        kind,
        row: opts.y,
        column: opts.x,
        modifiers: KeyModifiers::NONE,
      });
      proc.lock()?.send_mouse(&action).map_err(to_lua_err)?;
      Ok(())
    });

    // send_signal
    methods.add_method("send_signal", |_, proc, sig: Value| {
      let (sig, str) = match sig {
        Value::Integer(sig) => (sig as i32, sig.to_string()),
        Value::String(sig) => {
          let str = sig.to_str()?;
          let sig = signal_from_string(str).map_err(to_lua_err)?;
          (sig, str.to_string())
        }
        _ => {
          return Err(mlua::Error::external(
            "proc.kill() expects a string or an integer",
          ))
        }
      };
      log::info!("send_signal(): {:?}", str);
      proc.lock()?.send_signal(sig);
      Ok(())
    });

    // kill()
    methods.add_method("kill", |_, proc, ()| {
      log::info!("kill()");
      proc.lock()?.killer.kill().map_err(to_lua_err)
    });

    // resize
    methods.add_async_method("resize", async move |lua, proc, opts: Value| {
      let opts: ResizeConfig = lua.from_value(opts).map_err(to_lua_err)?;
      proc.lock()?.resize(opts).await.map_err(to_lua_err)
    });

    // wait()
    methods.add_async_method("wait", async move |_, proc, ()| {
      log::info!("wait()");
      proc.lock()?.wait().await.map_err(to_lua_err)
    });

    // wait_text(text, {timeout})
    methods.add_async_method(
      "wait_text",
      async move |_, proc, (text, opts): (String, Option<mlua::Table>)| {
        log::info!("wait_text(): {:?} {:?}", text, opts);
        let timeout = opts
          .map(|opts| opts.get("timeout"))
          .transpose()?
          .unwrap_or(1500);

        let proc = &proc.lock()?;
        let timeout = Duration::from_millis(timeout);
        tokio::time::timeout(timeout, async {
          loop {
            if proc
              .lock_vt()
              .unwrap()
              .screen()
              .contents()
              .contains(text.as_str())
            {
              break ();
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
          }
        })
        .await
        .map_err(to_lua_err)?;
        Ok(())
      },
    );

    // dump_txt(path)
    methods.add_async_method("dump_txt", async move |_, proc, path: String| {
      log::info!("dump_txt()");
      let proc = proc.lock()?;
      let vt = proc.lock_vt()?;
      dump_txt(vt.screen(), path.as_str()).map_err(to_lua_err)?;
      Ok(())
    });

    // dump_png(path)
    methods.add_async_method("dump_png", async move |_, proc, path: String| {
      log::info!("dump_png()");
      let proc = proc.lock()?;
      let vt = proc.lock_vt()?;
      dump_png(vt.screen(), path.as_str()).map_err(to_lua_err)?;
      Ok(())
    });
  }
}

fn signal_from_string(sig: &str) -> Result<libc::c_int> {
  let sig = match sig {
    "SIGHUP" => 1,
    "SIGINT" => 2,
    "SIGQUIT" => 3,
    "SIGILL" => 4,
    "SIGABRT" => 6,
    "SIGEMT" => 7,
    "SIGFPE" => 8,
    "SIGKILL" => 9,
    "SIGSEGV" => 11,
    "SIGPIPE" => 13,
    "SIGALRM" => 14,
    "SIGTERM" => 15,
    _ => bail!("Unknown signal: {}", sig),
  };
  Ok(sig)
}

fn from_vt_color<'lua>(
  lua: &'lua Lua,
  color: vt100::Color,
) -> mlua::Result<Value<'lua>> {
  let ret = match color {
    vt100::Color::Default => Value::Nil,
    vt100::Color::Idx(idx) => Value::Number(idx as f64),
    vt100::Color::Rgb(r, g, b) => {
      let s = format!("#{:02x}{:02x}{:02x}", r, g, b);
      Value::String(lua.create_string(s.as_str())?)
    }
  };
  Ok(ret)
}
