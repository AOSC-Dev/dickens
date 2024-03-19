use log::info;
use std::process::Command;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Library {
    pub soname: String,
    pub needed: Vec<String>,
}

pub fn get_library_and_deps(name: &str) -> anyhow::Result<Vec<Library>> {
    info!("Handling package {}", name);
    let mut res = vec![];
    let contents = String::from_utf8(Command::new("dpkg").arg("-L").arg(name).output()?.stdout)?;
    for file in contents.lines() {
        if file.starts_with("/usr/include/")
            || file.starts_with("/usr/share/")
            || file.starts_with("/etc/")
            || file.starts_with("/usr/lib/pkgconfig/")
            || file.starts_with("/usr/lib/gconv/")
        {
            continue;
        }

        info!("Handling file {}", file);
        let readelf_result =
            String::from_utf8(Command::new("readelf").arg("-d").arg(file).output()?.stdout)?;

        let mut soname: Option<&str> = None;
        let mut needed = vec![];
        for line in readelf_result.lines() {
            if line.contains("(NEEDED)") {
                needed.push(line.split("[").last().unwrap().split("]").next().unwrap());
            } else if line.contains("(SONAME)") {
                soname = Some(line.split("[").last().unwrap().split("]").next().unwrap());
            }
        }

        if let Some(soname) = soname {
            res.push(Library {
                soname: soname.to_string(),
                needed: needed.into_iter().map(str::to_string).collect(),
            });
        }
    }

    // dedup
    res.sort();
    res.dedup();
    Ok(res)
}

pub fn get_libraries(name: &str) -> anyhow::Result<Vec<String>> {
    info!("Handling package {}", name);
    let mut res: Vec<String> = vec![];
    let contents = String::from_utf8(Command::new("dpkg").arg("-L").arg(name).output()?.stdout)?;
    for file in contents.lines() {
        if file.starts_with("/usr/include/")
            || file.starts_with("/usr/share/")
            || file.starts_with("/etc/")
            || file.starts_with("/usr/lib/pkgconfig/")
            || file.starts_with("/usr/lib/gconv/")
        {
            continue;
        }

        info!("Handling file {}", file);
        let readelf_result =
            String::from_utf8(Command::new("readelf").arg("-d").arg(file).output()?.stdout)?;

        for line in readelf_result.lines() {
            if line.contains("(SONAME)") {
                res.push(file.split("/").last().unwrap().to_string());
                break;
            }
        }
    }

    // dedup
    res.sort();
    res.dedup();
    Ok(res)
}
