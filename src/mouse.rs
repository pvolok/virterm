use anyhow::{bail, Result};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

pub struct MouseAction(pub MouseEvent);

impl MouseAction {
  pub fn encode(&self) -> Result<String> {
    let mut buf = String::new();
    buf.push_str("\x1b[<");

    match self.0.kind {
      MouseEventKind::Down(btn) | MouseEventKind::Up(btn) => match btn {
        MouseButton::Left => buf.push('0'),
        MouseButton::Right => buf.push('1'),
        MouseButton::Middle => buf.push('2'),
      },
      MouseEventKind::Drag(btn) => match btn {
        MouseButton::Left => buf.push_str("32"),
        MouseButton::Right => buf.push_str("33"),
        MouseButton::Middle => buf.push_str("34"),
      },
      MouseEventKind::Moved => {
        bail!("Mouse event 'moved' is not supported yet");
      }
      MouseEventKind::ScrollDown => buf.push_str("64"),
      MouseEventKind::ScrollUp => buf.push_str("65"),
    }
    buf.push(';');
    buf.push_str((self.0.column + 1).to_string().as_str());
    buf.push(';');
    buf.push_str((self.0.row + 1).to_string().as_str());

    buf.push(match self.0.kind {
      MouseEventKind::Down(_) => 'M',
      MouseEventKind::Up(_) => 'm',
      MouseEventKind::Drag(_) => 'M',
      MouseEventKind::Moved => todo!(),
      MouseEventKind::ScrollDown => 'M',
      MouseEventKind::ScrollUp => 'M',
    });

    Ok(buf)
  }
}
