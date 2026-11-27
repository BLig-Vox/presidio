use std::{env, fs, io, path::Path};

use std::{os::unix::process::CommandExt, path::PathBuf, process::Command};

pub fn handle_command() -> io::Result<()> {
    let mut args = env::args_os().skip(1);
    let option = args.next();
    let executable = env::current_exe()?;
    let bundled_launcher = executable
        .ancestors()
        .skip(1)
        .find(|root| root.join("runtime/linux-x86_64/arch-xfce.sha256").is_file())
        .map(|root| root.join("scripts/arch-oci"))
        .filter(|launcher| launcher.is_file());

    let flag = option.as_deref().and_then(|option| option.to_str());
    match flag {
        Some("--desktop") if bundled_launcher.is_none() && cfg!(target_arch = "x86_64") => {
            update(false)
        }
        Some("--desktop" | "--viewer-desktop") => {
            let launcher = bundled_launcher
                .as_deref()
                .filter(|_| flag == Some("--desktop"));
            install(&executable, launcher)?;
            std::process::exit(0);
        }
        Some("--update") => {
            let edge = match args.next() {
                None => false,
                Some(arg) if arg == "--edge" && args.next().is_none() => true,
                _ => {
                    eprintln!("Usage: presidio --update [--edge]");
                    std::process::exit(2);
                }
            };
            update(edge)
        }
        Some("--edge") => {
            eprintln!("Use presidio --update --edge to install a preview.");
            std::process::exit(2);
        }
        None if option.is_none() => {
            let launcher = bundled_launcher
                .or_else(|| Some(data_home().ok()?.join("presidio/current/scripts/arch-oci")));
            match launcher.filter(|path| path.is_file()) {
                Some(launcher) => Err(Command::new(launcher).arg("--launch").exec()),
                None => Ok(()),
            }
        }
        _ => Ok(()),
    }
}

fn update(edge: bool) -> io::Result<()> {
    let mut command = Command::new("bash");
    command.args([
        "-c",
        include_str!("../scripts/install-workspace"),
        "install-workspace",
    ]);
    if edge {
        command.arg("--edge");
    }
    Err(command.exec())
}

fn data_home() -> io::Result<PathBuf> {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| env::var_os("HOME").map(|home| Path::new(&home).join(".local/share")))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no user data directory"))
}

fn install(executable: &Path, launcher: Option<&Path>) -> io::Result<()> {
    let exec = match launcher {
        Some(launcher) => format!("{} --launch", exec_path(launcher)?),
        None => exec_path(executable)?,
    };

    let data = data_home()?;
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
