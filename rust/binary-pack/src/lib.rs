use std::path::Path;

pub use binary::BinaryTrait;
use binary::{
    binary_clone_box,
    common::{
        async_trait::async_trait,
        eyre::{bail, Result},
    },
    Binary,
};

mod versions;

pub struct PackBinary;

#[async_trait]
impl BinaryTrait for PackBinary {
    fn spec(&self) -> Binary {
        Binary::new("pack", &[], &[])
    }

    binary_clone_box!();

    async fn versions(&self, _os: &str) -> Result<Vec<String>> {
        let versions = self.versions_update_maybe(
            versions::VERSIONS,
            self.versions_github_releases("buildpacks", "pack").await,
        );
        Ok(self.semver_versions_matching(&versions, ">=0.20"))
    }

    async fn install_version(
        &self,
        version: &str,
        dest: &Path,
        os: &str,
        arch: &str,
    ) -> Result<()> {
        let url = format!(
            "https://github.com/buildpacks/pack/releases/download/v{version}/pack-v{version}-",
            version = version
        ) + match os {
            "linux" => "linux.tgz",
            "macos" => match arch {
                "arm" => "macos-arm64.tgz",
                _ => "macos.tgz",
            },
            "windows" => "windows.zip",
            _ => bail!(
                "Installation of `pack` for operating system `{}` is not supported",
                os
            ),
        };
        let archive = self.download(&url, None, None).await?;

        self.extract(&archive, dest, 0)?;
        self.executables(dest, &["pack"])?;

        Ok(())
    }
}
