use std::ffi::OsStr;
use std::process::Command;

#[derive(Debug, Clone)]
enum MapnikConfigError {
    RunError(String)
}

impl std::error::Error for MapnikConfigError {}
impl std::fmt::Display for MapnikConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MapnikConfigError::RunError(msg) => f.write_fmt(format_args!("Couldn't run `mapnik-config`: {}", msg)),
        }
    }
}

fn mapnik_config<I, S>(args: I) -> anyhow::Result<String>
where I: IntoIterator<Item = S>,
      S: AsRef<OsStr>,
{
    let out = Command::new("mapnik-config")
        .args(args)
        .output()?;
    if !out.status.success() {
        return Err(MapnikConfigError::RunError(String::from_utf8(out.stderr).unwrap()).into());
    }

    return String::from_utf8(out.stdout).map_err(|err| err.into()).map(|s| s.trim().to_string());
}

pub fn fonts_dir() -> anyhow::Result<String> {
    return mapnik_config(["--fonts"]);
}

pub fn input_plugins_dir() -> anyhow::Result<String> {
    return mapnik_config(["--input-plugins"]);
}
