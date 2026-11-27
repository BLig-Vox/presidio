use std::{env, fs, io, path::Path};

pub fn install() -> io::Result<()> {
    let executable = env::current_exe()?;
    let launch = executable
        .ancestors()
        .skip(1)
        .find(|root| root.join("runtime/linux-x86_64/arch-xfce.sha256").is_file())
        .map(|root| root.join("scripts/arch-oci"))
        .filter(|launcher| launcher.is_file());
    let exec = match launch {
        Some(launcher) => format!("{} --launch", exec_path(&launcher)?),
        None => exec_path(&executable)?,
    };

    let data = env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| env::var_os("HOME").map(|home| Path::new(&home).join(".local/share")))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no user data directory"))?;
    let applications = data.join("applications");
    let icons = data.join("icons/hicolor/256x256/apps");
    fs::create_dir_all(&applications)?;
    fs::create_dir_all(&icons)?;
    fs::write(
        icons.join("presidio.png"),
        include_bytes!("../assets/presidio.png"),
    )?;

    let entry = applications.join("presidio.desktop");
    fs::write(
        &entry,
        format!(
            "[Desktop Entry]\nType=Application\nName=Presidio\nComment=A 3D pocket workspace\nExec={exec}\nIcon=presidio\nTerminal=false\nStartupWMClass=presidio\nCategories=Utility;\n"
        ),
    )?;
    println!("Installed {}", entry.display());
    Ok(())
}

fn exec_path(path: &Path) -> io::Result<String> {
    let path = path
        .to_str()
        .filter(|path| !path.chars().any(char::is_control))
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "unsupported executable path")
        })?;
    let mut quoted = String::from("\"");
    for character in path.chars() {
        match character {
            '\\' => quoted.push_str(r"\\\\"),
            '"' | '$' | '`' => {
                quoted.push_str(r"\\");
                quoted.push(character);
            }
            '%' => quoted.push_str("%%"),
            _ => quoted.push(character),
        }
    }
    quoted.push('"');
    Ok(quoted)
}
