use schemars::JsonSchema;
use serde::Serialize;
use std::path::Path;
use strum::{EnumIter, IntoEnumIterator};

#[derive(Debug, PartialEq, EnumIter)]
pub enum Format {
    Bash,
    Calc,
    Directory,
    Dockerfile,
    Docx,
    Flac,
    Gif,
    Html,
    Ipynb,
    JavaScript,
    Jpeg,
    Json,
    Json5,
    LaTeX,
    Makefile,
    Markdown,
    Mp3,
    Mp4,
    Odt,
    Ogg,
    Ogv,
    PlainText,
    Png,
    Python,
    R,
    RMarkdown,
    Rpng,
    Rust,
    Shell,
    ThreeGpp,
    Toml,
    TypeScript,
    Unknown,
    WebM,
    Xml,
    Yaml,
    Zsh,
}

impl Format {
    /// A list of known formats
    #[rustfmt::skip]
    pub fn spec(&self) -> FormatSpec {
        match self {

            // Data serialization formats. These may be used to store documents so `preview: true`.
            Format::Json => FormatSpec::new("JSON", "json", &[], false, true, FormatNodeType::Unknown),
            Format::Json5 => FormatSpec::new("JSON5", "json5", &[], false, true, FormatNodeType::Unknown),

            Format::Toml => FormatSpec::new("TOML", "toml", &[], false, true, FormatNodeType::Unknown),
            Format::Xml => FormatSpec::new("XML", "xml", &[], false, true, FormatNodeType::Unknown),
            Format::Yaml => FormatSpec::new("YAML", "yaml", &[], false, true, FormatNodeType::Unknown),

            // Code formats
            Format::Bash => FormatSpec::new("Bash", "bash", &[], false, false, FormatNodeType::SoftwareSourceCode),
            Format::Calc => FormatSpec::new("Calc", "calc", &[], false, false, FormatNodeType::SoftwareSourceCode),
            Format::Dockerfile => FormatSpec::new("Dockerfile", "dockerfile", &[], false, false, FormatNodeType::SoftwareSourceCode),
            Format::JavaScript => FormatSpec::new("JavaScript", "js", &[], false, false, FormatNodeType::SoftwareSourceCode),
            Format::Makefile => FormatSpec::new("Makefile", "makefile", &[], false, false, FormatNodeType::SoftwareSourceCode),
            Format::Python => FormatSpec::new("Python", "py", &["python3"], false, false, FormatNodeType::SoftwareSourceCode),
            Format::R => FormatSpec::new("R", "r", &[], false, false, FormatNodeType::SoftwareSourceCode),
            Format::Rust => FormatSpec::new("Rust", "rust", &[], false, false, FormatNodeType::SoftwareSourceCode),
            Format::Shell => FormatSpec::new("Shell", "sh", &[], false, false, FormatNodeType::SoftwareSourceCode),
            Format::TypeScript => FormatSpec::new("Typescript", "ts", &[], false, false, FormatNodeType::SoftwareSourceCode),
            Format::Zsh => FormatSpec::new("ZSH", "zsh", &[], false, false, FormatNodeType::SoftwareSourceCode),

            // Article formats
            Format::Docx => FormatSpec::new("Microsoft Word", "docx", &[], true, true, FormatNodeType::Article),
            Format::Html => FormatSpec::new("HTML", "html", &[], false, true, FormatNodeType::Article),
            Format::Ipynb => FormatSpec::new("Jupyter Notebook", "ipynb", &[], false, true, FormatNodeType::Article),
            Format::Markdown => FormatSpec::new("Markdown", "md", &[], false, true, FormatNodeType::Article),
            Format::Odt => FormatSpec::new("Open Office Text", "odt", &[], true, true, FormatNodeType::Article),
            Format::RMarkdown => FormatSpec::new("R Markdown", "rmd", &[], false, true, FormatNodeType::Article),
            Format::LaTeX => FormatSpec::new("LaTeX", "latex", &["tex"], false, true, FormatNodeType::Article),

            // Audio formats
            Format::Flac => FormatSpec::new("FLAC", "flac", &[], true, true, FormatNodeType::AudioObject),
            Format::Mp3 => FormatSpec::new("MP3", "mp3", &[], true, true, FormatNodeType::AudioObject),
            Format::Ogg => FormatSpec::new("Ogg", "ogg", &[], true, true, FormatNodeType::AudioObject),

            // Image formats
            Format::Gif => FormatSpec::new("GIF", "gif", &[], true, true, FormatNodeType::ImageObject),
            Format::Jpeg => FormatSpec::new("JPEG", "jpg", &["jpeg"], true, true, FormatNodeType::ImageObject),
            Format::Png => FormatSpec::new("PNG", "png", &[], true, true, FormatNodeType::ImageObject),
            Format::Rpng => FormatSpec::new("RPNG", "rpng", &[], true, true, FormatNodeType::ImageObject),

            // Video formats
            Format::ThreeGpp => FormatSpec::new("3GPP", "3gp", &[], true, true, FormatNodeType::VideoObject),
            Format::Mp4 => FormatSpec::new("MP4", "mp4", &[], true, true, FormatNodeType::VideoObject),
            Format::Ogv => FormatSpec::new("Ogg Video", "ogv", &[], true, true, FormatNodeType::VideoObject),
            Format::WebM => FormatSpec::new("WebM", "webm", &[], true, true, FormatNodeType::VideoObject),

            // Other
            Format::PlainText => FormatSpec::new("Plain text", "txt", &[], false, false, FormatNodeType::Unknown),

            // Specials
            Format::Directory => FormatSpec::directory(),
            Format::Unknown => FormatSpec::unknown("unknown")
        }
    }
}

/// The type of format as a schema `Node` type
#[derive(Clone, Debug, PartialEq, JsonSchema, Serialize)]
pub enum FormatNodeType {
    Article,
    AudioObject,
    ImageObject,
    VideoObject,
    SoftwareSourceCode,
    Unknown,
}

/// Specification of a format
///
/// Used to determine various application behaviors
/// e.g. not reading binary formats into memory unnecessarily
#[derive(Clone, Debug, PartialEq, JsonSchema, Serialize)]
#[schemars(deny_unknown_fields)]
pub struct FormatSpec {
    /// The title of the format e.g. "Markdown"
    pub title: String,

    /// The extension to use e.g. "md" when saving a file in this format
    ///
    /// Note: this is the "default" extension but other extensions can be
    /// listed in `aliases`.
    pub extension: String,

    /// Any additional names or extensions that this format
    /// should match against (should be lowercase).
    pub aliases: Vec<String>,

    /// Whether or not the format should be considered binary
    /// e.g. not to be displayed in a text / code editor
    pub binary: bool,

    /// Whether HTML previews are normally supported for documents of
    /// this format. See also `Document.previewable` which indicates whether
    /// a HTML preview is supported for a particular document.
    pub preview: bool,

    /// The kind of format
    pub node_type: FormatNodeType,

    /// Whether or not this is a known format specification (i.e. not automatically created)
    pub known: bool,
}

impl FormatSpec {
    /// Create a new format spec
    pub fn new(
        title: &str,
        extension: &str,
        aliases: &[&str],
        binary: bool,
        preview: bool,
        node_type: FormatNodeType,
    ) -> FormatSpec {
        FormatSpec {
            title: title.into(),
            extension: extension.into(),
            aliases: aliases.iter().map(|s| s.to_string()).collect(),
            binary,
            preview,
            node_type,
            known: true,
        }
    }

    /// Create the special `directory` format used on `File` objects
    /// that are directories
    pub fn directory() -> FormatSpec {
        FormatSpec {
            title: "Directory".to_string(),
            extension: "".into(),
            aliases: Vec::new(),
            binary: true,
            preview: false,
            node_type: FormatNodeType::Unknown,
            known: true,
        }
    }

    /// Create the special `unknown` file format where all we
    /// have is the name e.g. from a file extension.
    pub fn unknown(name: &str) -> FormatSpec {
        FormatSpec {
            title: name.to_string(),
            extension: name.to_lowercase(),
            aliases: Vec::new(),
            // Set binary to false so that any unregistered format
            // will be at least shown in editor...
            binary: false,
            // ..but not have a preview
            preview: false,
            node_type: FormatNodeType::Unknown,
            known: false,
        }
    }
}

/// Match a format name to a `Format`
///
/// Iterates over the `Format` variants and returns the first
/// that has a title, extension or aliases that match it.
pub fn match_name(name: &str) -> Format {
    let name = name.to_lowercase();
    for format in Format::iter() {
        let spec = format.spec();
        if name == spec.title.to_lowercase()
            || name == spec.extension
            || spec.aliases.contains(&name)
        {
            return format;
        }
    }
    Format::Unknown
}

/// Match a file path to a `Format`
///
/// Extracts a "name" (extension or file base name) from a
/// file path and then calls `match_name` on that.
pub fn match_path<P: AsRef<Path>>(path: &P) -> Format {
    let path = path.as_ref();

    // Get name from file extension, or filename if no extension
    let name = match path.extension() {
        Some(ext) => ext,
        None => match path.file_name() {
            Some(name) => name,
            // Fallback to the provided "path"
            None => path.as_os_str(),
        },
    };

    // Match that name
    match_name(&name.to_string_lossy().to_string())
}
