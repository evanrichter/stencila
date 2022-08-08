use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    env, fs,
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use notify::DebouncedEvent;
use schemars::{gen::SchemaGenerator, schema::Schema, JsonSchema};

use common::{
    eyre::{self, bail, Result},
    indexmap::IndexMap,
    itertools::Itertools,
    maplit::hashset,
    once_cell::sync::Lazy,
    serde::Serialize,
    serde_json,
    serde_with::skip_serializing_none,
    strum::Display,
    tokio::{
        self,
        sync::{broadcast, mpsc, Mutex, RwLock},
        task::JoinHandle,
    },
    tracing,
};
use events::publish;
use formats::FormatSpec;
use graph::{Graph, PlanOptions, PlanOrdering, PlanScope};
use graph_triples::{resources, Relations};
use kernels::{KernelInfos, KernelSpace, KernelSymbols};
use node_address::{Address, AddressMap};
use node_execute::{
    compile, execute, CancelRequest, CompileRequest, ExecuteRequest, PatchRequest, RequestId,
    Response, WriteRequest,
};
use node_patch::{apply, diff, merge, Patch};
use node_pointer::{resolve, resolve_mut};
use node_reshape::reshape;
use node_validate::Validator;
use path_utils::pathdiff;
use providers::DetectItem;
use stencila_schema::{Article, InlineContent, Node, Parameter};

use crate::utils::schemas;

#[derive(Debug, JsonSchema, Serialize, Display)]
#[serde(rename_all = "lowercase", crate = "common::serde")]
#[strum(serialize_all = "lowercase")]
enum DocumentEventType {
    Deleted,
    Renamed,
    Modified,
    Patched,
    Encoded,
}

#[skip_serializing_none]
#[derive(Debug, JsonSchema, Serialize)]
#[serde(crate = "common::serde")]
#[schemars(deny_unknown_fields)]
struct DocumentEvent {
    /// The type of event
    #[serde(rename = "type")]
    type_: DocumentEventType,

    /// The document associated with the event
    #[schemars(schema_with = "DocumentEvent::schema_document")]
    document: Document,

    /// The content associated with the event, only provided for, `modified`
    /// and `encoded` events.
    content: Option<String>,

    /// The format of the document, only provided for `modified` (the format
    /// of the document) and `encoded` events (the format of the encoding).
    #[schemars(schema_with = "DocumentEvent::schema_format")]
    format: Option<FormatSpec>,

    /// The `Patch` associated with a `Patched` event
    #[schemars(schema_with = "DocumentEvent::schema_patch")]
    patch: Option<Patch>,
}

impl DocumentEvent {
    /// Generate the JSON Schema for the `document` property to avoid nesting
    fn schema_document(_generator: &mut SchemaGenerator) -> Schema {
        schemas::typescript("Document", true)
    }

    /// Generate the JSON Schema for the `format` property to avoid nesting
    fn schema_format(_generator: &mut schemars::gen::SchemaGenerator) -> Schema {
        schemas::typescript("Format", false)
    }

    /// Generate the JSON Schema for the `patch` property to avoid nesting
    fn schema_patch(_generator: &mut schemars::gen::SchemaGenerator) -> Schema {
        schemas::typescript("Patch", false)
    }
}

/// The status of a document with respect to on-disk synchronization
#[derive(Debug, Clone, JsonSchema, Serialize, Display)]
#[serde(rename_all = "lowercase", crate = "common::serde")]
#[strum(serialize_all = "lowercase")]
enum DocumentStatus {
    /// The document `content` is the same as on disk at its `path`.
    Synced,
    /// The document `content` has modifications that have not yet
    /// been written to its `path`.
    Unwritten,
    /// The document `path` has modifications that have not yet
    /// been read into its `content`.
    Unread,
    /// The document `path` no longer exists and is now set to `None`.
    /// The user will need to choose a new path for the document if they
    /// want to save it.
    Deleted,
}

/// An in-memory representation of a document
#[derive(Debug, JsonSchema, Serialize)]
#[serde(crate = "common::serde")]
#[schemars(deny_unknown_fields)]
pub struct Document {
    /// The document identifier
    pub id: String,

    /// The absolute path of the document's file.
    pub path: PathBuf,

    /// The project directory for this document.
    ///
    /// Used to restrict file links (e.g. image paths) to within
    /// the project for both security and reproducibility reasons.
    /// For documents opened from within a project, this will be project directory.
    /// For "orphan" documents (opened by themselves) this will be the
    /// parent directory of the document. When the document is compiled,
    /// an error will be returned if a file link is outside of the root.
    project: PathBuf,

    /// Whether or not the document's file is in the temporary
    /// directory.
    temporary: bool,

    /// The synchronization status of the document.
    /// This is orthogonal to `temporary` because a document's
    /// `content` can be synced or un-synced with the file system
    /// regardless of whether or not its `path` is temporary..
    status: DocumentStatus,

    /// The last time that the document was written to disk.
    ///
    /// Used to ignore file modification notification events generated by
    /// this application itself.
    #[serde(skip)]
    last_write: Arc<RwLock<Instant>>,

    /// The name of the document
    ///
    /// Usually the filename from the `path` but "Untitled"
    /// for temporary documents.
    name: String,

    /// The format of the document.
    ///
    /// On initialization, this is inferred, if possible, from the file name extension
    /// of the document's `path`. However, it may change whilst the document is
    /// open in memory (e.g. if the `load` function sets a different format).
    #[schemars(schema_with = "Document::schema_format")]
    format: FormatSpec,

    /// Whether a HTML preview of the document is supported
    ///
    /// This is determined by the type of the `root` node of the document.
    /// Will be `true` if the `root` is a type for which HTML previews are
    /// implemented e.g. `Article`, `ImageObject` and `false` if the `root`
    /// is `None`, or of some other type e.g. `Entity`.
    ///
    /// This flag is intended for dynamically determining whether to open
    /// a preview panel for a document by default. Regardless of its value,
    /// a user should be able to open a preview panel, in HTML or some other
    /// format, for any document.
    previewable: bool,

    /// The current UTF8 string content of the document.
    ///
    /// When a document is `read()` from a file the `content` is the content
    /// of the file. The `content` may subsequently be changed using
    /// the `load()` function. A call to `write()` will write the content
    /// back to `path`.
    ///
    /// Skipped during serialization because will often be large.
    #[serde(skip)]
    content: String,

    /// The root Stencila Schema node of the document
    ///
    /// Can be any type of `Node` but defaults to an empty `Article`.
    ///
    /// A [`RwLock`] to enable separate, concurrent tasks to read (e.g. for dumping to some
    /// format) and write (e.g. to apply patches from clients) the node.
    ///
    /// Skipped during serialization because will often be large.
    #[serde(skip)]
    root: Arc<RwLock<Node>>,

    /// Addresses of nodes in `root` that have an `id`
    ///
    /// Used to fetch a particular node (and do something with it like `patch`
    /// or `execute` it) rather than walking the node tree looking for it.
    /// It is necessary to use [`Address`] here (rather than say raw pointers) because
    /// pointers or references will change as the document is patched.
    /// These addresses are shifted when the document is patched to account for this.
    #[serde(skip)]
    addresses: Arc<RwLock<AddressMap>>,

    /// The kernel space for this document.
    ///
    /// This is where document variables are stored and executable nodes such as
    /// `CodeChunk`s and `Parameters`s are executed.
    #[serde(skip)]
    kernels: Arc<RwLock<KernelSpace>>,

    /// The set of dependency relations between this document, or nodes in this document,
    /// and other resources.
    ///
    /// Relations may be external (e.g. the document links to another `Resource::File`),
    /// or internal (e.g. the second code chunk uses a `Resource::Symbol` defined in the
    /// first code chunk).
    ///
    /// Stored for use in building the project's graph, but that may be removed
    /// in the future. Not serialized since this information is in `self.graph`.
    #[serde(skip)]
    pub relations: Relations,

    /// The document's dependency graph
    ///
    /// This is derived from `relations`.
    #[serde(skip)]
    pub graph: Arc<RwLock<Graph>>,

    /// The clients that are subscribed to each topic for this document
    ///
    /// Keeping track of client ids per topics allows for a some
    /// optimizations. For example, events will only be published on topics that have at least one
    /// subscriber.
    ///
    /// Valid subscription topics are the names of the `DocumentEvent` types:
    ///
    /// - `removed`: published when document file is deleted
    /// - `renamed`: published when document file is renamed
    /// - `modified`: published when document file is modified
    /// - `encoded:<format>` published when a document's content
    ///    is changed internally or externally and  conversions have been
    ///    completed e.g. `encoded:html`
    subscriptions: HashMap<String, HashSet<String>>,

    #[serde(skip)]
    patch_request_sender: mpsc::UnboundedSender<PatchRequest>,

    #[serde(skip)]
    compile_request_sender: mpsc::Sender<CompileRequest>,

    #[serde(skip)]
    execute_request_sender: mpsc::Sender<ExecuteRequest>,

    #[serde(skip)]
    cancel_request_sender: mpsc::Sender<CancelRequest>,

    #[serde(skip)]
    response_receiver: broadcast::Receiver<Response>,
}

#[allow(unused)]
impl Document {
    /// Generate the JSON Schema for the `format` property to avoid duplicated
    /// inline type.
    fn schema_format(_generator: &mut schemars::gen::SchemaGenerator) -> Schema {
        schemas::typescript("Format", true)
    }

    /// Generate the JSON Schema for the `addresses` property to avoid duplicated types.
    fn schema_addresses(_generator: &mut schemars::gen::SchemaGenerator) -> Schema {
        schemas::typescript("Record<string, Address>", true)
    }

    /// Create a new empty document.
    ///
    /// # Arguments
    ///
    /// - `path`: The path of the document; defaults to a temporary path.
    /// - `format`: The format of the document; defaults to plain text.
    ///
    /// This function is intended to be used by editors when creating
    /// a new document. If the `path` is not specified, the created document
    /// will be `temporary: true` and have a temporary file path.
    #[tracing::instrument]
    fn new(path: Option<PathBuf>, format: Option<String>) -> Document {
        let id = uuids::generate("do").to_string();

        let format = if let Some(format) = format {
            formats::match_path(&format)
        } else if let Some(path) = path.as_ref() {
            formats::match_path(path)
        } else {
            formats::match_name("txt")
        }
        .spec();
        let previewable = format.preview;

        let (path, name, temporary) = match path {
            Some(path) => {
                let name = path
                    .file_name()
                    .map(|os_str| os_str.to_string_lossy())
                    .unwrap_or_else(|| "Untitled".into())
                    .into();

                (path, name, false)
            }
            None => {
                let path = env::temp_dir().join(
                    [
                        uuids::generate("fi").to_string(),
                        ".".to_string(),
                        format.extension.clone(),
                    ]
                    .concat(),
                );
                // Ensure that the file exists
                if !path.exists() {
                    fs::write(path.clone(), "").expect("Unable to write temporary file");
                }

                let name = "Untitled".into();

                (path, name, true)
            }
        };

        let project = path
            .parent()
            .expect("Unable to get path parent")
            .to_path_buf();

        let root = Arc::new(RwLock::new(Node::Article(Article::default())));
        let addresses = Arc::new(RwLock::new(AddressMap::default()));
        let graph = Arc::new(RwLock::new(Graph::default()));
        let kernels = Arc::new(RwLock::new(KernelSpace::new(Some(&project))));
        let last_write = Arc::new(RwLock::new(Instant::now()));

        let (write_request_sender, mut write_request_receiver) =
            mpsc::unbounded_channel::<WriteRequest>();

        let (patch_request_sender, mut patch_request_receiver) =
            mpsc::unbounded_channel::<PatchRequest>();

        let (compile_request_sender, mut compile_request_receiver) =
            mpsc::channel::<CompileRequest>(100);

        let (execute_request_sender, mut execute_request_receiver) =
            mpsc::channel::<ExecuteRequest>(100);

        let (cancel_request_sender, mut cancel_request_receiver) =
            mpsc::channel::<CancelRequest>(100);

        let (response_sender, mut response_receiver) = broadcast::channel::<Response>(1);

        let root_clone = root.clone();
        let last_write_clone = last_write.clone();
        let path_clone = path.clone();
        let format_clone = Some(format.extension.clone());
        let response_sender_clone = response_sender.clone();
        tokio::spawn(async move {
            Self::write_task(
                &root_clone,
                &last_write_clone,
                &path_clone,
                format_clone.as_deref(),
                &mut write_request_receiver,
                &response_sender_clone,
            )
            .await
        });

        let id_clone = id.clone();
        let root_clone = root.clone();
        let addresses_clone = addresses.clone();
        let compile_sender_clone = compile_request_sender.clone();
        let write_sender_clone = write_request_sender.clone();
        let response_sender_clone = response_sender.clone();
        tokio::spawn(async move {
            Self::patch_task(
                &id_clone,
                &root_clone,
                &addresses_clone,
                &compile_sender_clone,
                &write_sender_clone,
                &mut patch_request_receiver,
                &response_sender_clone,
            )
            .await
        });

        let id_clone = id.clone();
        let path_clone = path.clone();
        let project_clone = project.clone();
        let root_clone = root.clone();
        let addresses_clone = addresses.clone();
        let graph_clone = graph.clone();
        let patch_sender_clone = patch_request_sender.clone();
        let execute_sender_clone = execute_request_sender.clone();
        let write_sender_clone = write_request_sender.clone();
        let response_sender_clone = response_sender.clone();
        tokio::spawn(async move {
            Self::compile_task(
                &id_clone,
                &path_clone,
                &project_clone,
                &root_clone,
                &addresses_clone,
                &graph_clone,
                &patch_sender_clone,
                &execute_sender_clone,
                &write_sender_clone,
                &mut compile_request_receiver,
                &response_sender_clone,
            )
            .await
        });

        let id_clone = id.clone();
        let path_clone = path.clone();
        let project_clone = project.clone();
        let root_clone = root.clone();
        let addresses_clone = addresses.clone();
        let graph_clone = graph.clone();
        let kernels_clone = kernels.clone();
        let patch_sender_clone = patch_request_sender.clone();
        tokio::spawn(async move {
            Self::execute_task(
                &id_clone,
                &path_clone,
                &project_clone,
                &root_clone,
                &addresses_clone,
                &graph_clone,
                &kernels_clone,
                &patch_sender_clone,
                &write_request_sender,
                &mut cancel_request_receiver,
                &mut execute_request_receiver,
                &response_sender,
            )
            .await
        });

        Document {
            id,
            path,
            project,
            temporary,
            name,
            format,
            previewable,

            status: DocumentStatus::Synced,
            last_write,
            content: Default::default(),

            root,
            addresses,
            graph,
            kernels,

            relations: Default::default(),
            subscriptions: Default::default(),

            patch_request_sender,
            compile_request_sender,
            execute_request_sender,
            cancel_request_sender,
            response_receiver,
        }
    }

    /// Create a representation of the document
    ///
    /// Used to represent the document in events and as the return value of functions without
    /// to provide properties such as `path` and `status` without cloning things such as
    /// its `kernels`.
    ///
    /// TODO: This function needs to be factored out of existence or create a lighter weight
    /// repr / summary of a document for serialization.
    pub fn repr(&self) -> Self {
        Self {
            id: self.id.clone(),
            path: self.path.clone(),
            project: self.project.clone(),
            temporary: self.temporary,
            status: self.status.clone(),
            name: self.name.clone(),
            format: self.format.clone(),
            previewable: self.previewable,
            addresses: self.addresses.clone(),
            graph: self.graph.clone(),
            subscriptions: self.subscriptions.clone(),
            last_write: self.last_write.clone(),

            content: Default::default(),
            kernels: Default::default(),
            relations: Default::default(),

            root: Arc::new(RwLock::new(Node::Article(Article::default()))),

            patch_request_sender: self.patch_request_sender.clone(),
            compile_request_sender: self.compile_request_sender.clone(),
            execute_request_sender: self.execute_request_sender.clone(),
            cancel_request_sender: self.cancel_request_sender.clone(),
            response_receiver: self.response_receiver.resubscribe(),
        }
    }

    /// Create a new document, optionally with content.
    pub async fn create<P: AsRef<Path>>(
        path: Option<P>,
        content: Option<String>,
        format: Option<String>,
    ) -> Result<Document> {
        let path = path.map(|path| PathBuf::from(path.as_ref()));

        let mut document = Document::new(path, format);
        if let Some(content) = content {
            document.load(content, None).await?;
        }

        Ok(document)
    }

    /// Open a document from an existing file.
    ///
    /// # Arguments
    ///
    /// - `path`: The path of the file to create the document from
    ///
    /// - `format`: The format of the document. If `None` will be inferred from
    ///             the path's file extension.
    /// TODO: add project: Option<PathBuf> so that project can be explictly set
    #[tracing::instrument(skip(path))]
    pub async fn open<P: AsRef<Path>>(path: P, format: Option<String>) -> Result<Document> {
        let path = PathBuf::from(path.as_ref());

        let mut document = Document::new(Some(path.clone()), format);
        if let Err(error) = document.read(true).await {
            tracing::warn!("While reading document `{}`: {}", path.display(), error)
        };

        Ok(document)
    }

    /// Alter properties of the document
    ///
    /// # Arguments
    ///
    /// - `path`: The path of document's file
    ///
    /// - `format`: The format of the document. If `None` will be inferred from
    ///             the path's file extension.
    #[tracing::instrument(skip(self, path))]
    pub async fn alter<P: AsRef<Path>>(
        &mut self,
        path: Option<P>,
        format: Option<String>,
    ) -> Result<()> {
        if let Some(path) = &path {
            let path = path.as_ref().canonicalize()?;

            if path.is_dir() {
                bail!("Can not open a folder as a document; maybe try opening it as a project instead.")
            }

            self.project = path
                .parent()
                .expect("Unable to get path parent")
                .to_path_buf();

            self.name = path
                .file_name()
                .map(|os_str| os_str.to_string_lossy())
                .unwrap_or_else(|| "Untitled".into())
                .into();

            self.path = path;
            self.temporary = false;
            self.status = DocumentStatus::Unwritten;
        }

        if let Some(format) = format {
            self.format = formats::match_path(&format).spec();
        } else if let Some(path) = path {
            self.format = formats::match_path(&path).spec();
        };

        self.previewable = self.format.preview;

        // Given that the `format` may have changed, it is necessary
        // to update the `root` of the document
        self.update(true).await?;

        Ok(())
    }

    /// Read the document from the file system, update it and return its content.
    ///
    /// # Arguments
    ///
    /// - `force_load`: if `false` then if the file is empty, or is the same as the existing
    ///                 content then do not load the content into the document
    ///
    /// Using `force_load: false` is recommended when calling this function in response to
    /// file modification events as writes in quick succession can cause the file to be momentarily
    /// empty when read.
    ///
    /// Sets `status` to `Synced`. For binary files, does not actually read the content
    /// but will update the document nonetheless (possibly delegating the actual read
    /// to a binary or plugin)
    #[tracing::instrument(skip(self))]
    pub async fn read(&mut self, force_load: bool) -> Result<String> {
        let content = if !self.format.binary {
            let content = fs::read_to_string(&self.path)?;
            if force_load || (!content.is_empty() && content != self.content) {
                self.load(content.clone(), None).await?;
            }
            content
        } else {
            self.update(true).await?;
            "".to_string()
        };
        self.status = DocumentStatus::Synced;
        Ok(content)
    }

    /// Write the document to the file system, optionally load new `content`
    /// and set `format` before doing so.
    ///
    /// # Arguments
    ///
    /// - `content`: the content to load into the document
    /// - `format`: the format of the content; if not supplied assumed to be
    ///    the document's existing format.
    ///
    /// Sets `status` to `Synced`.
    #[tracing::instrument(skip(self, content))]
    pub async fn write(&mut self, content: Option<String>, format: Option<String>) -> Result<()> {
        if let Some(content) = content {
            self.load(content, format.clone()).await?;
        }

        let content_to_write = if let Some(input_format) = format.as_ref() {
            let input_format = formats::match_path(&input_format).spec();
            if input_format != self.format {
                self.dump(None, None).await?
            } else {
                self.content.clone()
            }
        } else {
            self.content.clone()
        };

        fs::write(&self.path, content_to_write.as_bytes())?;
        self.status = DocumentStatus::Synced;
        *self.last_write.write().await = Instant::now();

        Ok(())
    }

    /// Write the document to the file system, as an another file, possibly in
    /// another format.
    ///
    /// # Arguments
    ///
    /// - `path`: the path for the new file.
    /// - `format`: the format to dump the content as; if not supplied assumed to be
    ///    the document's existing format.
    /// - `theme`: theme to apply to the new document (HTML and PDF only).
    ///
    /// Note: this does not change the `path`, `format` or `status` of the current
    /// document.
    #[tracing::instrument(skip(self, path))]
    pub async fn write_as<P: AsRef<Path>>(
        &self,
        path: P,
        format: Option<String>,
        theme: Option<String>,
    ) -> Result<()> {
        let path = path.as_ref();

        let format = format.unwrap_or_else(|| {
            path.extension().map_or_else(
                || self.format.extension.clone(),
                |ext| ext.to_string_lossy().to_string(),
            )
        });

        let mut options = codecs::EncodeOptions {
            standalone: true,
            theme,
            ..Default::default()
        };

        let root = &*self.root.read().await;
        codecs::to_path(root, path, Some(&format), Some(options)).await?;

        Ok(())
    }

    /// A background task to write the document to its path on request
    ///
    /// # Arguments
    ///
    /// - `root`: The root [`Node`] to write (will be read locked)
    ///
    /// - `path`: The filesystem path to write to
    ///
    /// - `format`: The format to write (defaults to the path extension)
    ///
    /// - `request_receiver`: The channel to receive [`WriteRequest`]s on
    ///
    /// - `response_sender`: The channel to send a [`Response`] on when each request if fulfilled
    async fn write_task(
        root: &Arc<RwLock<Node>>,
        last_write: &Arc<RwLock<Instant>>,
        path: &Path,
        format: Option<&str>,
        request_receiver: &mut mpsc::UnboundedReceiver<WriteRequest>,
        response_sender: &broadcast::Sender<Response>,
    ) {
        let duration = Duration::from_millis(1000);
        let mut write = false;
        loop {
            match tokio::time::timeout(duration, request_receiver.recv()).await {
                // Request received: record and continue to wait for timeout
                Ok(Some(request)) => {
                    write = true;
                    if !request.now {
                        continue;
                    }
                }
                // Sender dropped: end of task
                Ok(None) => break,
                // Timeout so do the following with the last unhandled request, if any
                Err(..) => {}
            };

            if write {
                tracing::trace!("Writing document to `{}`", path.display());
                if let Err(error) =
                    codecs::to_path(root.read().await.deref(), path, format, None).await
                {
                    tracing::error!("While writing to `{}`: {}", path.display(), error);
                }

                *last_write.write().await = Instant::now();
                write = false;
            }
        }
    }

    /// Dump the document's content to a string in its current, or
    /// alternative, format.
    ///
    /// # Arguments
    ///
    /// - `format`: the format to dump the content as; if not supplied assumed to be
    ///    the document's existing format.
    ///
    /// - `node_id`: the id of the node within the document to dump
    #[tracing::instrument(skip(self))]
    pub async fn dump(&self, format: Option<String>, node_id: Option<String>) -> Result<String> {
        let format = match format {
            Some(format) => format,
            None => return Ok(self.content.clone()),
        };

        let root = &*self.root.read().await;
        if let Some(node_id) = node_id {
            let address = self.addresses.read().await.get(&node_id).cloned();
            let pointer = resolve(root, address, Some(node_id))?;
            let node = pointer.to_node()?;
            codecs::to_string(&node, &format, None).await
        } else {
            codecs::to_string(root, &format, None).await
        }
    }

    /// Load content into the document
    ///
    /// If the format of the new content is different to the document's format
    /// then the content will be converted to the document's format.
    ///
    /// # Arguments
    ///
    /// - `content`: the content to load into the document
    /// - `format`: the format of the content; if not supplied assumed to be
    ///    the document's existing format.
    #[tracing::instrument(skip(self, content))]
    pub async fn load(&mut self, content: String, format: Option<String>) -> Result<()> {
        let mut decode_content = true;
        if let Some(format) = format {
            let other_format = formats::match_path(&format).spec();
            if other_format != self.format {
                let node = codecs::from_str(&content, &other_format.extension, None).await?;
                if !self.format.binary {
                    self.content = codecs::to_string(&node, &self.format.extension, None).await?;
                }
                let mut root = &mut *self.root.write().await;
                *root = node;
                decode_content = false;
            } else {
                self.content = content;
            }
        } else {
            self.content = content;
        };
        self.status = DocumentStatus::Unwritten;

        self.update(decode_content).await
    }

    /// Generate a [`Patch`] describing the operations needed to modify this
    /// document so that it is equal to another.
    #[tracing::instrument(skip(self, other))]
    pub async fn diff(&self, other: &Document) -> Result<Patch> {
        let me = &*self.root.read().await;
        let other = &*other.root.read().await;
        let patch = diff(me, other);
        Ok(patch)
    }

    /// Merge changes from two or more derived version into this document.
    ///
    /// See documentation on the [`merge`] function for how any conflicts
    /// are resolved.
    #[tracing::instrument(skip(self, deriveds))]
    pub async fn merge(&mut self, deriveds: &[Document]) -> Result<()> {
        let mut guard = self.root.write().await;

        // Need to store `let` bindings to read guards before dereferencing them
        let mut guards = Vec::new();
        for derived in deriveds {
            let guard = derived.root.read().await;
            guards.push(guard)
        }
        let others: Vec<&Node> = guards.iter().map(|guard| guard.deref()).collect();

        // Do the merge into root
        merge(&mut *guard, &others);

        // TODO updating of *content from root* and publishing of events etc needs to be sorted out
        if !self.format.binary {
            self.content = codecs::to_string(&*guard, &self.format.extension, None).await?;
        }

        // Drop root guard to allow update
        drop(guard);

        self.update(false).await?;

        Ok(())
    }

    /// A background task to patch the root node of the document on request
    ///
    /// Use an unbounded channel for sending patches, so that sending threads never
    /// block (if there are lots of patches) and thereby hold on to locks causing a
    /// deadlock.
    ///
    /// # Arguments
    ///
    /// - `id`: The id of the document (used in the published event topic)
    ///
    /// - `root`: The root [`Node`] to apply the patch to (will be write locked)
    ///
    /// - `addresses`: The [`AddressMap`] to use to locate nodes within the root
    ///                node (will be read locked)
    ///
    /// - `compile_sender`: The channel to send any [`CompileRequest`]s after a patch is applied
    ///
    /// - `write_sender`: The channel to send any [`WriteRequest`]s after a patch is applied
    ///
    /// - `request_receiver`: The channel to receive [`PatchRequest`]s on
    ///
    /// - `response_sender`: The channel to send a [`Response`] on when each request if fulfilled
    async fn patch_task(
        id: &str,
        root: &Arc<RwLock<Node>>,
        addresses: &Arc<RwLock<AddressMap>>,
        compile_sender: &mpsc::Sender<CompileRequest>,
        write_sender: &mpsc::UnboundedSender<WriteRequest>,
        request_receiver: &mut mpsc::UnboundedReceiver<PatchRequest>,
        response_sender: &broadcast::Sender<Response>,
    ) {
        while let Some(request) = request_receiver.recv().await {
            tracing::trace!("Patching document `{}` for request `{}`", &id, request.id);

            let mut patch = request.patch;
            let start = patch.target.clone();

            // If the patch is empty then continue early rather than obtain locks etc
            if patch.is_empty() {
                continue;
            }

            // Block for minimal longevity locks
            {
                let root = &mut *root.write().await;
                let addresses = &*addresses.read().await;

                // If the patch has a `target` but no `address` then use `address_map` to populate the address
                // for faster patch application.
                if let (None, Some(node_id)) = (&patch.address, &patch.target) {
                    if let Some(address) = addresses.get(node_id) {
                        patch.address = Some(address.clone());
                    }
                }

                // Apply the patch to the root node
                apply(root, &patch);

                // Pre-publish the patch
                patch.prepublish(root);
            }

            // Publish the patch
            publish(
                &["documents:", id, ":patched"].concat(),
                &DocumentEvent {
                    type_: DocumentEventType::Patched,
                    patch: Some(patch),
                    // TODO: The following are made `None` to keep the size of the event smaller but really
                    // should be removed from the event (`Document:new()` is particularly wasteful of compute)
                    document: Document::new(None, None),
                    content: None,
                    format: None,
                },
            );

            // Possibly compile, execute, and/or write
            if request.compile {
                tracing::trace!(
                    "Sending compile request for document `{}` for patch request `{}`",
                    &id,
                    request.id
                );
                if let Err(error) = compile_sender
                    .send(CompileRequest::new(request.execute, request.write, start))
                    .await
                {
                    tracing::error!(
                        "While sending compile request for document `{}`: {}",
                        id,
                        error
                    );
                }
            } else if request.write {
                tracing::trace!(
                    "Sending write request for document `{}` for patch request `{}`",
                    &id,
                    request.id
                );
                if let Err(error) = write_sender.send(WriteRequest::new(false)) {
                    tracing::error!(
                        "While sending write request for document `{}`: {}",
                        id,
                        error
                    );
                }
            }

            // Send response
            if let Err(error) = response_sender.send(Response::PatchResponse(request.id)) {
                tracing::debug!(
                    "While sending patch response for document `{}`: {}",
                    id,
                    error
                );
            }
        }
    }

    /// Request that a [`Patch`] be applied to the root node of the document
    ///
    /// # Arguments
    ///
    /// - `patch`: The patch to apply
    ///
    /// - `compile`: Should the document be compiled after the patch is applied?
    ///
    /// - `execute`: Should the document be executed after the patch is applied?
    ///              If the patch as a `target` then the document will be executed from that
    ///              node, otherwise the entire document will be executed.
    /// - `write`: Should the document be written after the patch is applied?
    #[tracing::instrument(skip(self, patch))]
    pub async fn patch_request(
        &self,
        patch: Patch,
        compile: bool,
        execute: bool,
        write: bool,
    ) -> Result<RequestId> {
        tracing::debug!("Sending patch request for document `{}`", self.id);

        let request = PatchRequest::new(patch, compile, execute, write);
        let request_id = request.id.clone();
        if let Err(error) = self.patch_request_sender.send(request) {
            bail!(
                "When sending patch request for document `{}`: {}",
                self.id,
                error
            )
        };

        Ok(request_id)
    }

    /// A background task to compile the root node of the document on request
    ///
    /// # Arguments
    ///
    /// - `id`: The id of the document
    ///
    /// - `path`: The path of the document to be compiled
    ///
    /// - `project`: The project of the document to be compiled
    ///
    /// - `root`: The root [`Node`] to apply the compilation patch to
    ///
    /// - `addresses`: The [`AddressMap`] to be updated
    ///
    /// - `graph`:  The [`Graph`] to be updated
    ///
    /// - `patch_sender`: A [`PatchRequest`] channel to send patches describing the changes to
    ///                   compiled nodes
    ///
    /// - `execute_sender`: An [`ExecuteRequest`] channel to send any requests to execute the
    ///                     document after it has been compiled
    ///
    /// - `write_sender`: The channel to send any [`WriteRequest`]s after a patch is applied
    ///
    /// - `request_receiver`: The channel to receive [`CompileRequest`]s on
    ///
    /// - `response_sender`: The channel to send a [`Response`] on when each request if fulfilled
    #[allow(clippy::too_many_arguments)]
    pub async fn compile_task(
        id: &str,
        path: &Path,
        project: &Path,
        root: &Arc<RwLock<Node>>,
        addresses: &Arc<RwLock<AddressMap>>,
        graph: &Arc<RwLock<Graph>>,
        patch_sender: &mpsc::UnboundedSender<PatchRequest>,
        execute_sender: &mpsc::Sender<ExecuteRequest>,
        write_sender: &mpsc::UnboundedSender<WriteRequest>,
        request_receiver: &mut mpsc::Receiver<CompileRequest>,
        response_sender: &broadcast::Sender<Response>,
    ) {
        let duration = Duration::from_millis(300);
        let mut request_ids = Vec::new();
        let mut execute = false;
        let mut write = false;
        loop {
            match tokio::time::timeout(duration, request_receiver.recv()).await {
                // Compile request received, so record it and continue to wait for timeout
                Ok(Some(request)) => {
                    request_ids.push(request.id);
                    execute |= request.execute;
                    write |= request.write;
                    continue;
                }
                // Sender dropped, end of task
                Ok(None) => break,
                // Timeout so do the following with the last unhandled request, if any
                Err(..) => {}
            };

            if request_ids.is_empty() {
                continue;
            }

            let request_ids_display = || {
                request_ids
                    .as_slice()
                    .iter()
                    .map(|id| id.to_string())
                    .join(",")
            };
            tracing::trace!(
                "Compiling document `{}` for requests `{}`",
                id,
                request_ids_display()
            );

            // Compile the root node
            match compile(path, project, root, patch_sender).await {
                Ok((new_addresses, new_graph)) => {
                    *addresses.write().await = new_addresses;
                    *graph.write().await = new_graph;
                }
                Err(error) => tracing::error!("While compiling document `{}`: {}", id, error),
            }

            // Possibly execute and/or write
            if execute {
                tracing::trace!(
                    "Sending execute request for document `{}` for compile requests `{}`",
                    &id,
                    request_ids_display()
                );
                if let Err(error) = execute_sender
                    .send(ExecuteRequest::new(write, None, None, None))
                    .await
                {
                    tracing::error!(
                        "While sending execute request for document `{}`: {}",
                        id,
                        error
                    );
                }
            } else if write {
                tracing::trace!(
                    "Sending write request for document `{}` for compile requests `{}`",
                    &id,
                    request_ids_display()
                );
                if let Err(error) = write_sender.send(WriteRequest::new(false)) {
                    tracing::error!(
                        "While sending write request for document `{}`: {}",
                        id,
                        error
                    );
                }
            }

            // Send responses for each request
            for request_id in &request_ids {
                tracing::trace!(
                    "Sending compile response for document `{}` for request `{}`",
                    id,
                    request_id
                );
                if let Err(error) =
                    response_sender.send(Response::CompileResponse(request_id.clone()))
                {
                    tracing::debug!(
                        "While sending compile response for document `{}`: {}",
                        id,
                        error
                    );
                }
            }

            request_ids.clear();
            execute = false;
            write = false;
        }
    }

    /// Request that the the document be compiled
    #[tracing::instrument(skip(self))]
    pub async fn compile_request(
        &self,
        execute: bool,
        write: bool,
        start: Option<String>,
    ) -> Result<RequestId> {
        tracing::debug!("Sending compile request for document `{}`", self.id);

        let request = CompileRequest::new(execute, write, start);
        let request_id = request.id.clone();
        if let Err(error) = self.compile_request_sender.send(request).await {
            bail!(
                "When sending compile request for document `{}`: {}",
                self.id,
                error
            )
        };

        Ok(request_id)
    }

    /// Compile the document
    ///
    /// This method is the same as `compile_request` but will wait for the compilation to finish
    /// before returning. This is useful in some circumstances, such as ensuring the document
    /// is compiled before HTML is encoded for it on initial opening.
    #[tracing::instrument(skip(self))]
    pub async fn compile(
        &mut self,
        execute: bool,
        write: bool,
        start: Option<String>,
    ) -> Result<()> {
        let request_id = self.compile_request(execute, write, start).await?;

        tracing::trace!(
            "Waiting for compile response for document `{}` for request `{}`",
            self.id,
            request_id
        );
        while let Ok(response) = self.response_receiver.recv().await {
            if let Response::CompileResponse(id) = response {
                if id == request_id {
                    tracing::trace!(
                        "Received compile response for document `{}` for request `{}`",
                        self.id,
                        request_id
                    );
                    break;
                }
            }
        }

        Ok(())
    }

    /// A background task to execute the root node of the document on request
    ///
    /// # Arguments
    ///
    /// - `id`: The id of the document
    ///
    /// - `path`: The path of the document to be compiled
    ///
    /// - `project`: The project of the document to be compiled
    ///
    /// - `root`: The root [`Node`] to apply the compilation patch to
    ///
    /// - `addresses`: The [`AddressMap`] to be updated
    ///
    /// - `graph`:  The [`Graph`] to be updated
    ///
    /// - `kernel_space`:  The [`KernelSpace`] to use for execution
    ///
    /// - `patch_sender`: A [`PatchRequest`] channel sender to send patches describing the changes to
    ///                   executed nodes
    ///
    /// - `write_sender`: The channel to send any [`WriteRequest`]s on
    ///
    /// - `cancel_receiver`: The channel to receive [`CancelRequest`]s on
    ///
    /// - `request_receiver`: The channel to receive [`ExecuteRequest`]s on
    ///
    /// - `response_sender`: The channel to send a [`Response`] on when each request if fulfilled
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_task(
        id: &str,
        path: &Path,
        project: &Path,
        root: &Arc<RwLock<Node>>,
        addresses: &Arc<RwLock<AddressMap>>,
        graph: &Arc<RwLock<Graph>>,
        kernel_space: &Arc<RwLock<KernelSpace>>,
        patch_sender: &mpsc::UnboundedSender<PatchRequest>,
        write_sender: &mpsc::UnboundedSender<WriteRequest>,
        cancel_receiver: &mut mpsc::Receiver<CancelRequest>,
        request_receiver: &mut mpsc::Receiver<ExecuteRequest>,
        response_sender: &broadcast::Sender<Response>,
    ) {
        while let Some(request) = request_receiver.recv().await {
            tracing::trace!("Executing document `{}` for request `{}`", &id, request.id);

            // Resolve options
            let start = request
                .start
                .map(|node_id| resources::code(path, &node_id, "", None));
            let ordering = request
                .ordering
                .unwrap_or_else(PlanOptions::default_ordering);
            let max_concurrency = request
                .max_concurrency
                .unwrap_or_else(PlanOptions::default_max_concurrency);
            let options = PlanOptions {
                ordering,
                max_concurrency,
            };

            // Generate the execution plan
            let plan = match graph.read().await.plan(start, None, Some(options)).await {
                Ok(plan) => plan,
                Err(error) => {
                    tracing::error!("While generating execution plan: {}", error);
                    continue;
                }
            };

            // Execute the plan on the root node
            execute(
                &plan,
                root,
                addresses,
                kernel_space,
                patch_sender,
                cancel_receiver,
            )
            .await;

            if request.write {
                tracing::trace!(
                    "Sending write request for document `{}` for request `{}`",
                    &id,
                    request.id
                );
                if let Err(error) = write_sender.send(WriteRequest::new(false)) {
                    tracing::error!(
                        "While sending write request for document `{}`: {}",
                        id,
                        error
                    );
                }
            }

            // Send response
            if let Err(error) = response_sender.send(Response::ExecuteResponse(request.id.clone()))
            {
                tracing::debug!(
                    "While sending execute response for document `{}`: {}",
                    id,
                    error
                );
            }
        }
    }

    /// Request that the document be executed
    #[tracing::instrument(skip(self))]
    pub async fn execute_request(
        &self,
        write: bool,
        start: Option<String>,
        ordering: Option<PlanOrdering>,
        max_concurrency: Option<usize>,
    ) -> Result<RequestId> {
        tracing::debug!("Sending execute request for document `{}`", self.id);

        let request = ExecuteRequest::new(write, start, ordering, max_concurrency);
        let request_id = request.id.clone();
        if let Err(error) = self.execute_request_sender.send(request).await {
            bail!(
                "When sending execute request for document `{}`: {}",
                self.id,
                error
            )
        };

        Ok(request_id)
    }

    /// Execute the document
    ///
    /// This method is the same as `execute_request` but will wait for the execution to finish
    /// before returning. This is useful in some circumstances, such as ensuring the document
    /// is executed before saving it to file.
    #[tracing::instrument(skip(self))]
    pub async fn execute(
        &mut self,
        write: bool,
        start: Option<String>,
        ordering: Option<PlanOrdering>,
        max_concurrency: Option<usize>,
    ) -> Result<()> {
        // Execute the document
        let request_id = self
            .execute_request(false, start, ordering, max_concurrency)
            .await?;

        // Wait for execution to finish
        tracing::trace!(
            "Waiting for execute response for document `{}` for request `{}`",
            self.id,
            request_id
        );
        while let Ok(response) = self.response_receiver.recv().await {
            if let Response::ExecuteResponse(id) = response {
                if id == request_id {
                    tracing::trace!(
                        "Received execute response for document `{}` for request `{}`",
                        self.id,
                        request_id
                    );
                    break;
                }
            }
        }

        // Recompile the document to ensure properties such as `code_dependencies` reflect the
        // new state of the document, and write if necessary
        self.compile(false, write, None).await?;

        Ok(())
    }

    /// React to a change in a file path
    ///
    /// If the path corresponds to a `File` resource in the document's graph then re-compile,
    /// re-execute, and write the document.
    async fn react(&mut self, path: &Path) {
        if let Ok(resource_info) = self
            .graph
            .read()
            .await
            .find_resource_info(&resources::file(path))
        {
            tracing::trace!(
                "Compiling, executing and writing document `{}` because file changed: {}",
                self.id,
                path.display()
            );
            if let Err(error) = self.compile_request(true, true, None).await {
                tracing::error!(
                    "When sending compile request for document `{}`: {}",
                    self.id,
                    error
                );
            }
        }
    }

    /// Get the parameters of the document
    pub async fn params(&mut self) -> Result<IndexMap<String, (String, Address, Parameter)>> {
        // Compile the document to ensure its `addresses` are up to date
        self.compile(false, false, None).await?;

        // Collect parameters from addresses
        let addresses = self.addresses.read().await;
        let root = &*self.root.read().await;
        let params = addresses
            .iter()
            .filter_map(|(id, address)| {
                if let Ok(pointer) = resolve(root, Some(address.clone()), Some(id.clone())) {
                    if let Some(InlineContent::Parameter(param)) = pointer.as_inline() {
                        return Some((
                            param.name.clone(),
                            (id.clone(), address.clone(), param.clone()),
                        ));
                    }
                }
                None
            })
            .collect();

        Ok(params)
    }

    /// Call the document with a set of parameters
    pub async fn call(&mut self, args: HashMap<String, String>) -> Result<()> {
        // Get the document's params
        let mut params = self.params().await?;

        // Attempt to set params based on args
        {
            let root = &mut *self.root.write().await;
            for (name, value) in args {
                if let Some((id, address, param)) = params.remove(&name) {
                    if let Some(validator) = param.validator.as_deref() {
                        match validator.parse(&value) {
                            Ok(value) => {
                                if let Ok(mut pointer) = resolve_mut(root, Some(address), Some(id))
                                {
                                    if let Some(InlineContent::Parameter(param)) =
                                        pointer.as_inline_mut()
                                    {
                                        param.value = Some(Box::new(value));
                                    }
                                }
                            }
                            Err(error) => bail!(
                                "While attempting to set document parameter `{}`: {}",
                                name,
                                error
                            ),
                        }
                    }
                } else {
                    bail!("Document does not have a parameter named `{}`", name)
                }
            }
        }

        // Now execute the document
        self.execute(false, None, None, None).await?;

        Ok(())
    }

    /// Cancel the execution of the document
    ///
    /// # Arguments
    ///
    /// - `start`: The node whose execution should be cancelled.
    ///
    /// - `scope`: The scope of the cancellation (the `Single` node identified
    ///            by `start` or `All` nodes in the current plan).
    #[tracing::instrument(skip(self))]
    pub async fn cancel(
        &self,
        start: Option<String>,
        scope: Option<PlanScope>,
    ) -> Result<RequestId> {
        tracing::debug!("Cancelling execution of document `{}`", self.id);

        let request = CancelRequest::new(start, scope);
        let request_id = request.id.clone();
        self.cancel_request_sender.send(request).await.or_else(|_| {
            bail!(
                "When sending cancel request for document `{}`: the receiver has dropped",
                self.id
            )
        });

        Ok(request_id)
    }

    /// Restart a kernel (or all kernels) in the document's kernel space
    ///
    /// Cancels any execution plan that is running, destroy the document's
    /// existing kernel, and create's a new one
    #[tracing::instrument(skip(self))]
    pub async fn restart(&self, kernel_id: Option<String>) -> Result<()> {
        tracing::debug!("Restarting kernel/s for document `{}`", self.id);

        self.cancel(None, Some(PlanScope::All)).await;

        let kernels = &*self.kernels.write().await;
        kernels.restart(kernel_id).await?;

        Ok(())
    }

    /// Get the list of kernels in the document's kernel space
    pub async fn kernels(&self) -> KernelInfos {
        let kernel_space = &*self.kernels.read().await;
        kernel_space.kernels().await
    }

    /// Get the list of symbols in the document's kernel space
    pub async fn symbols(&self) -> KernelSymbols {
        let kernel_space = &*self.kernels.read().await;
        kernel_space.symbols().await
    }

    /// Update the `root` (and associated properties) of the document and publish updated encodings
    ///
    /// Publishes `encoded:` events for each of the formats subscribed to.
    /// Error results from this function (e.g. compile errors)
    /// should generally not be bubbled up.
    ///
    /// # Arguments
    ///
    /// - `decode_content`: Should the current content of the be decoded?. This
    ///                     is an optimization for situations where the `root` has
    ///                     just been decoded from the current `content`.
    #[tracing::instrument(skip(self))]
    async fn update(&mut self, decode_content: bool) -> Result<()> {
        tracing::debug!(
            "Updating document `{}` at `{}`",
            self.id,
            self.path.display()
        );

        // Decode the binary file or, in-memory content into the `root` node
        // of the document
        let format = &self.format.extension;
        let mut root = if self.format.binary {
            if self.path.exists() {
                tracing::debug!("Decoding document `{}` root from path", self.id);
                codecs::from_path(&self.path, Some(format), None).await?
            } else {
                self.root.read().await.clone()
            }
        } else if !self.content.is_empty() {
            if decode_content {
                tracing::debug!("Decoding document `{}` root from content", self.id);
                codecs::from_str(&self.content, format, None).await?
            } else {
                self.root.read().await.clone()
            }
        } else {
            tracing::debug!("Setting document `{}` root to empty article", self.id);
            Node::Article(Article::default())
        };

        // Reshape the `root`
        // TODO: Pass user options for reshaping through
        reshape(&mut root, None)?;

        // Determine if the document is preview-able, based on the type of the root
        // This list of types should be updated as HTML encoding is implemented for each.
        self.previewable = matches!(
            root,
            Node::Article(..)
                | Node::ImageObject(..)
                | Node::AudioObject(..)
                | Node::VideoObject(..)
        );

        // Set the root and compile
        // TODO: Reconsider this in refactoring of alternative format representations of docs
        *self.root.write().await = root;
        self.compile(false, false, None).await?;

        // Publish any events for which there are subscriptions (this will probably go elsewhere)
        for subscription in self.subscriptions.keys() {
            // Encode the `root` into each of the formats for which there are subscriptions
            if let Some(format) = subscription.strip_prefix("encoded:") {
                tracing::debug!("Encoding document `{}` to format `{}`", self.id, format);
                match codecs::to_string(&*self.root.read().await, format, None).await {
                    Ok(content) => {
                        self.publish(
                            DocumentEventType::Encoded,
                            Some(content),
                            Some(format.into()),
                        );
                    }
                    Err(error) => {
                        tracing::warn!("Unable to encode to format `{}`: {}", format, error)
                    }
                }
            }
        }

        Ok(())
    }

    /// Detect entities within the document
    pub async fn detect(&self) -> Result<Vec<DetectItem>> {
        let root = &*self.root.read().await;
        providers::detect(root).await
    }

    /// Generate a topic string for the document
    pub fn topic(&self, subtopic: &str) -> String {
        ["documents:", &self.id, ":", subtopic].concat()
    }

    /// Subscribe a client to one of the document's topics
    pub fn subscribe(&mut self, topic: &str, client: &str) -> String {
        match self.subscriptions.entry(topic.into()) {
            Entry::Occupied(mut occupied) => {
                occupied.get_mut().insert(client.into());
            }
            Entry::Vacant(vacant) => {
                vacant.insert(hashset! {client.into()});
            }
        }
        self.topic(topic)
    }

    /// Unsubscribe a client from one of the document's topics
    pub fn unsubscribe(&mut self, topic: &str, client: &str) -> String {
        if let Entry::Occupied(mut occupied) = self.subscriptions.entry(topic.to_string()) {
            let subscribers = occupied.get_mut();
            subscribers.remove(client);
            if subscribers.is_empty() {
                occupied.remove();
            }
        }
        self.topic(topic)
    }

    /// Get the number of subscribers to one of the document's topics
    fn subscribers(&self, topic: &str) -> usize {
        if let Some(subscriptions) = self.subscriptions.get(topic) {
            subscriptions.len()
        } else {
            0
        }
    }

    /// Publish an event for this document
    fn publish(&self, type_: DocumentEventType, content: Option<String>, format: Option<String>) {
        let format = format.map(|name| formats::match_name(&name).spec());

        let subtopic = match type_ {
            DocumentEventType::Encoded => format!(
                "encoded:{}",
                format
                    .clone()
                    .map_or_else(|| "undef".to_string(), |format| format.extension)
            ),
            _ => type_.to_string(),
        };

        publish(
            &self.topic(&subtopic),
            &DocumentEvent {
                type_,
                document: self.repr(),
                content,
                format,
                patch: None,
            },
        )
    }

    /// Called when the file is removed from the file system
    ///
    /// Sets `status` to `Deleted` and publishes a `Deleted` event so that,
    /// for example, a document's tab can be updated to indicate it is deleted.
    fn deleted(&mut self, path: PathBuf) {
        tracing::debug!(
            "Deleted event for document `{}` at `{}`",
            self.id,
            path.display()
        );

        self.status = DocumentStatus::Deleted;

        self.publish(DocumentEventType::Deleted, None, None)
    }

    /// Called when the file is renamed
    ///
    /// Changes the `path` and publishes a `Renamed` event so that, for example,
    /// a document's tab can be updated with the new file name.
    #[allow(dead_code)]
    fn renamed(&mut self, from: PathBuf, to: PathBuf) {
        tracing::debug!(
            "Renamed event for document `{}`: `{}` to `{}`",
            self.id,
            from.display(),
            to.display()
        );

        // If the document has been moved out of its project then we need to reassign `project`
        // (to ensure that files in the old project can not be linked to).
        if to.strip_prefix(&self.project).is_err() {
            self.project = match to.parent() {
                Some(path) => path.to_path_buf(),
                None => to.clone(),
            }
        }

        self.path = to;

        self.publish(DocumentEventType::Renamed, None, None)
    }

    const LAST_WRITE_MUTE_MILLIS: u64 = 300;

    /// Called when the file is modified
    ///
    /// Reads the file into `content` and emits a `Modified` event so that the user
    /// can be asked if they want to load the new content into editor, or overwrite with
    /// existing editor content.
    ///
    /// Will ignore any events within a small duration of `write()` being called to avoid
    /// reacting to file modifications initiated by this process
    async fn modified(&mut self, path: PathBuf) {
        if self.last_write.read().await.elapsed()
            < Duration::from_millis(Document::LAST_WRITE_MUTE_MILLIS)
        {
            return;
        }

        tracing::debug!(
            "Modified event for document `{}` at `{}`",
            self.id,
            path.display()
        );

        self.status = DocumentStatus::Unread;

        match self.read(false).await {
            Ok(content) => self.publish(
                DocumentEventType::Modified,
                Some(content),
                Some(self.format.extension.clone()),
            ),
            Err(error) => tracing::error!("While attempting to read modified file: {}", error),
        }
    }
}

#[derive(Debug)]
pub struct DocumentHandler {
    /// The document being handled.
    document: Arc<Mutex<Document>>,

    /// The event handler thread's join handle.
    ///
    /// Held so that when this handler is dropped, the
    /// event handler thread is aborted.
    handler: Option<JoinHandle<()>>,
}

impl Clone for DocumentHandler {
    fn clone(&self) -> Self {
        DocumentHandler {
            document: self.document.clone(),
            handler: None,
        }
    }
}

impl Drop for DocumentHandler {
    fn drop(&mut self) {
        match &self.handler {
            Some(handler) => handler.abort(),
            None => {}
        }
    }
}

impl DocumentHandler {
    /// Create a new document handler.
    ///
    /// # Arguments
    ///
    /// - `document`: The document that this handler is for.
    /// - `watch`: Whether to watch the document (e.g. not for temporary, new files)
    fn new(document: Document, watch: bool) -> DocumentHandler {
        let id = document.id.clone();
        let path = document.path.clone();

        let document = Arc::new(Mutex::new(document));
        let handler = if watch {
            let handler = DocumentHandler::watch(id, path, Arc::clone(&document));
            Some(handler)
        } else {
            None
        };

        DocumentHandler { document, handler }
    }

    const WATCHER_DELAY_MILLIS: u64 = 100;

    /// Watch the document.
    ///
    /// It is necessary to have a file watcher that is separate from a project directory watcher
    /// for documents that are opened independent of a project (a.k.a. orphan documents).
    ///
    /// It is also necessary for this watcher to be on the parent folder of the document
    /// (which, for some documents may be concurrent with the watcher for the project) and to filter
    /// events related to the file. That is necessary because some events are otherwise
    /// not captured e.g. file renames (delete and then create) and file writes by some software
    /// (e.g. LibreOffice deletes and then creates a file instead of just writing it).
    fn watch(id: String, path: PathBuf, document: Arc<Mutex<Document>>) -> JoinHandle<()> {
        let (async_sender, mut async_receiver) = tokio::sync::mpsc::channel(100);

        let path_cloned = path.clone();

        // Standard thread to run blocking sync file watcher
        std::thread::spawn(move || -> Result<()> {
            use notify::{watcher, RecursiveMode, Watcher};

            let (watcher_sender, watcher_receiver) = std::sync::mpsc::channel();
            let mut watcher = watcher(
                watcher_sender,
                Duration::from_millis(DocumentHandler::WATCHER_DELAY_MILLIS),
            )?;
            let parent = path.parent().unwrap_or(&path);
            watcher.watch(&parent, RecursiveMode::NonRecursive)?;

            // Event checking timeout. Can be quite long since only want to check
            // whether we can end this thread.
            let timeout = Duration::from_millis(100);

            let path_string = path.display().to_string();
            let span = tracing::info_span!("document_watch", path = path_string.as_str());
            let _enter = span.enter();
            tracing::trace!(
                "Starting document watcher for '{}' at '{}'",
                id,
                path_string
            );
            loop {
                // Check for an event. Use `recv_timeout` so we don't
                // get stuck here and will do following check that ends this
                // thread if the owning `DocumentHandler` is dropped
                if let Ok(event) = watcher_receiver.recv_timeout(timeout) {
                    if let Err(error) = async_sender.blocking_send(event) {
                        tracing::debug!(
                            "While sending file watch event watcher for document '{}': {}",
                            id,
                            error
                        );
                        break;
                    }
                }
            }
            tracing::trace!("Ending document watcher for '{}' at '{}'", id, path_string);

            // Drop the sync send so that the event handling thread also ends
            drop(async_sender);

            Ok(())
        });

        // Async task to handle events
        tokio::spawn(async move {
            let mut document_path = path_cloned;
            tracing::trace!("Starting document handler");
            while let Some(event) = async_receiver.recv().await {
                match event {
                    DebouncedEvent::Create(path) | DebouncedEvent::Write(path) => {
                        let doc = &mut *document.lock().await;
                        doc.react(&path).await;
                        if path == document_path {
                            doc.modified(path.clone()).await
                        }
                    }
                    DebouncedEvent::Remove(path) => {
                        let doc = &mut *document.lock().await;
                        doc.react(&path).await;
                        if path == document_path {
                            doc.deleted(path)
                        }
                    }
                    DebouncedEvent::Rename(from, to) => {
                        let doc = &mut *document.lock().await;
                        doc.react(&from).await;
                        doc.react(&to).await;
                        if from == document_path {
                            document_path = to.clone();
                            doc.renamed(from, to)
                        }
                    }
                    _ => {}
                }
            }
            // Because we abort this thread, this entry may never get
            // printed (only if the `async_sender` is dropped before this is aborted)
            tracing::trace!("Ending document handler");
        })
    }
}

/// An in-memory store of documents
#[derive(Debug, Default)]
pub struct Documents {
    /// A mapping of file paths to open documents
    registry: Mutex<HashMap<String, DocumentHandler>>,
}

impl Documents {
    /// Create a new documents store
    pub fn new() -> Self {
        Self::default()
    }

    /// List documents that are currently open
    ///
    /// Returns a vector of document paths (relative to the current working directory)
    pub async fn list(&self) -> Result<Vec<String>> {
        let cwd = std::env::current_dir()?;
        let mut paths = Vec::new();
        for document in self.registry.lock().await.values() {
            let path = &document.document.lock().await.path;
            let path = match pathdiff::diff_paths(path, &cwd) {
                Some(path) => path,
                None => path.clone(),
            };
            let path = path.display().to_string();
            paths.push(path);
        }
        Ok(paths)
    }

    /// Create a new document
    pub async fn create<P: AsRef<Path>>(
        &self,
        path: Option<P>,
        content: Option<String>,
        format: Option<String>,
    ) -> Result<Document> {
        let document = Document::create(path, content, format).await?;
        let document_id = document.id.clone();
        let document_repr = document.repr();
        let handler = DocumentHandler::new(document, false);
        self.registry.lock().await.insert(document_id, handler);

        Ok(document_repr)
    }

    /// Open a document
    ///
    /// # Arguments
    ///
    /// - `path`: The path of the document to open
    /// - `format`: The format to open the document as (inferred from filename extension if not supplied)
    ///
    /// If the document has already been opened, it will not be re-opened, but rather the existing
    /// in-memory instance will be returned.
    pub async fn open<P: AsRef<Path>>(&self, path: P, format: Option<String>) -> Result<Document> {
        let path = Path::new(path.as_ref()).canonicalize()?;

        for handler in self.registry.lock().await.values() {
            let document = handler.document.lock().await;
            if document.path == path {
                return Ok(document.repr());
            }
        }

        let document = Document::open(path, format).await?;
        let document_id = document.id.clone();
        let document_repr = document.repr();
        let handler = DocumentHandler::new(document, true);
        self.registry.lock().await.insert(document_id, handler);

        Ok(document_repr)
    }

    /// Close a document
    ///
    /// # Arguments
    ///
    /// - `id_or_path`: The id or path of the document to close
    ///
    /// If `id_or_path` matches an existing document `id` then that document will
    /// be closed. Otherwise a search will be done and the first document with a matching
    /// path will be closed.
    pub async fn close<P: AsRef<Path>>(&self, id_or_path: P) -> Result<()> {
        let id_or_path_path = id_or_path.as_ref();
        let id_or_path_string = id_or_path_path.to_string_lossy().to_string();
        let mut id_to_remove = String::new();

        if self.registry.lock().await.contains_key(&id_or_path_string) {
            id_to_remove = id_or_path_string
        } else {
            let path = id_or_path_path.canonicalize()?;
            for handler in self.registry.lock().await.values() {
                let document = handler.document.lock().await;
                if document.path == path {
                    id_to_remove = document.id.clone();
                    break;
                }
            }
        };
        self.registry.lock().await.remove(&id_to_remove);

        Ok(())
    }

    /// Subscribe a client to a topic for a document
    pub async fn subscribe(
        &self,
        id: &str,
        topic: &str,
        client: &str,
    ) -> Result<(Document, String)> {
        let document_lock = self.get(id).await?;
        let mut document_guard = document_lock.lock().await;
        let topic = document_guard.subscribe(topic, client);
        Ok((document_guard.repr(), topic))
    }

    /// Unsubscribe a client from a topic for a document
    pub async fn unsubscribe(
        &self,
        id: &str,
        topic: &str,
        client: &str,
    ) -> Result<(Document, String)> {
        let document_lock = self.get(id).await?;
        let mut document_guard = document_lock.lock().await;
        let topic = document_guard.unsubscribe(topic, client);
        Ok((document_guard.repr(), topic))
    }

    /// Get a document that has previously been opened
    pub async fn get(&self, id: &str) -> Result<Arc<Mutex<Document>>> {
        if let Some(handler) = self.registry.lock().await.get(id) {
            Ok(handler.document.clone())
        } else {
            bail!("No document with id {}", id)
        }
    }
}

/// The global documents store
pub static DOCUMENTS: Lazy<Documents> = Lazy::new(Documents::new);

/// Get JSON Schemas for this module
pub fn schemas() -> Result<serde_json::Value> {
    let schemas = serde_json::Value::Array(vec![
        schemas::generate::<Document>()?,
        schemas::generate::<DocumentEvent>()?,
    ]);
    Ok(schemas)
}

#[cfg(feature = "cli")]
pub mod commands {
    use std::str::FromStr;

    use cli_utils::{
        args::params,
        clap::{self, Parser},
        result,
        table::{Table, Title},
        Result, Run,
    };
    use common::{async_trait::async_trait, itertools::Itertools};
    use graph::{PlanOptions, PlanOrdering};
    use node_patch::diff_display;
    use stencila_schema::{
        EnumValidator, IntegerValidator, NumberValidator, StringValidator, ValidatorTypes,
    };

    use crate::utils::json;

    use super::*;

    /// Manage documents
    #[derive(Parser)]
    pub struct Command {
        #[clap(subcommand)]
        pub action: Action,
    }

    #[derive(Parser)]
    pub enum Action {
        List(List),
        Open(Open),
        Close(Close),
        Show(Show),

        #[cfg(feature = "kernels-cli")]
        Execute(kernel_commands::Execute),
        #[cfg(feature = "kernels-cli")]
        Kernels(kernel_commands::Kernels),
        #[cfg(feature = "kernels-cli")]
        Tasks(kernel_commands::Tasks),
        #[cfg(feature = "kernels-cli")]
        Queues(kernel_commands::Queues),
        #[cfg(feature = "kernels-cli")]
        Cancel(kernel_commands::Cancel),
        #[cfg(feature = "kernels-cli")]
        Symbols(kernel_commands::Symbols),
        #[cfg(feature = "kernels-cli")]
        Restart(kernel_commands::Restart),

        Graph(Graph),
        #[clap(alias = "pars")]
        Params(Params),
        Run(Run_),
        Plan(Plan),
        Query(Query),
        Diff(Diff),
        Merge(Merge),
        Detect(Detect),
    }

    #[async_trait]
    impl Run for Command {
        async fn run(&self) -> Result {
            let Self { action } = self;
            match action {
                Action::List(action) => action.run().await,
                Action::Open(action) => action.run().await,
                Action::Close(action) => action.run().await,
                Action::Show(action) => action.run().await,

                #[cfg(feature = "kernels-cli")]
                Action::Execute(action) => action.run().await,
                #[cfg(feature = "kernels-cli")]
                Action::Kernels(action) => action.run().await,
                #[cfg(feature = "kernels-cli")]
                Action::Tasks(action) => action.run().await,
                #[cfg(feature = "kernels-cli")]
                Action::Queues(action) => action.run().await,
                #[cfg(feature = "kernels-cli")]
                Action::Cancel(action) => action.run().await,
                #[cfg(feature = "kernels-cli")]
                Action::Symbols(action) => action.run().await,
                #[cfg(feature = "kernels-cli")]
                Action::Restart(action) => action.run().await,

                Action::Graph(action) => action.run().await,
                Action::Params(action) => action.run().await,
                Action::Run(action) => action.run().await,
                Action::Plan(action) => action.run().await,
                Action::Query(action) => action.run().await,
                Action::Diff(action) => action.run().await,
                Action::Merge(action) => action.run().await,
                Action::Detect(action) => action.run().await,
            }
        }
    }

    // The arguments used to specify the document file path and format
    // Reused (with flatten) below
    #[derive(Parser)]
    struct File {
        /// The path of the document file
        path: String,

        /// The format of the document file
        #[clap(short, long)]
        format: Option<String>,
    }
    impl File {
        async fn open(&self) -> eyre::Result<Document> {
            DOCUMENTS.open(&self.path, self.format.clone()).await
        }

        async fn get(&self) -> eyre::Result<Arc<Mutex<Document>>> {
            let document = self.open().await?;
            DOCUMENTS.get(&document.id).await
        }
    }

    /// List open documents
    #[derive(Parser)]
    pub struct List {}
    #[async_trait]
    impl Run for List {
        async fn run(&self) -> Result {
            let list = DOCUMENTS.list().await?;
            result::value(list)
        }
    }

    /// Open a document
    #[derive(Parser)]
    pub struct Open {
        #[clap(flatten)]
        file: File,
    }
    #[async_trait]
    impl Run for Open {
        async fn run(&self) -> Result {
            self.file.open().await?;
            result::nothing()
        }
    }

    /// Close a document
    #[derive(Parser)]
    pub struct Close {
        /// The path of the document file
        pub path: String,
    }
    #[async_trait]
    impl Run for Close {
        async fn run(&self) -> Result {
            DOCUMENTS.close(&self.path).await?;
            result::nothing()
        }
    }

    /// Show a document
    #[derive(Parser)]
    pub struct Show {
        #[clap(flatten)]
        file: File,

        /// A pointer to the part of the document to show e.g. `variables`, `format.name`
        ///
        /// Some, usually large, document properties are only shown when specified with a
        /// pointer (e.g. `content` and `root`).
        pub pointer: Option<String>,
    }
    #[async_trait]
    impl Run for Show {
        async fn run(&self) -> Result {
            let document = self.file.open().await?;
            if let Some(pointer) = &self.pointer {
                if pointer == "content" {
                    result::content(&document.format.extension, &document.content)
                } else if pointer == "root" {
                    let root = &*document.root.read().await;
                    result::value(root)
                } else {
                    let data = serde_json::to_value(document)?;
                    if let Some(part) = data.pointer(&json::pointer(pointer)) {
                        Ok(result::value(part)?)
                    } else {
                        bail!("Invalid pointer for document: {}", pointer)
                    }
                }
            } else {
                result::value(document)
            }
        }
    }

    // Subcommands that only work if `kernels-cli` feature is enabled
    #[cfg(feature = "kernels-cli")]
    mod kernel_commands {
        use super::*;

        #[derive(Parser)]
        #[clap(alias = "exec")]
        pub struct Execute {
            #[clap(flatten)]
            file: File,

            #[clap(flatten)]
            execute: kernels::commands::Execute,
        }

        #[async_trait]
        impl Run for Execute {
            async fn run(&self) -> Result {
                let document = self.file.get().await?;
                let document = document.lock().await;
                let _kernels = document.kernels.clone();
                //self.execute.run(&mut kernels).await
                result::nothing()
            }
        }

        #[derive(Parser)]
        pub struct Kernels {
            #[clap(flatten)]
            file: File,

            #[clap(flatten)]
            kernels: kernels::commands::Running,
        }

        #[async_trait]
        impl Run for Kernels {
            async fn run(&self) -> Result {
                let document = self.file.get().await?;
                let document = document.lock().await;
                let kernels = document.kernels.read().await;
                self.kernels.run(&*kernels).await
            }
        }

        #[derive(Parser)]
        pub struct Tasks {
            #[clap(flatten)]
            file: File,

            #[clap(flatten)]
            tasks: kernels::commands::Tasks,
        }

        #[async_trait]
        impl Run for Tasks {
            async fn run(&self) -> Result {
                let document = self.file.get().await?;
                let document = document.lock().await;
                let kernels = document.kernels.read().await;
                self.tasks.run(&*kernels).await
            }
        }

        #[derive(Parser)]
        pub struct Queues {
            #[clap(flatten)]
            file: File,

            #[clap(flatten)]
            queues: kernels::commands::Queues,
        }

        #[async_trait]
        impl Run for Queues {
            async fn run(&self) -> Result {
                let document = self.file.get().await?;
                let document = document.lock().await;
                let kernels = document.kernels.read().await;
                self.queues.run(&*kernels).await
            }
        }

        #[derive(Parser)]
        pub struct Cancel {
            #[clap(flatten)]
            file: File,

            #[clap(flatten)]
            cancel: kernels::commands::Cancel,
        }

        #[async_trait]
        impl Run for Cancel {
            async fn run(&self) -> Result {
                let document = self.file.get().await?;
                let document = document.lock().await;
                let _kernels = document.kernels.clone();
                //self.cancel.run(&mut *kernels).await
                result::nothing()
            }
        }

        #[derive(Parser)]
        pub struct Symbols {
            #[clap(flatten)]
            file: File,

            #[clap(flatten)]
            symbols: kernels::commands::Symbols,
        }

        #[async_trait]
        impl Run for Symbols {
            async fn run(&self) -> Result {
                let document = self.file.get().await?;
                let document = document.lock().await;
                let kernels = document.kernels.read().await;
                self.symbols.run(&*kernels).await
            }
        }

        #[derive(Parser)]
        pub struct Restart {
            #[clap(flatten)]
            file: File,

            #[clap(flatten)]
            restart: kernels::commands::Restart,
        }

        #[async_trait]
        impl Run for Restart {
            async fn run(&self) -> Result {
                let document = self.file.get().await?;
                let document = document.lock().await;
                let kernels = document.kernels.read().await;
                self.restart.run(&*kernels).await
            }
        }
    }

    /// Output the dependency graph for a document
    ///
    /// Tip: When using the DOT format (the default), if you have GraphViz and ImageMagick
    /// installed you can view the graph by piping the output to them. For example, to
    /// view a graph of the current project:
    ///
    /// ```sh
    /// $ stencila documents graph | dot -Tpng | display
    /// ```
    ///
    #[derive(Parser)]
    #[clap(verbatim_doc_comment)]
    pub struct Graph {
        #[clap(flatten)]
        file: File,

        /// The format to output the graph as
        #[clap(long, short, default_value = "dot", possible_values = &graph::FORMATS)]
        to: String,
    }

    #[async_trait]
    impl Run for Graph {
        async fn run(&self) -> Result {
            let document = self.file.get().await?;
            let document = document.lock().await;
            let content = document.graph.read().await.to_format(&self.to)?;
            result::content(&self.to, &content)
        }
    }

    /// Show the parameters of a document
    #[derive(Parser)]
    #[clap(verbatim_doc_comment)]
    pub struct Params {
        #[clap(flatten)]
        file: File,
    }

    /// A row in the table of parameters
    #[derive(Serialize, Table)]
    #[serde(crate = "common::serde")]
    #[table(crate = "cli_utils::cli_table")]
    struct Param {
        #[table(title = "Name")]
        name: String,

        #[table(title = "Id")]
        id: String,

        #[table(skip)]
        address: Address,

        #[table(title = "Validation", display_fn = "option_validator")]
        validator: Option<ValidatorTypes>,

        #[table(title = "Default", display_fn = "option_node")]
        default: Option<Node>,
    }

    fn option_validator(validator: &Option<ValidatorTypes>) -> String {
        let validator = match validator {
            Some(validator) => validator,
            None => return String::new(),
        };
        match validator {
            ValidatorTypes::BooleanValidator(..) => "Boolean".to_string(),
            ValidatorTypes::NumberValidator(NumberValidator {
                minimum,
                maximum,
                multiple_of,
                ..
            }) => format!(
                "Number {} {} {}",
                minimum
                    .map(|min| format!("min:{}", min))
                    .unwrap_or_default(),
                maximum
                    .map(|max| format!("max:{}", max))
                    .unwrap_or_default(),
                multiple_of
                    .as_ref()
                    .map(|mult| format!("multiple-of:{}", mult))
                    .unwrap_or_default()
            )
            .trim()
            .to_string(),
            ValidatorTypes::IntegerValidator(IntegerValidator {
                minimum,
                maximum,
                multiple_of,
                ..
            }) => format!(
                "Integer {} {} {}",
                minimum
                    .map(|min| format!("min:{}", min))
                    .unwrap_or_default(),
                maximum
                    .map(|max| format!("max:{}", max))
                    .unwrap_or_default(),
                multiple_of
                    .as_ref()
                    .map(|mult| format!("multiple-of:{}", mult))
                    .unwrap_or_default()
            )
            .trim()
            .to_string(),
            ValidatorTypes::StringValidator(StringValidator {
                min_length,
                max_length,
                pattern,
                ..
            }) => format!(
                "String {} {} {}",
                min_length
                    .map(|min| format!("min-length:{}", min))
                    .unwrap_or_default(),
                max_length
                    .map(|max| format!("max-length:{}", max))
                    .unwrap_or_default(),
                pattern
                    .as_ref()
                    .map(|pattern| format!("pattern:{}", pattern))
                    .unwrap_or_default()
            )
            .trim()
            .to_string(),
            ValidatorTypes::EnumValidator(EnumValidator { values, .. }) => format!(
                "One of {}",
                values
                    .iter()
                    .map(|value| serde_json::to_string(value).unwrap_or_default())
                    .join(", ")
            )
            .trim()
            .to_string(),
            _ => "*other*".to_string(),
        }
    }

    fn option_node(validator: &Option<Node>) -> String {
        let node = match validator {
            Some(node) => node,
            None => return String::new(),
        };
        serde_json::to_string(node).unwrap_or_default()
    }

    #[async_trait]
    impl Run for Params {
        async fn run(&self) -> Result {
            let document = self.file.get().await?;
            let mut document = document.lock().await;
            let params = document.params().await?;
            let params = params
                .into_iter()
                .map(|(name, (id, address, param))| Param {
                    name,
                    id,
                    address,
                    validator: param.validator.map(|boxed| *boxed),
                    default: param.default.map(|boxed| *boxed),
                })
                .collect_vec();
            result::table(params, Param::title())
        }
    }

    /// Run a document
    #[derive(Parser)]
    pub struct Run_ {
        /// The path of the document to execute
        pub input: PathBuf,

        /// Parameter `name=value` pairs
        args: Vec<String>,

        /// The path to save the executed document
        #[clap(short, long, alias = "out")]
        output: Option<PathBuf>,

        /// The format of the input (defaults to being inferred from the file extension or content type)
        #[clap(short, long)]
        from: Option<String>,

        /// The format of the output (defaults to being inferred from the file extension)
        #[clap(short, long)]
        to: Option<String>,

        /// The theme to apply to the output (only for HTML and PDF)
        #[clap(short = 'e', long)]
        theme: Option<String>,

        /// The id of the node to start execution from
        #[clap(short, long)]
        start: Option<String>,

        /// Ordering for the execution plan
        #[clap(long, parse(try_from_str = PlanOrdering::from_str), ignore_case = true)]
        ordering: Option<PlanOrdering>,

        /// Maximum concurrency for the execution plan
        ///
        /// A maximum concurrency of 2 means that no more than two tasks will
        /// run at the same time (ie. in the same stage).
        /// Defaults to the number of CPUs on the machine.
        #[clap(short, long)]
        concurrency: Option<usize>,
    }

    #[async_trait]
    impl Run for Run_ {
        async fn run(&self) -> Result {
            // Open document
            let mut document = Document::open(&self.input, self.from.clone()).await?;

            // Call with args, or just execute
            if !self.args.is_empty() {
                let args = params(&self.args);
                document.call(args).await?;
            } else {
                document
                    .execute(
                        false,
                        self.start.clone(),
                        self.ordering.clone(),
                        self.concurrency,
                    )
                    .await?;
            }

            tracing::info!("Finished running document");

            // Display or write output
            if let Some(output) = &self.output {
                let out = output.display().to_string();
                if out == "-" {
                    let format = self.to.clone().unwrap_or_else(|| "json".to_string());
                    let content = document.dump(Some(format.clone()), None).await?;
                    return result::content(&format, &content);
                } else {
                    document
                        .write_as(output, self.to.clone(), self.theme.clone())
                        .await?;
                }
            }

            result::nothing()
        }
    }

    /// Generate an execution plan for a document
    #[derive(Parser)]
    pub struct Plan {
        /// The path of the document to execute
        pub input: PathBuf,

        /// The format of the input (defaults to being inferred from the file extension or content type)
        #[clap(short, long)]
        from: Option<String>,

        /// The id of the node to start execution from
        #[clap(short, long)]
        start: Option<String>,

        /// Ordering for the execution plan
        #[clap(short, long, parse(try_from_str = PlanOrdering::from_str), ignore_case = true)]
        ordering: Option<PlanOrdering>,

        /// Maximum concurrency for the execution plan
        ///
        /// A maximum concurrency of 2 means that no more than two tasks will
        /// run at the same time (ie. in the same stage).
        /// Defaults to the number of CPUs on the machine.
        #[clap(short, long)]
        concurrency: Option<usize>,
    }

    #[async_trait]
    impl Run for Plan {
        async fn run(&self) -> Result {
            // Open document
            let document = Document::open(&self.input, self.from.clone()).await?;

            let start = self
                .start
                .as_ref()
                .map(|node_id| resources::code(&document.path, node_id, "", None));

            let options = PlanOptions {
                ordering: self
                    .ordering
                    .clone()
                    .unwrap_or_else(PlanOptions::default_ordering),
                max_concurrency: self
                    .concurrency
                    .unwrap_or_else(PlanOptions::default_max_concurrency),
            };

            let plan = {
                let graph = document.graph.write().await;
                graph.plan(start, None, Some(options)).await?
            };

            result::new("md", &plan.to_markdown(), &plan)
        }
    }

    /// Query a document
    #[derive(Parser)]
    pub struct Query {
        /// The path of the document file
        file: String,

        /// The query to run on the document
        query: String,

        /// The format of the file
        #[clap(short, long)]
        format: Option<String>,

        /// The language of the query
        #[clap(
            short,
            long,
            default_value = "jmespath",
            possible_values = &node_query::LANGS
        )]
        lang: String,
    }

    #[async_trait]
    impl Run for Query {
        async fn run(&self) -> Result {
            let Self {
                file,
                format,
                query,
                lang,
            } = self;
            let document = DOCUMENTS.open(file, format.clone()).await?;
            let node = &*document.root.read().await;
            let result = node_query::query(node, query, lang)?;
            result::value(result)
        }
    }

    /// Display the structural differences between two documents
    #[derive(Parser)]
    pub struct Diff {
        /// The path of the first document
        first: PathBuf,

        /// The path of the second document
        second: PathBuf,

        /// The format to display the difference in
        ///
        /// Defaults to a "unified diff" of the JSON representation
        /// of the documents. Unified diffs of other formats are available
        /// e.g. "md", "yaml". Use "raw" for the raw patch as a list of
        /// operations.
        #[clap(short, long, default_value = "json")]
        format: String,
    }

    #[async_trait]
    impl Run for Diff {
        async fn run(&self) -> Result {
            let Self {
                first,
                second,
                format,
            } = self;
            let first = Document::open(first, None).await?;
            let second = Document::open(second, None).await?;

            let first = &*first.root.read().await;
            let second = &*second.root.read().await;

            if format == "raw" {
                let patch = diff(first, second);
                result::value(patch)
            } else {
                let diff = diff_display(first, second, format).await?;
                result::content("patch", &diff)
            }
        }
    }

    /// Merge changes from two or more derived versions of a document
    ///
    /// This command can be used as a Git custom "merge driver".
    /// First, register Stencila as a merge driver,
    ///
    /// ```sh
    /// $ git config merge.stencila.driver "stencila merge --git %O %A %B"
    /// ```
    ///
    /// (The placeholders `%A` etc are used by `git` to pass arguments such
    /// as file paths and options to `stencila`.)
    ///
    /// Then, in your `.gitattributes` file assign the driver to specific
    /// types of files e.g.,
    ///
    /// ```text
    /// *.{md|docx} merge=stencila
    /// ```
    ///
    /// This can be done per project, or globally.
    #[derive(Parser)]
    #[clap(verbatim_doc_comment)]
    // See https://git-scm.com/docs/gitattributes#_defining_a_custom_merge_driver and
    // https://www.julianburr.de/til/custom-git-merge-drivers/ for more examples of defining a
    // custom driver. In particular the meaning of the placeholders %O, %A etc
    pub struct Merge {
        /// The path of the original version
        original: PathBuf,

        /// The paths of the derived versions
        #[clap(required = true, multiple_occurrences = true)]
        derived: Vec<PathBuf>,

        /// A flag to indicate that the command is being used as a Git merge driver
        ///
        /// When the `merge` command is used as a Git merge driver the second path
        /// supplied is the file that is written to.
        #[clap(short, long)]
        git: bool,
    }

    #[async_trait]
    impl Run for Merge {
        async fn run(&self) -> Result {
            let mut original = Document::open(&self.original, None).await?;

            let mut docs: Vec<Document> = Vec::new();
            for path in &self.derived {
                docs.push(Document::open(path, None).await?)
            }

            original.merge(&docs).await?;

            if self.git {
                original.write_as(&self.derived[0], None, None).await?;
            } else {
                original.write(None, None).await?;
            }

            result::nothing()
        }
    }

    /// Detect entities within a document
    #[derive(Parser)]
    pub struct Detect {
        /// The path of the document file
        pub file: String,
    }

    #[async_trait]
    impl Run for Detect {
        async fn run(&self) -> Result {
            let mut document = DOCUMENTS.open(&self.file, None).await?;
            document.read(true).await?;
            let nodes = document.detect().await?;
            result::value(nodes)
        }
    }
}

#[cfg(test)]
mod tests {
    use test_utils::fixtures;

    use super::*;

    #[tokio::test]
    async fn new() {
        let doc = Document::new(None, None);
        assert!(doc.path.starts_with(env::temp_dir()));
        assert!(doc.temporary);
        assert!(matches!(doc.status, DocumentStatus::Synced));
        assert_eq!(doc.format.extension, "txt");
        assert_eq!(doc.content, "");
        assert_eq!(doc.subscriptions, HashMap::new());

        let doc = Document::new(None, Some("md".to_string()));
        assert!(doc.path.starts_with(env::temp_dir()));
        assert!(doc.temporary);
        assert!(matches!(doc.status, DocumentStatus::Synced));
        assert_eq!(doc.format.extension, "md");
        assert_eq!(doc.content, "");
        assert_eq!(doc.subscriptions, HashMap::new());
    }

    #[tokio::test]
    async fn open() -> Result<()> {
        for file in &["elife-small.json", "era-plotly.json"] {
            let doc = Document::open(fixtures().join("articles").join(file), None).await?;
            assert!(!doc.temporary);
            assert!(matches!(doc.status, DocumentStatus::Synced));
            assert_eq!(doc.format.extension, "json");
            assert!(!doc.content.is_empty());
            assert_eq!(doc.subscriptions, HashMap::new());
        }

        Ok(())
    }
}
