use anyhow::Result;

pub fn dump_txt(screen: &vt100::Screen, path: &str) -> Result<()> {
  std::fs::write(path, screen.contents())?;

  Ok(())
}
