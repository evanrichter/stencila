/**
 * Used to determine various application behaviors e.g. not reading binary formats into memory unnecessarily
 */
export interface Format {
  /**
   * Whether or not this is a known format (ie.e. not automatically created)
   */
  known: boolean
  /**
   * The lowercase name of the format e.g. `md`, `docx`, `dockerfile`
   */
  name: string
  /**
   * Whether or not the format should be considered binary e.g. not to be displayed in a text / code editor
   */
  binary: boolean
  /**
   * Whether HTML previews are normally supported for documents of this format. See also `Document.previewable` which indicates whether a HTML preview is supported for a particular document.
   */
  preview: boolean
  /**
   * Any additional extensions (other than it's name) that this format should match against.
   */
  extensions: string[]
}

/**
 * An in-memory representation of a document
 */
export interface Document {
  /**
   * The document identifier
   */
  id: string
  /**
   * The absolute path of the document's file.
   */
  path: string
  /**
   * The project directory for this document.
   *
   * Used to restrict file links (e.g. image paths) to within the project for both security and reproducibility reasons. For documents opened from within a project, this will be project directory. For "orphan" documents (opened by themselves) this will be the parent directory of the document. When the document is compiled, an error will be returned if a file link is outside of the root.
   */
  project: string
  /**
   * Whether or not the document's file is in the temporary directory.
   */
  temporary: boolean
  /**
   * The synchronization status of the document. This is orthogonal to `temporary` because a document's `content` can be synced or un-synced with the file system regardless of whether or not its `path` is temporary..
   */
  status: 'synced' | 'unwritten' | 'unread' | 'deleted'
  /**
   * The name of the document
   *
   * Usually the filename from the `path` but "Untitled" for temporary documents.
   */
  name: string
  /**
   * The format of the document.
   *
   * On initialization, this is inferred, if possible, from the file name extension of the document's `path`. However, it may change whilst the document is open in memory (e.g. if the `load` function sets a different format).
   */
  format: Format
  /**
   * Whether a HTML preview of the document is supported
   *
   * This is determined by the type of the `root` node of the document. Will be `true` if the `root` is a type for which HTML previews are implemented e.g. `Article`, `ImageObject` and `false` if the `root` is `None`, or of some other type e.g. `Entity`.
   *
   * This flag is intended for dynamically determining whether to open a preview panel for a document by default. Regardless of its value, a user should be able to open a preview panel, in HTML or some other format, for any document.
   */
  previewable: boolean
  /**
   * The set of relations between nodes in this document and other resources.
   *
   * Relations may be external (e.g. this document links to another file) or internal (e.g. the second code chunk uses a variable defined in the first code chunk).
   */
  relations?: Record<string, [Relation, Resource]>
  /**
   * Keeps track of the number of subscribers to each of the document's topic channels. Events will only be published on channels that have at least one subscriber.
   *
   * Valid subscription topics are the names of the `DocumentEvent` types:
   *
   * - `removed`: published when document file is deleted - `renamed`: published when document file is renamed - `modified`: published when document file is modified - `encoded:<format>` published when a document's content is changed internally or externally and  conversions have been completed e.g. `encoded:html`
   */
  subscriptions: {
    [k: string]: number
  }
}

export interface DocumentEvent {
  /**
   * The type of event
   */
  type: 'deleted' | 'renamed' | 'modified' | 'encoded'
  /**
   * The document associated with the event
   */
  document: Document
  /**
   * The content associated with the event, only provided for, `modified` and `encoded` events.
   */
  content?: string
  /**
   * The format of the document, only provided for `modified` (the format of the document) and `encoded` events (the format of the encoding).
   */
  format?: Format
}

/**
 * An implementation, and extension, of schema.org [`Project`](https://schema.org/Project). Uses schema.org properties where possible but adds extension properties where needed (e.g. `theme`).
 */
export interface Project {
  /**
   * The name of the project
   */
  name?: string
  /**
   * A description of the project
   */
  description?: string
  /**
   * The path (within the project) of the project's image
   *
   * If not specified, will default to the most recently modified image in the project (if any).
   */
  image?: string
  /**
   * The path (within the project) of the project's main file
   *
   * If not specified, will default to the first file matching the the regular expression in the configuration settings.
   */
  main?: string
  /**
   * The default theme to use when viewing documents in this project
   *
   * If not specified, will default to the default theme in the configuration settings.
   */
  theme?: string
  /**
   * A list of project sources and their destination within the project
   */
  sources?: SourceDestination[]
  /**
   * A list of file conversions
   */
  conversions?: {
    /**
     * The path of the input document
     */
    input?: string
    /**
     * The path of the output document
     */
    output?: string
    /**
     * The format of the input (defaults to being inferred from the file extension of the input)
     */
    from?: string
    /**
     * The format of the output (defaults to being inferred from the file extension of the output)
     */
    to?: string
    /**
     * Whether or not the conversion is active
     */
    active?: boolean
  }[]
  /**
   * Glob patterns for paths to be excluded from file watching
   *
   * As a performance optimization, paths that match these patterns are excluded from file watching updates. If not specified, will default to the patterns in the configuration settings.
   */
  watchExcludePatterns?: string[]
  /**
   * The filesystem path of the project folder
   */
  path: string
  /**
   * The resolved path of the project's image file
   */
  imagePath?: string
  /**
   * The resolved path of the project's main file
   */
  mainPath?: string
  /**
   * The files in the project folder
   */
  files: Record<string, File>
  /**
   * The project's dependency graph
   */
  graph: Graph
}

export interface ProjectEvent {
  /**
   * The project associated with the event
   */
  project: Project
  /**
   * The type of event
   */
  type: 'updated'
}

/**
 * A file or directory within a `Project`
 */
export interface File {
  /**
   * The absolute path of the file or directory
   */
  path: string
  /**
   * The name of the file or directory
   */
  name: string
  /**
   * Time that the file was last modified (Unix Epoch timestamp)
   */
  modified?: number
  /**
   * Size of the file in bytes
   */
  size?: number
  /**
   * Format of the file
   *
   * Usually this is the lower cased filename extension (if any) but may also be normalized. May be more convenient, and usually more available, than the `media_type` property.
   */
  format: Format
  /**
   * The parent `File`, if any
   */
  parent?: string
  /**
   * If a directory, a list of the canonical paths of the files within it. Otherwise, `None`.
   *
   * A `BTreeSet` rather than a `Vec` so that paths are ordered without having to be resorted after insertions. Another option is `BinaryHeap` but `BinaryHeap::retain` is  only on nightly and so is awkward to use.
   */
  children?: string[]
}

/**
 * These events published under the `projects:<project-path>:files` topic.
 */
export interface FileEvent {
  /**
   * The path of the project (absolute)
   */
  project: string
  /**
   * The path of the file (absolute)
   *
   * For `renamed` events this is the _old_ path.
   */
  path: string
  /**
   * The type of event e.g. `Refreshed`, `Modified`, `Created`
   *
   * A `refreshed` event is emitted when the entire set of files is updated.
   */
  type: 'refreshed' | 'created' | 'removed' | 'renamed' | 'modified'
  /**
   * The updated file
   *
   * Will be `None` for for `refreshed` and `removed` events, or if for some reason it was not possible to fetch metadata about the file.
   */
  file?: File
  /**
   * The updated set of files in the project
   *
   * Represents the new state of the file tree after the event including updated `parent` and `children` properties of files affects by the event.
   */
  files: Record<string, File>
}

/**
 * A resource in a dependency graph (the nodes of the graph)
 */
export type Resource =
  | {
      type: 'Symbol'
      /**
       * The path of the file that the symbol is defined in
       */
      path: string
      /**
       * The name/identifier of the symbol
       */
      name: string
      /**
       * The type of the object that the symbol refers to (e.g `Number`, `Function`)
       *
       * Should be used as a hint only, and as such is excluded from equality and hash functions.
       */
      kind: string
    }
  | {
      type: 'Node'
      /**
       * The path of the file that the node is defined in
       */
      path: string
      /**
       * The id of the node with the document
       */
      id: string
      /**
       * The type of node e.g. `Parameter`, `CodeChunk`
       */
      kind: string
    }
  | {
      type: 'File'
      /**
       * The path of the file
       */
      path: string
    }
  | {
      type: 'Source'
      /**
       * The name of the project source
       */
      name: string
    }
  | {
      type: 'Module'
      /**
       * The programming language of the module
       */
      language: string
      /**
       * The name of the module
       */
      name: string
    }
  | {
      type: 'Url'
      /**
       * The URL of the external resource
       */
      url: string
    }

/**
 * The relation between two resources in a dependency graph (the edges of the graph)
 *
 * Some relations carry additional information such whether the relation is active (`Import` and `Convert`) or the range that they occur in code (`Assign`, `Use`, `Read`) etc
 */
export type Relation =
  | {
      type: 'Assign'
      /**
       * The range within code that the assignment is done
       */
      range: [number, number, number, number]
    }
  | {
      type: 'Convert'
      /**
       * Whether or not the conversion is automatically updated
       */
      auto: boolean
    }
  | {
      type: 'Embed'
      [k: string]: unknown
    }
  | {
      type: 'Import'
      /**
       * Whether or not the import is automatically updated
       */
      auto: boolean
    }
  | {
      type: 'Include'
      [k: string]: unknown
    }
  | {
      type: 'Link'
      [k: string]: unknown
    }
  | {
      type: 'Read'
      /**
       * The range within code that the read is declared
       */
      range: [number, number, number, number]
    }
  | {
      type: 'Use'
      /**
       * The range within code that the use is declared
       */
      range: [number, number, number, number]
    }
  | {
      type: 'Write'
      /**
       * The range within code that the write is declared
       */
      range: [number, number, number, number]
    }

/**
 * A subject-relation-object triple
 */
export type Triple = [Resource, Relation, Resource]

/**
 * A project dependency graph
 */
export interface Graph {
  /**
   * The resources in the graph
   */
  nodes: Resource[]
  /**
   * The relations between resources in the graph
   */
  edges: {
    from: 'integer'
    to: 'integer'
    relation: Resource
  }[]
}

export interface GraphEvent {
  /**
   * The path of the project (absolute)
   */
  project: string
  /**
   * The type of event
   */
  type: 'updated'
  /**
   * The graph at the time of the event
   */
  graph: Graph
}

/**
 * Each source by destination combination should be unique to a project. It is possible to have the same source being imported to multiple destinations within a project and for multiple sources to used the same destination (e.g. the root directory of the project).
 */
export interface SourceDestination {
  /**
   * The source from which files will be imported
   */
  source?:
    | {
        type: 'Null'
      }
    | {
        type: 'Elife'
        /**
         * Number of the article
         */
        article: number
      }
    | {
        type: 'GitHub'
        /**
         * Owner of the repository
         */
        owner: string
        /**
         * Name of the repository
         */
        name: string
        /**
         * Path within the repository
         */
        path?: string
      }
  /**
   * The destination path within the project
   */
  destination?: string
  /**
   * Whether or not the source is active
   *
   * If the source is active an import job will be created for it each time the project is updated.
   */
  active?: boolean
  /**
   * A list of file paths currently associated with the source, relative to the project root
   */
  files?: string[]
}

/**
 * As far as possible using existing properties defined in schema.org [`SoftwareApplication`](https://schema.org/SoftwareApplication) but extensions added where necessary.
 */
export interface Plugin {
  /**
   * The name of the plugin
   */
  name: string
  /**
   * The version of the plugin
   */
  softwareVersion: string
  /**
   * A description of the plugin
   */
  description: string
  /**
   * URL of the image to be used when displaying the plugin
   */
  image?: string
  /**
   * A list of URLS that the plugin can be installed from
   */
  installUrl: string[]
  /**
   * A list of plugin "features" Each feature is a `JSONSchema` object describing a method (including its parameters).
   */
  featureList: true[]
  /**
   * If the plugin is installed, the installation type
   */
  installation?: 'docker' | 'binary' | 'javascript' | 'python' | 'r' | 'link'
  /**
   * The last time that the plugin manifest was updated. Used to determine if a refresh is necessary.
   */
  refreshed?: string
  /**
   * The next version of the plugin, if any.
   *
   * If the plugin is installed and there is a newer version of the plugin then this property should be set at the time of refresh.
   */
  next?: Plugin
  /**
   * The current alias for this plugin, if any
   */
  alias?: string
}

export interface Config {
  /**
   * Configuration settings for project defaults
   */
  projects?: {
    /**
     * Patterns used to infer the main file of projects
     *
     * For projects that do not specify a main file, each file is tested against these case insensitive patterns in order. The first file (alphabetically) that matches is the project's main file.
     */
    mainPatterns?: string[]
    /**
     * Default project theme
     *
     * Will be applied to all projects that do not specify a theme
     */
    theme?: string
    /**
     * Default glob patterns for paths to be excluded from file watching
     *
     * Used for projects that do not specify their own watch exclude patterns. As a performance optimization, paths that match these patterns are excluded from file watching updates. The default list includes common directories that often have many files that are often updated.
     */
    watchExcludePatterns?: string[]
  }
  /**
   * Configuration settings for logging
   */
  logging?: {
    /**
     * Configuration settings for log entries printed to stderr when using the CLI
     */
    stderr?: {
      /**
       * The maximum log level to emit
       */
      level?: 'trace' | 'debug' | 'info' | 'warn' | 'error' | 'never'
      /**
       * The format for the logs entries
       */
      format?: 'simple' | 'detail' | 'json'
    }
    /**
     * Configuration settings for log entries shown to the user in the desktop
     */
    desktop?: {
      /**
       * The maximum log level to emit
       */
      level?: 'trace' | 'debug' | 'info' | 'warn' | 'error' | 'never'
    }
    /**
     * Configuration settings for logs entries written to file
     */
    file?: {
      /**
       * The path of the log file
       */
      path?: string
      /**
       * The maximum log level to emit
       */
      level?: 'trace' | 'debug' | 'info' | 'warn' | 'error' | 'never'
    }
  }
  /**
   * Configuration settings for telemetry
   */
  telemetry?: {
    /**
     * Telemetry settings for Stencila CLI
     */
    cli?: {
      /**
       * Whether to send error reports. Default is false.
       */
      error_reports?: boolean
    }
    /**
     * Telemetry settings for Stencila Desktop
     */
    desktop?: {
      /**
       * Whether to send error reports. Default is false.
       */
      error_reports?: boolean
    }
  }
  /**
   * Configuration settings for running as a server
   */
  serve?: {
    /**
     * The URL to serve on (defaults to `ws://127.0.0.1:9000`)
     */
    url?: string
    /**
     * Secret key to use for signing and verifying JSON Web Tokens
     */
    key?: string
    /**
     * Do not require a JSON Web Token to access the server
     */
    insecure?: boolean
  }
  /**
   * Configuration settings for plugin installation and management
   */
  plugins?: {
    /**
     * The order of preference of plugin installation method.
     */
    installations?: ('docker' | 'binary' | 'javascript' | 'python' | 'r' | 'link')[]
    /**
     * The local plugin aliases that extends and/or override those in the global aliases at <https://github.com/stencila/stencila/blob/master/plugins.json>
     */
    aliases?: {
      [k: string]: string
    }
  }
  /**
   * Configuration settings for installation and management of third party binaries
   */
  binaries?: {
    /**
     * Whether binaries should be automatically installed when they are required
     */
    auto?: boolean
  }
  /**
   * Configuration settings for document editors.
   */
  editors?: {
    /**
     * Default format for new documents
     */
    defaultFormat?: string
    /**
     * Show line numbers
     */
    lineNumbers?: boolean
    /**
     * Enable wrapping of lines
     */
    lineWrapping?: boolean
  }
  /**
   * Configuration settings used when upgrading the application (and optionally plugins) automatically, in the background. These settings are NOT used as defaults when using the CLI `upgrade` command directly.
   */
  upgrade?: {
    /**
     * Plugins should also be upgraded to latest version
     */
    plugins?: boolean
    /**
     * Prompt to confirm an upgrade
     */
    confirm?: boolean
    /**
     * Show information on the upgrade process
     */
    verbose?: boolean
    /**
     * The interval between automatic upgrade checks (defaults to "1 day"). Only used when for configuration. Set to "off" for no automatic checks.
     */
    auto?: string
  }
}

/**
 * An event associated with changes to the configuration
 */
export interface ConfigEvent {
  /**
   * The type of event
   */
  type: 'set' | 'reset'
  /**
   * The configuration at the time of the event
   */
  config: Config
}

/**
 * An enumeration of custom errors returned by this library
 *
 * Where possible functions should return one of these errors to provide greater context to the user, in particular regarding actions that can be taken to resolve the error.
 */
export type Error =
  | {
      type: 'UnknownFormat'
      format: string
      message: string
    }
  | {
      type: 'UndelegatableMethod'
      method: Method
      message: string
    }
  | {
      type: 'UndelegatableCall'
      method: Method
      params: {
        [k: string]: unknown
      }
      message: string
    }
  | {
      type: 'PluginNotInstalled'
      plugin: string
      message: string
    }
  | {
      type: 'Unspecified'
      message: string
    }
/**
 * An enumeration of all methods
 */
export type Method = 'import' | 'export' | 'decode' | 'encode' | 'coerce' | 'reshape' | 'compile' | 'build' | 'execute'

export const FORMATS: Record<string, Format> = {
  "3gp": {
    "known": true,
    "name": "3gp",
    "binary": true,
    "preview": true,
    "extensions": []
  },
  "dir": {
    "known": true,
    "name": "dir",
    "binary": true,
    "preview": false,
    "extensions": []
  },
  "dockerfile": {
    "known": true,
    "name": "dockerfile",
    "binary": false,
    "preview": false,
    "extensions": []
  },
  "docx": {
    "known": true,
    "name": "docx",
    "binary": true,
    "preview": true,
    "extensions": []
  },
  "flac": {
    "known": true,
    "name": "flac",
    "binary": true,
    "preview": true,
    "extensions": []
  },
  "gif": {
    "known": true,
    "name": "gif",
    "binary": true,
    "preview": true,
    "extensions": []
  },
  "html": {
    "known": true,
    "name": "html",
    "binary": false,
    "preview": true,
    "extensions": []
  },
  "ipynb": {
    "known": true,
    "name": "ipynb",
    "binary": false,
    "preview": true,
    "extensions": []
  },
  "jpg": {
    "known": true,
    "name": "jpg",
    "binary": true,
    "preview": true,
    "extensions": [
      "jpeg"
    ]
  },
  "js": {
    "known": true,
    "name": "js",
    "binary": false,
    "preview": false,
    "extensions": []
  },
  "json": {
    "known": true,
    "name": "json",
    "binary": false,
    "preview": true,
    "extensions": []
  },
  "json5": {
    "known": true,
    "name": "json5",
    "binary": false,
    "preview": true,
    "extensions": []
  },
  "latex": {
    "known": true,
    "name": "latex",
    "binary": false,
    "preview": true,
    "extensions": [
      "tex"
    ]
  },
  "makefile": {
    "known": true,
    "name": "makefile",
    "binary": false,
    "preview": false,
    "extensions": []
  },
  "md": {
    "known": true,
    "name": "md",
    "binary": false,
    "preview": true,
    "extensions": []
  },
  "mp3": {
    "known": true,
    "name": "mp3",
    "binary": true,
    "preview": true,
    "extensions": []
  },
  "mp4": {
    "known": true,
    "name": "mp4",
    "binary": true,
    "preview": true,
    "extensions": []
  },
  "odt": {
    "known": true,
    "name": "odt",
    "binary": true,
    "preview": true,
    "extensions": []
  },
  "ogg": {
    "known": true,
    "name": "ogg",
    "binary": true,
    "preview": true,
    "extensions": []
  },
  "ogv": {
    "known": true,
    "name": "ogv",
    "binary": true,
    "preview": true,
    "extensions": []
  },
  "png": {
    "known": true,
    "name": "png",
    "binary": true,
    "preview": true,
    "extensions": []
  },
  "py": {
    "known": true,
    "name": "py",
    "binary": false,
    "preview": false,
    "extensions": []
  },
  "r": {
    "known": true,
    "name": "r",
    "binary": false,
    "preview": false,
    "extensions": []
  },
  "rmd": {
    "known": true,
    "name": "rmd",
    "binary": false,
    "preview": true,
    "extensions": []
  },
  "rpng": {
    "known": true,
    "name": "rpng",
    "binary": true,
    "preview": true,
    "extensions": []
  },
  "sh": {
    "known": true,
    "name": "sh",
    "binary": false,
    "preview": false,
    "extensions": []
  },
  "toml": {
    "known": true,
    "name": "toml",
    "binary": false,
    "preview": true,
    "extensions": []
  },
  "ts": {
    "known": true,
    "name": "ts",
    "binary": false,
    "preview": false,
    "extensions": []
  },
  "txt": {
    "known": true,
    "name": "txt",
    "binary": false,
    "preview": false,
    "extensions": []
  },
  "webm": {
    "known": true,
    "name": "webm",
    "binary": true,
    "preview": true,
    "extensions": []
  },
  "xml": {
    "known": true,
    "name": "xml",
    "binary": false,
    "preview": true,
    "extensions": []
  },
  "yaml": {
    "known": true,
    "name": "yaml",
    "binary": false,
    "preview": true,
    "extensions": []
  }
}
