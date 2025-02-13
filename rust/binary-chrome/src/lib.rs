use binary::{
    common::{
        async_trait::async_trait,
        eyre::{bail, Result},
    },
    Binary, BinaryTrait,
};
use std::{fs::read_dir, path::Path};

pub struct ChromeBinary;

#[async_trait]
impl BinaryTrait for ChromeBinary {
    fn spec(&self) -> Binary {
        Binary::new(
            "chrome",
            &["Google Chrome"],
            &[
                "/Applications/Google Chrome.app/Contents/MacOS",
                "C:\\Program Files\\Google\\Chrome\\Application",
            ],
        )
    }

    fn clone_box(&self) -> Box<dyn BinaryTrait> {
        Box::new(Self {})
    }

    async fn versions(&self, _os: &str) -> Result<Vec<String>> {
        // Version history at https://en.wikipedia.org/wiki/Google_Chrome_version_history.
        // Rather than support installing multiple versions, we normally only support the
        // most recent version in the stable channel.
        // Note: Use triples ending in `.0` here and make sure there is a mapping in the
        // `install_version` method.
        Ok(vec!["96.0.0".to_string()])
    }

    /// Get the version of the Chrome binary
    ///
    /// This is necessary because on Windows a bug prevents the use of `--version`.
    /// Here we search for the empty directory with the version as its name.
    /// See https://stackoverflow.com/questions/50880917/how-to-get-chrome-version-using-command-prompt-in-windows
    /// for more details and alternative approaches.
    fn version(&self, path: &Path) -> Option<String> {
        let spec = self.spec();
        if cfg!(target_os = "windows") {
            let dir = path.parent().unwrap_or(path);
            if let Ok(entries) = read_dir(dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let parts: Vec<&str> = name.split('.').take(3).collect();
                    if parts.len() == 3 {
                        let version = parts.join(".");
                        return Some(version);
                    }
                }
            }
        }
        spec.version(path)
    }

    async fn install_version(
        &self,
        version: &str,
        dest: &Path,
        os: &str,
        _arch: &str,
    ) -> Result<()> {
        // Chrome uses a peculiar version system with the build number
        // at the third position and not every build for every OS. So, use minor version
        // for mapping
        let minor_version = version.split('.').take(2).collect::<Vec<&str>>().join(".");

        // Map the minor_version to a "position" number which can be obtained from
        // https://vikyd.github.io/download-chromium-history-version.
        // Note: the position number may be different for each os/arch
        let suffix = match minor_version.as_ref() {
            "96.0" => match os {
                "macos" => "Mac/925110/chrome-mac.zip",
                "windows" => "Win_x64/925110/chrome-win.zip",
                "linux" => "Linux_x64/926934/chrome-linux.zip",
                _ => bail!("Unmapped OS '{}'", os),
            },
            _ => bail!("Unmapped version number '{}'", version),
        };

        let url = format!(
            "https://www.googleapis.com/download/storage/v1/b/chromium-browser-snapshots/o/{suffix}?alt=media",
            suffix = suffix.replace('/', "%2F")
        );
        let filename = format!("chrome-v{version}-{os}.zip", version = version, os = os);
        let archive = self.download(&url, Some(filename), None).await?;

        self.extract(&archive, dest, 1)?;
        self.executables(dest, &["chrome", "chrome.exe"])?;

        Ok(())
    }
}
