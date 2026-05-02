//! Set the OS "hidden" attribute on a file. Best-effort — failures are
//! silently ignored since hiding is a cosmetic preference.

use std::path::Path;

pub fn set_hidden(path: &Path) {
    let _ = set_hidden_impl(path);
}

pub fn set_visible(path: &Path) {
    let _ = set_visible_impl(path);
}

#[cfg(target_os = "macos")]
fn set_hidden_impl(path: &Path) -> std::io::Result<()> {
    use std::process::Command;
    let status = Command::new("chflags").arg("hidden").arg(path).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("chflags failed"))
    }
}

#[cfg(target_os = "windows")]
fn set_hidden_impl(path: &Path) -> std::io::Result<()> {
    use std::process::Command;
    let status = Command::new("attrib").arg("+h").arg(path).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("attrib +h failed"))
    }
}

#[allow(clippy::unnecessary_wraps)]
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn set_hidden_impl(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[allow(clippy::unnecessary_wraps)]
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn set_visible_impl(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn set_visible_impl(path: &Path) -> std::io::Result<()> {
    use std::process::Command;
    let status = Command::new("chflags").arg("nohidden").arg(path).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("chflags nohidden failed"))
    }
}

#[cfg(target_os = "windows")]
fn set_visible_impl(path: &Path) -> std::io::Result<()> {
    use std::process::Command;
    let status = Command::new("attrib").arg("-h").arg(path).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("attrib -h failed"))
    }
}
