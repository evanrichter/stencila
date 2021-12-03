//! Utilities for managing and calling external binaries

use defaults::Defaults;
use eyre::{bail, Result};
use itertools::Itertools;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use std::{
    cmp::Ordering,
    collections::HashMap,
    env::{
        self,
        consts::{ARCH, OS},
    },
    fs, io,
    path::{Path, PathBuf},
    process::{Output, Stdio},
    str::FromStr,
};
use tokio::{
    process::{Child, Command},
    sync::Mutex,
};

mod binaries;

///! A module for locating, running and installing third party binaries.
///!
///! Binaries may be used as runtimes for plugins (e.g. Node.js, Python) or
///! are used directly by the `methods` module (e.g Pandoc).
///!
///! This modules defines the `Binary` struct that can be used to define a
///! binary (e.g. how to determine the current version, how to download
///! a desired version) and functions for resolving, installing and executing
///! those binaries.

/// Get the directory where binaries are stored
pub fn binaries_dir() -> PathBuf {
    let user_data_dir = dirs::data_dir().unwrap_or_else(|| env::current_dir().unwrap());
    match env::consts::OS {
        "macos" | "windows" => user_data_dir.join("Stencila").join("Binaries"),
        _ => user_data_dir.join("stencila").join("binaries"),
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct BinaryInstallation {
    /// The name of the binary
    #[serde(skip)]
    pub name: String,

    /// The path the binary is installed to
    pub path: PathBuf,

    /// The version of the binary at the path
    pub version: Option<String>,
}

impl BinaryInstallation {
    /// Create an instance
    pub fn new(name: String, path: PathBuf, version: Option<String>) -> BinaryInstallation {
        BinaryInstallation {
            name,
            path,
            version,
        }
    }

    /// Get the command for the binary
    pub fn command(&self) -> Command {
        Command::new(&self.path)
    }

    /// Run the binary
    ///
    /// Returns the output of the command
    pub async fn run(&self, args: &[String]) -> Result<Output> {
        let output = self.command().args(args).output().await?;
        Ok(output)
    }

    /// Run the binary and connect to stdin, stdout and stderr streams
    ///
    /// Returns a `Child` process whose
    pub fn interact(&self, args: &[String]) -> Result<Child> {
        Ok(self
            .command()
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?)
    }
}

#[derive(Defaults, Serialize)]
struct Binary {
    /// The name of the binary
    name: String,

    /// Any aliases used to search for the binary
    aliases: Vec<String>,

    /// Installations of the binary found locally
    installations: Vec<BinaryInstallation>,

    /// Versions of the binary that this module supports
    /// installation of.
    ///
    /// Used to select a version to install based on semver
    /// requirements.
    installable: Vec<String>,

    /// The arguments used to get the version of the binary
    #[serde(skip)]
    #[def = r#"vec!["--version".to_string()]"#]
    version_args: Vec<String>,

    /// The regex used to get the version from the output of
    /// the binary.
    #[serde(skip)]
    #[def = r#"Regex::new("\\d+.\\d+(.\\d+)?").unwrap()"#]
    version_regex: Regex,
}

impl Clone for Binary {
    fn clone(&self) -> Binary {
        Binary {
            name: self.name.clone(),
            aliases: self.aliases.clone(),
            installations: self.installations.clone(),
            installable: self.installable.clone(),
            ..Default::default()
        }
    }
}

impl Binary {
    /// Define a binary
    pub fn new(name: &str, aliases: &[&str], versions: &[&str]) -> Binary {
        Binary {
            name: name.to_string(),
            aliases: aliases
                .iter()
                .map(|s| String::from_str(s).unwrap())
                .collect(),
            installable: versions
                .iter()
                .map(|s| String::from_str(s).unwrap())
                .rev()
                .collect(),
            ..Default::default()
        }
    }

    /// Get the directory where versions of a binary are installed
    pub fn dir(&self, version: Option<String>, ensure: bool) -> Result<PathBuf> {
        let dir = binaries_dir().join(&self.name);
        let dir = if let Some(version) = version {
            dir.join(version)
        } else {
            dir
        };

        if ensure {
            fs::create_dir_all(&dir)?
        }

        Ok(dir)
    }

    /// Get the version of the binary at a path
    ///
    /// Parses the output of the command and adds a `0` patch semver part if
    /// necessary.
    pub fn version(&self, path: &Path) -> Option<String> {
        let output = std::process::Command::new(path)
            .args(&self.version_args)
            .output();
        if let Ok(output) = output {
            let stdout = std::str::from_utf8(&output.stdout).unwrap_or("");
            if let Some(version) = self.version_regex.captures(stdout).map(|captures| {
                let mut parts: Vec<&str> = captures[0].split('.').collect();
                while parts.len() < 3 {
                    parts.push("0")
                }
                parts.join(".")
            }) {
                return Some(version);
            }
        }
        None
    }

    /// Resolve the path and version of a binary
    pub fn resolve(&mut self) {
        // Collect the directories for previously installed versions
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Ok(dir) = self.dir(None, false) {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        // Search for binary in top level (Windows)
                        dirs.push(path.clone());
                        // Search for binary in `bin` (MacOS & Linux convention)
                        dirs.push(path.join("bin"))
                    }
                }
            }
        }
        tracing::debug!("Found existing dirs {:?}", dirs);

        // Add the system PATH env var
        if let Some(path) = env::var_os("PATH") {
            tracing::debug!("Found PATH {:?}", path);
            let mut paths = env::split_paths(&path).collect();
            dirs.append(&mut paths);
        }

        // Join together in a PATH style string
        let dirs = if !dirs.is_empty() {
            match env::join_paths(dirs) {
                Ok(joined) => Some(joined),
                Err(error) => {
                    tracing::warn!("While joining paths: {}", error);
                    None
                }
            }
        } else {
            None
        };

        // Search for executables with name or one of aliases
        // tracing::debug!("Searching for executables in {:?}", dirs);
        let names = [vec![self.name.clone()], self.aliases.clone()].concat();
        let paths = names
            .iter()
            .map(|name| {
                match which::which_in_all(name, dirs.clone(), std::env::current_dir().unwrap()) {
                    Ok(paths) => paths.collect(),
                    Err(error) => {
                        tracing::warn!(
                            "While searching for executables for {}: {}",
                            self.name,
                            error
                        );
                        Vec::new()
                    }
                }
            })
            .flatten();

        // Get version of each executable found
        // tracing::debug!("Getting versions for paths {:?}", paths);
        let mut installs: Vec<BinaryInstallation> = paths
            .map(|path| {
                BinaryInstallation::new(self.name.clone(), path.clone(), self.version(&path))
            })
            .collect();

        // Sort the installations by descending order of version so that
        // the most recent version (meeting semver requirements) is returned by `installation()`.
        installs.sort_by(|a, b| match (&a.version, &b.version) {
            (Some(a), Some(b)) => {
                let a = semver::Version::parse(a).unwrap();
                let b = semver::Version::parse(b).unwrap();
                a.partial_cmp(&b).unwrap_or(Ordering::Equal)
            }
            (Some(..), None) => Ordering::Greater,
            (None, Some(..)) => Ordering::Less,
            (None, None) => Ordering::Equal,
        });
        installs.reverse();

        self.installations = installs
    }

    /// Are any versions installed that match the semver requirement (if specified)
    pub fn installation(&self, semver: Option<String>) -> Result<Option<BinaryInstallation>> {
        if let Some(semver) = semver {
            let semver = semver::VersionReq::parse(&semver)?;
            for install in &self.installations {
                if let Some(version) = &install.version {
                    let version = semver::Version::parse(version)?;
                    if semver.matches(&version) {
                        return Ok(Some(install.clone()));
                    }
                }
            }
            Ok(None)
        } else if let Some(install) = self.installations.first() {
            Ok(Some(install.clone()))
        } else {
            Ok(None)
        }
    }

    /// Install the most recent version of the binary (meeting optional semver, OS,
    /// and arch requirements).
    pub async fn install(
        &mut self,
        semver: Option<String>,
        os: Option<String>,
        arch: Option<String>,
    ) -> Result<()> {
        let semver = if let Some(semver) = semver {
            semver
        } else {
            self.installable
                .first()
                .expect("Always at least one version")
                .clone()
        };
        let semver = semver::VersionReq::parse(&semver)?;

        if let Some(version) = self.installable.iter().find_map(|version| {
            match semver
                .matches(&semver::Version::parse(version).expect("Version to always be valid"))
            {
                true => Some(version),
                false => None,
            }
        }) {
            self.install_version(version, os, arch).await?;
        } else {
            bail!(
                "No known version of '{}' which meets semantic version requirement '{}'",
                self.name,
                semver
            )
        }

        // Always re-resolve after an install
        self.resolve();

        Ok(())
    }

    /// Install a specific version of the binary
    pub async fn install_version(
        &self,
        version: &str,
        os: Option<String>,
        arch: Option<String>,
    ) -> Result<()> {
        let os = os.unwrap_or_else(|| OS.to_string());
        let arch = arch.unwrap_or_else(|| ARCH.to_string());
        match self.name.as_ref() {
            "chrome" => self.install_chrome(version, &os, &arch).await,
            "node" => self.install_node(version, &os, &arch).await,
            "pandoc" => self.install_pandoc(version, &os, &arch).await,
            "python" => self.install_python(version, &os, &arch).await,
            _ => bail!(
                "Stencila is not able to install '{name}'.",
                name = self.name
            ),
        }
    }

    /// Install Chrome
    async fn install_chrome(&self, version: &str, os: &str, _arch: &str) -> Result<()> {
        // Chrome uses a peculiar version system with the build number
        // at the third position and not every build for every OS. So, use minor versio
        // for mapping
        let minor_version = version.split('.').take(2).join(".");
        // Map the minor_version to a "position" number which can be obtained from
        // https://vikyd.github.io/download-chromium-history-version
        let suffix = match minor_version.as_ref() {
            "91.0" => match os {
                "macos" => "Mac/869727/chrome-mac.zip",
                "windows" => "Win_x64/867878/chrome-win.zip",
                "linux" => "Linux_x64/860960/chrome-linux.zip",
                _ => bail!("Unmapped OS '{}'", os),
            },
            _ => bail!("Unmapped version number '{}'", version),
        };

        let url = format!(
            "https://www.googleapis.com/download/storage/v1/b/chromium-browser-snapshots/o/{suffix}?alt=media",
            suffix = suffix.replace("/", "%2F")
        );

        let archive = self.download(&url).await?;
        let dest = self.dir(Some(version.into()), true)?;
        self.extract(&archive, 1, &self.dir(Some(version.into()), true)?)?;
        self.executable(&dest, &["chrome", "chrome.exe"])?;

        Ok(())
    }

    /// Install Node.js
    async fn install_node(&self, version: &str, os: &str, arch: &str) -> Result<()> {
        let url = format!(
            "https://nodejs.org/dist/v{version}/node-v{version}-",
            version = version
        ) + match os {
            "macos" => match arch {
                "arm" => "darwin-arm64.tar.gz",
                _ => "darwin-x64.tar.gz",
            },
            "windows" => match arch {
                "x86" => "win-x86.zip",
                _ => "win-x64.zip",
            },
            "linux" => match arch {
                "arm" => "linux-arm64.tar.xz",
                _ => "linux-x64.tar.xz",
            },
            _ => bail!("Unable to determine Node download URL"),
        };

        let archive = self.download(&url).await?;
        let dest = self.dir(Some(version.into()), true)?;
        self.extract(&archive, 1, &dest)?;
        self.executable(&dest, &["bin/node", "bin/npm", "node.exe", "npm"])?;

        Ok(())
    }

    /// Install Pandoc
    async fn install_pandoc(&self, version: &str, os: &str, arch: &str) -> Result<()> {
        // Map standard semver triples to Pandoc's version numbers
        // See https://github.com/jgm/pandoc/releases
        let version = match version {
            "2.14.0" => "2.14.0.3",
            "2.15.0" => "2.15",
            "2.16.0" => "2.16",
            _ => version,
        };

        let url = format!(
            "https://github.com/jgm/pandoc/releases/download/{version}/pandoc-{version}-",
            version = version
        ) + match os {
            "macos" => "macOS.zip",
            "windows" => "windows-x86_64.zip",
            "linux" => match arch {
                "arm" => "linux-arm64.tar.gz",
                _ => "linux-amd64.tar.gz",
            },
            _ => bail!("Unable to determine Pandoc download URL"),
        };

        let archive = self.download(&url).await?;
        let dest = self.dir(Some(version.into()), true)?;
        self.extract(&archive, 1, &dest)?;
        self.executable(&dest, &["bin/pandoc", "pandoc.exe"])?;

        Ok(())
    }

    /// Install Python
    ///
    /// On Windows uses Pythons "embeddable" distributions intended for this purpose.
    async fn install_python(&self, version: &str, os: &str, arch: &str) -> Result<()> {
        let url = format!(
            "https://www.python.org/ftp/python/{version}/python-{version}-embed-",
            version = version
        ) + match os {
            "windows" => match arch {
                "x86" => "win32.zip",
                "x86_64" => "amd64.zip",
                _ => bail!("Unhandled arch '{}", arch),
            },
            _ => bail!(
                "Stencila is unable to install Python for operating system '{}'.",
                os
            ),
        };

        let archive = self.download(&url).await?;
        let dest = self.dir(Some(version.into()), true)?;
        self.extract(&archive, 0, &dest)?;
        self.executable(&dest, &["bin/python3", "python3.exe"])?;

        Ok(())
    }

    /// Download a URL (usually an archive) to a temporary, but optionally cached, file
    async fn download(&self, url: &str) -> Result<PathBuf> {
        let url_parsed = url::Url::parse(url)?;
        let filename = url_parsed
            .path_segments()
            .expect("No segments in URL")
            .last()
            .expect("No file in URL");
        let path = binaries_dir().join("downloads").join(filename);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?
        }

        // Reuse downloaded files, only use this during development
        // and testing to avoid multiple downloads
        #[cfg(ignore)]
        if path.exists() {
            return Ok(path);
        }

        tracing::info!("📥 Downloading {} to {}", url, path.display());
        let response = reqwest::get(url).await?.error_for_status()?;
        let bytes = response.bytes().await?;
        let mut file = fs::File::create(&path)?;
        io::copy(&mut bytes.as_ref(), &mut file)?;

        Ok(path)
    }

    /// Extract an archive to a destination
    fn extract(&self, path: &Path, strip: usize, dest: &Path) -> Result<()> {
        tracing::info!("🔓 Extracting {} to {}", path.display(), dest.display());

        let ext = path
            .extension()
            .expect("Always has an extension")
            .to_str()
            .expect("Always convertible to str");

        match ext {
            "zip" => self.extract_zip(path, strip, dest),
            _ => self.extract_tar(ext, path, strip, dest),
        }
    }

    /// Extract a tar archive
    fn extract_tar(&self, ext: &str, file: &Path, strip: usize, dest: &Path) -> Result<()> {
        let file = fs::File::open(&file)?;
        let mut archive = tar::Archive::new(match ext {
            "tar" => Box::new(file) as Box<dyn io::Read>,
            "gz" | "tgz" => Box::new(flate2::read::GzDecoder::new(file)),
            "xz" => Box::new(xz2::read::XzDecoder::new(file)),
            _ => bail!("Unhandled archive extension {}", ext),
        });

        tracing::debug!(
            "Extracted {} entries",
            archive
                .entries()?
                .filter_map(|entry| entry.ok())
                .map(|mut entry| -> Result<()> {
                    let mut path = entry.path()?.display().to_string();
                    if strip > 0 {
                        let mut components: Vec<String> =
                            path.split('/').map(String::from).collect();
                        components.drain(0..strip);
                        path = components.join("/")
                    }

                    let out_path = dest.join(&path);
                    entry.unpack(&out_path).expect("Unable to unpack");

                    Ok(())
                })
                .filter_map(|result| result.ok())
                .count()
        );
        Ok(())
    }

    /// Extract a zip archive
    fn extract_zip(&self, file: &Path, strip: usize, dest: &Path) -> Result<()> {
        let file = fs::File::open(&file)?;
        let mut archive = zip::ZipArchive::new(file)?;

        let mut count = 0;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index)?;
            let mut path = entry
                .enclosed_name()
                .expect("Always has an enclosed name")
                .display()
                .to_string();

            if strip > 0 {
                let mut components: Vec<String> = path.split('/').map(String::from).collect();
                components.drain(0..strip);
                path = components.join("/")
            }

            let out_path = dest.join(&path);
            if let Some(parent) = out_path.parent() {
                if let Err(error) = fs::create_dir_all(parent) {
                    if error.kind() != io::ErrorKind::AlreadyExists {
                        bail!(error)
                    }
                }
            }

            if entry.is_file() {
                let mut out_file = fs::File::create(out_path)?;
                std::io::copy(&mut entry, &mut out_file)?;
                count += 1;
            }
        }

        tracing::debug!("Extracted {} entries", count);
        Ok(())
    }

    /// Make extracted files executable (if they exists)
    ///
    /// While tar archives retain permissions, zip archives do not,
    /// so we need to make sure to do this.
    fn executable(&self, dir: &Path, files: &[&str]) -> Result<()> {
        for file in files {
            let path = dir.join(file);
            if path.exists() {
                #[cfg(any(target_os = "linux", target_os = "macos"))]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
                }
            }
        }
        Ok(())
    }

    /// Uninstall a version, or all versions, of a binary
    #[allow(dead_code)]
    pub async fn uninstall(&mut self, version: Option<String>) -> Result<()> {
        let dir = self.dir(version, false)?;
        if dir.exists() {
            fs::remove_dir_all(dir)?
        } else {
            tracing::warn!("No matching Stencila installed binary found")
        }

        // Always re-resolve after an uninstall
        self.resolve();

        Ok(())
    }
}

/// A global store of binaries
static BINARIES: Lazy<Mutex<HashMap<String, Binary>>> = Lazy::new(|| {
    let map = binaries::all()
        .into_iter()
        .map(|binary| (binary.name.clone(), binary))
        .collect();

    Mutex::new(map)
});

/// Get a binary
async fn binary(name: &str) -> Binary {
    let binaries = &mut *BINARIES.lock().await;
    if let Some(binary) = binaries.get(name) {
        binary.clone()
    } else {
        binaries.insert(
            name.to_string(),
            Binary {
                name: name.into(),
                ..Default::default()
            },
        );
        binaries
            .get(name)
            .expect("Should have just been inserted")
            .clone()
    }
}

/// A cache of installations used to memoize calls to `installation`.
static INSTALLATIONS: Lazy<Mutex<HashMap<String, BinaryInstallation>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Get an installation
///
/// This is a relatively expensive function, even if the binary is already installed,
/// because it searches the file system and executes commands to get their version.
/// Therefore, this function memoizes installations in `INSTALLATIONS` for each `name` and `semver`.
/// Each cached result is removed if the binary is installed or uninstalled.
pub async fn installation(name: &str, semver: &str) -> Result<BinaryInstallation> {
    let name_semver = [name, "@", semver].concat();

    let installations = &mut *INSTALLATIONS.lock().await;
    if let Some(installation) = installations.get(&name_semver) {
        return Ok(installation.clone());
    }

    let mut binary = binary(name).await;
    binary.resolve();

    let semver = if semver == "*" {
        None
    } else {
        Some(semver.into())
    };

    if let Some(installation) = binary.installation(semver)? {
        installations.insert(name_semver, installation.clone());
        Ok(installation)
    } else {
        bail!("No matching installation found")
    }
}

/// Is a binary installation meeting semantic versioning requirements installed?
pub async fn installed(name: &str, semver: &str) -> bool {
    installation(name, semver).await.is_ok()
}

/// Get a binary installation meeting semantic versioning requirements.
///
/// If the binary is already available, or automatic installs are configured, returns
/// a `BinaryInstallation` that can be used to run commands. Otherwise, errors
/// with a message that the required binary is not yet installed, or failed to install.
pub async fn require(name: &str, semver: &str) -> Result<BinaryInstallation> {
    if let Ok(installation) = installation(name, semver).await {
        return Ok(installation);
    }

    // TODO: Use an env var to set this?
    let auto = true;
    if auto {
        let name_semver = [name, "@", semver].concat();
        let semver = if semver == "*" {
            None
        } else {
            Some(semver.into())
        };

        let mut binary = binary(name).await;
        binary.install(semver.clone(), None, None).await?;

        let installations = &mut *INSTALLATIONS.lock().await;
        if let Some(installation) = binary.installation(semver)? {
            installations.insert(name_semver, installation.clone());
            Ok(installation)
        } else {
            bail!("Failed to automatically install binary '{}'", name)
        }
    } else {
        bail!("Required binary '{}' is not installed", name)
    }
}

#[cfg(feature = "cli")]
pub mod commands {
    use super::*;
    use cli_utils::structopt::StructOpt;
    use cli_utils::{async_trait::async_trait, result, Result, Run};

    #[derive(Debug, StructOpt)]
    #[structopt(
        about = "Manage helper binaries",
        setting = structopt::clap::AppSettings::ColoredHelp,
        setting = structopt::clap::AppSettings::VersionlessSubcommands
    )]
    pub struct Command {
        #[structopt(subcommand)]
        pub action: Action,
    }

    #[derive(Debug, StructOpt)]
    #[structopt(
        setting = structopt::clap::AppSettings::DeriveDisplayOrder
    )]
    pub enum Action {
        Show(Show),
        Installable(Installable),
        Install(Install),
        Uninstall(Uninstall),
        Run(Run_),
    }

    #[async_trait]
    impl Run for Command {
        async fn run(&self) -> Result {
            let Self { action } = self;
            match action {
                Action::Show(action) => action.run().await,
                Action::Installable(action) => action.run().await,
                Action::Install(action) => action.run().await,
                Action::Uninstall(action) => action.run().await,
                Action::Run(action) => action.run().await,
            }
        }
    }

    /// Show information on a binary
    ///
    /// Searches for the binary on your path and in Stencila's "binaries"
    /// folder for versions that are installed. Use the `semver` argument
    /// if you only want to show the binary if the semantic version
    /// requirement is met.
    ///
    /// This command should find any binary that is on your PATH
    /// (i.e. including those not in the `stencila binaries installable` list).
    #[derive(Debug, StructOpt)]
    #[structopt(
        setting = structopt::clap::AppSettings::DeriveDisplayOrder,
        setting = structopt::clap::AppSettings::ColoredHelp
    )]
    pub struct Show {
        /// The name of the binary e.g. pandoc
        pub name: String,

        /// The semantic version requirement for the binary e.g. >2
        ///
        /// If this is supplied and the binary does not meet the semver
        /// requirement nothing will be shown
        pub semver: Option<String>,
    }

    #[async_trait]
    impl Run for Show {
        async fn run(&self) -> Result {
            let binary = if let Some(binary) = BINARIES.lock().await.get_mut(&self.name) {
                binary.resolve();
                binary.clone()
            } else {
                let mut binary = Binary {
                    name: self.name.clone(),
                    ..Default::default()
                };
                binary.resolve();
                binary
            };

            if binary.installation(self.semver.clone())?.is_some() {
                result::value(binary)
            } else {
                tracing::info!(
                    "No matching binary found. Perhaps use `stencila binaries install`?"
                );
                result::nothing()
            }
        }
    }

    /// List binaries (and their versions) that can be installed using Stencila
    ///
    /// The returned list is a list of the binaries/versions that Stencila knows how to install.
    #[derive(Debug, StructOpt)]
    #[structopt(
        setting = structopt::clap::AppSettings::DeriveDisplayOrder,
        setting = structopt::clap::AppSettings::ColoredHelp
    )]
    pub struct Installable {}

    #[async_trait]
    impl Run for Installable {
        async fn run(&self) -> Result {
            let binaries = &*BINARIES.lock().await;
            let list: Vec<serde_json::Value> = binaries
                .values()
                .map(|binary| {
                    serde_json::json!({
                        "name": binary.name.clone(),
                        "versions": binary.installable.clone()
                    })
                })
                .collect();
            result::value(list)
        }
    }

    /// Install a binary
    ///
    /// Installs the latest version of the binary, that also meets any
    /// semantic version requirement supplied, into the Stencila "binaries"
    /// folder.
    #[derive(Debug, StructOpt)]
    #[structopt(
        setting = structopt::clap::AppSettings::DeriveDisplayOrder,
        setting = structopt::clap::AppSettings::ColoredHelp
    )]
    pub struct Install {
        /// The name of the binary (must be a registered binary name)
        pub name: String,

        /// The semantic version requirement (the most recent matching version will be installed)
        pub semver: Option<String>,

        /// The operating system to install for (defaults to the current)
        #[structopt(short, long, possible_values = &OS_VALUES )]
        pub os: Option<String>,

        /// The architecture to install for (defaults to the current)
        #[structopt(short, long, possible_values = &ARCH_VALUES)]
        pub arch: Option<String>,
    }

    const OS_VALUES: [&str; 3] = ["macos", "windows", "linux"];
    const ARCH_VALUES: [&str; 3] = ["x86", "x86_64", "arm"];

    #[async_trait]
    impl Run for Install {
        async fn run(&self) -> Result {
            if let Some(binary) = BINARIES.lock().await.get_mut(&self.name) {
                binary
                    .install(self.semver.clone(), self.os.clone(), self.arch.clone())
                    .await?;
                tracing::info!("📦 Installed {}", self.name);
            } else {
                tracing::warn!("No registered binary with that name. See `stencila binaries list`.")
            }

            result::nothing()
        }
    }

    /// Uninstall a binary
    ///
    /// Removes the binary (optionally, just a specific version) from the Stencila
    /// "binaries" folder. No other installations of the binary on the system will
    /// will be removed.
    #[derive(Debug, StructOpt)]
    #[structopt(
        setting = structopt::clap::AppSettings::DeriveDisplayOrder,
        setting = structopt::clap::AppSettings::ColoredHelp
    )]
    pub struct Uninstall {
        /// The name of the binary (must be a registered binary name)
        pub name: String,

        /// The specific version of the binary to uninstall
        ///
        /// If this is not provided, all versions will be removed.
        pub version: Option<String>,
    }
    #[async_trait]
    impl Run for Uninstall {
        async fn run(&self) -> Result {
            if let Some(binary) = BINARIES.lock().await.get_mut(&self.name) {
                binary.uninstall(self.version.clone()).await?;
                tracing::info!("🗑️ Uninstalled {}", self.name);
            } else {
                tracing::warn!("No registered binary with that name. See `stencila binaries list`.")
            }

            result::nothing()
        }
    }

    /// Run a command using a binary
    ///
    /// Pass arguments and options to the binary after the `--` flag.
    #[derive(Debug, StructOpt)]
    #[structopt(
        setting = structopt::clap::AppSettings::DeriveDisplayOrder,
        setting = structopt::clap::AppSettings::ColoredHelp
    )]
    pub struct Run_ {
        /// The name of the binary e.g. node
        pub name: String,

        /// The semantic version requirement e.g. 16
        pub semver: Option<String>,

        /// The arguments and options to pass to the binary
        #[structopt(raw(true))]
        pub args: Vec<String>,
    }

    #[async_trait]
    impl Run for Run_ {
        async fn run(&self) -> Result {
            let installation = require(
                &self.name,
                &self.semver.clone().unwrap_or_else(|| "*".to_string()),
            )
            .await?;
            let output = installation.run(&self.args).await?;

            use std::io::Write;
            std::io::stdout().write_all(output.stdout.as_ref())?;
            std::io::stderr().write_all(output.stderr.as_ref())?;

            result::nothing()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Result;

    // End to end CLI test that install, show and uninstall
    // the latest version of each binary. Intended as a coarse
    // tests of installation of each binary. These tests are
    // tagged with #[ignore] because they are slow, so in development
    // you don't want to run them, and because if they are run in
    // parallel with other tests that use `require()` they can cause deadlocks
    // and other on-disk conflicts.

    // Run this test at the start of CI tests using
    //   cargo test binaries::tests::install -- --ignored --nocapture
    #[cfg(feature = "cli")]
    #[tokio::test]
    #[ignore]
    async fn install() -> Result<()> {
        use super::commands::{Install, Installable, Show};
        use cli_utils::Run;
        use eyre::bail;
        use test_utils::assert_json_eq;

        Installable {}.run().await?;

        let binaries = (*super::BINARIES.lock().await).clone();
        for name in binaries.keys() {
            eprintln!("Testing {}", name);

            Install {
                name: name.clone(),
                semver: None,
                os: None,
                arch: None,
            }
            .run()
            .await?;

            let display = Show {
                name: name.clone(),
                semver: None,
            }
            .run()
            .await?;

            let value = if let Some(value) = display.value {
                value
            } else {
                bail!("Expected value")
            };
            assert_json_eq!(value.get("name"), Some(name.clone()));
            assert!(!value
                .get("installs")
                .expect("To have installs")
                .as_array()
                .expect("To be array")
                .is_empty());
        }

        Ok(())
    }

    // Run this test at the end of CI tests using
    //   cargo test binaries::tests::uninstall -- --ignored --nocapture
    #[cfg(feature = "cli")]
    #[tokio::test]
    #[ignore]
    async fn uninstall() -> Result<()> {
        use super::commands::Uninstall;
        use cli_utils::Run;

        let binaries = (*super::BINARIES.lock().await).clone();
        for name in binaries.keys() {
            Uninstall {
                name: name.clone(),
                version: None,
            }
            .run()
            .await?;
        }

        Ok(())
    }
}
