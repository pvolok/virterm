use std::{ffi::OsString, io::Write, sync::Arc, time::Duration};

use anyhow::{bail, Result};
use mlua::{LuaSerdeExt, UserData, Value};
use portable_pty::{ChildKiller, MasterPty, PtySize};
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::{
  dump_png::dump_png,
  dump_txt::dump_txt,
  encode_term::{encode_key, KeyCodeEncodeModes},
  key::Key,
  lua_utils::to_lua_err,
};

pub struct Proc {
  pub pid: i32,
  pub master: Box<dyn MasterPty + Send>,
  pub killer: Box<dyn ChildKiller + Send + Sync>,
  pub wait:
    Option<tokio::sync::oneshot::Receiver<Result<portable_pty::ExitStatus>>>,

  pub vt: Arc<Mutex<vt100::Parser>>,
}

#[derive(Debug, Deserialize)]
pub struct ProcConfig {
  #[serde(default = "default_width")]
  pub width: u16,
  #[serde(default = "default_height")]
  pub height: u16,
}

impl Default for ProcConfig {
  fn default() -> Self {
    Self {
      width: default_width(),
      height: default_height(),
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

impl Proc {
  #[cfg(windows)]
  pub fn shell(shell: &str, cfg: &ProcConfig) -> Result<Self> {
    let shell_arg = OsString::new();
    shell_arg.push("\0");
    shell_arg.push(shell);

    Self::start(vec!["cmd.exe".into(), "/c".into(), shell_arg], cfg)
  }

  #[cfg(not(windows))]
  pub fn shell(shell: &str, cfg: &ProcConfig) -> Result<Self> {
    Self::start(vec!["/bin/sh".into(), "-c".into(), shell.into()], cfg)
  }

  pub fn start(args: Vec<OsString>, cfg: &ProcConfig) -> Result<Self> {
    let pair =
      portable_pty::native_pty_system().openpty(portable_pty::PtySize {
        rows: cfg.height,
        cols: cfg.width,
        pixel_width: 0,
        pixel_height: 0,
      })?;
    let mut cmd = portable_pty::CommandBuilder::from_argv(args);
    cmd.cwd(std::env::current_dir()?.as_os_str());
    let mut child = pair.slave.spawn_command(cmd)?;
    let pid = child.process_id().map(|i| i as i32).unwrap_or(-1);
    let killer = child.clone_killer();

    let (wait_send, wait) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
      let result = child.wait().map_err(anyhow::Error::from);
      let _r = wait_send.send(result);
    });

    let vt = vt100::Parser::new(cfg.height, cfg.width, 100);
    let vt = Arc::new(Mutex::new(vt));

    let mut reader = pair.master.try_clone_reader().unwrap();

    {
      let vt = vt.clone();
      tokio::task::spawn_blocking(move || {
        let mut buf = [0; 4 * 1024];
        loop {
          match reader.read(&mut buf[..]) {
            Ok(count) => {
              if count > 0 {
                vt.clone().blocking_lock().process(&buf[..count]);
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

  pub async fn send_key(&mut self, key: &Key) {
    let application_cursor_keys =
      self.vt.lock().await.screen().application_cursor();
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
    self.vt.lock().await.set_size(opts.height, opts.width);
    self.master.resize(PtySize {
      cols: opts.width,
      rows: opts.height,
      pixel_width: 0,
      pixel_height: 0,
    })?;
    Ok(())
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
      proc.send_key(&key).await;
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
          .unwrap_or(1000);

        let vt = &proc.lock()?.vt;
        let timeout = Duration::from_millis(timeout);
        tokio::time::timeout(timeout, async {
          loop {
            if vt.lock().await.screen().contents().contains(text.as_str()) {
              break ();
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
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
      let vt = proc.vt.lock().await;
      dump_txt(vt.screen(), path.as_str()).map_err(to_lua_err)?;
      Ok(())
    });

    // dump_png(path)
    methods.add_async_method("dump_png", async move |_, proc, path: String| {
      log::info!("dump_png()");
      let proc = proc.lock()?;
      let vt = proc.vt.lock().await;
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
