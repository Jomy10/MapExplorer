use std::sync::LockResult;

pub trait ResultExt<T> {
    fn anyhow(self) -> anyhow::Result<T>;
}

impl<T> ResultExt<T> for Result<T, String> {
    fn anyhow(self) -> anyhow::Result<T> {
        self.map_err(|err| anyhow::format_err!(err))
    }
}

impl<T> ResultExt<T> for LockResult<T> {
    fn anyhow(self) -> anyhow::Result<T> {
        self.map_err(|err| anyhow::format_err!("{}", err))
    }
}
