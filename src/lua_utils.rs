pub fn to_lua_err<T>(err: T) -> mlua::Error
where
  T: Into<Box<dyn std::error::Error + Send + Sync>>,
{
  mlua::Error::external(err)
}
