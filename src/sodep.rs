use log::info;
use std::process::Command;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LibraryDependency {
    pub name: String,
    pub needed: Vec<String>,
}

pub fn get_library_deps(name: &str) -> anyhow::Result<Vec<LibraryDependency>> {
    info!("Handling package {}", name);
    let mut res = vec![];
    let output = Command::new("dpkg").arg("-L").arg(name).output()?;
    if !output.status.success() {
        anyhow::bail!("Failed to list files of package {}", name)
    }

    let contents = String::from_utf8(output.stdout)?;
    for file in contents.lines() {
        if file.starts_with("/usr/include/")
            || file.starts_with("/usr/share/")
            || file.starts_with("/etc/")
            || file.starts_with("/usr/lib/pkgconfig/")
            || file.starts_with("/usr/lib/gconv/")
        {
            continue;
        }

        let readelf_result =
            String::from_utf8(Command::new("readelf").arg("-d").arg(file).output()?.stdout)?;

        let mut needed = vec![];
        for line in readelf_result.lines() {
            if line.contains("(NEEDED)") {
                needed.push(line.split("[").last().unwrap().split("]").next().unwrap());
            }
        }

        if !needed.is_empty() {
            res.push(LibraryDependency {
                name: file.split("/").last().unwrap().to_string(),
                needed: needed.into_iter().map(str::to_string).collect(),
            });
            info!("Found file {}", file);
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
    let output = Command::new("dpkg").arg("-L").arg(name).output()?;
    if !output.status.success() {
        anyhow::bail!("Failed to list files of package {}", name)
    }

    let contents = String::from_utf8(output.stdout)?;
    for file in contents.lines() {
        if !file.contains(".so") {
            continue;
        }

        if file.starts_with("/usr/include/")
            || file.starts_with("/usr/share/")
            || file.starts_with("/etc/")
            || file.starts_with("/usr/lib/pkgconfig/")
            || file.starts_with("/usr/lib/gconv/")
        {
            continue;
        }

        let readelf_result =
            String::from_utf8(Command::new("readelf").arg("-d").arg(file).output()?.stdout)?;

        for line in readelf_result.lines() {
            if line.contains("(SONAME)") || line.contains("(NEEDED)") {
                res.push(file.split("/").last().unwrap().to_string());
                info!("Found file {}", file);
                break;
            }
        }
    }

    // dedup
    res.sort();
    res.dedup();
    Ok(res)
}
