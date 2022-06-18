use std::{ffi::OsString, sync::Arc};

use anyhow::{bail, Result};
use portable_pty::{ChildKiller, MasterPty};
use tokio::sync::Mutex;

use crate::{
  encode_term::{encode_key, KeyCodeEncodeModes},
  key::Key,
};

pub struct Proc {
  pub master: Box<dyn MasterPty + Send>,
  pub killer: Box<dyn ChildKiller + Send + Sync>,
  pub wait:
    Option<tokio::sync::oneshot::Receiver<Result<portable_pty::ExitStatus>>>,

  pub vt: Arc<Mutex<vt100::Parser>>,
}

impl Proc {
  pub fn start(args: Vec<OsString>) -> Result<Self> {
    let pair =
      portable_pty::native_pty_system().openpty(portable_pty::PtySize {
        rows: 30,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
      })?;
    let mut cmd = portable_pty::CommandBuilder::from_argv(args);
    cmd.cwd(std::env::current_dir()?.as_os_str());
    let mut child = pair.slave.spawn_command(cmd)?;
    let killer = child.clone_killer();

    let (wait_send, wait) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
      let result = child.wait().map_err(anyhow::Error::from);
      let _r = wait_send.send(result);
    });

    let vt = vt100::Parser::new(30, 80, 100);
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

  pub async fn wait(&mut self) -> Result<()> {
    if let Some(wait) = self.wait.take() {
      match wait.await? {
        Ok(status) if status.success() => {
          log::info!("WAIT: Process returned ok")
        }
        Ok(_) => log::info!("WAIT: Process returned error"),
        Err(err) => log::info!("WAIT: Error: {}", err),
      }
      Ok(())
    } else {
      bail!("Can't wait the process more than once");
    }
  }
}
