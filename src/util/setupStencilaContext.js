import FunctionManager from '../function/FunctionManager'
import Host from '../host/Host'
import Engine from '../engine/Engine'
import ArticleEngineAdapter from '../article/ArticleEngineAdapter'
import SheetEngineAdapter from '../sheet/SheetEngineAdapter'

export default function setupStencilaContext(documentContainer, config = {}) {
  let libs = Object.assign({
    core: window.STENCILA_LIBCORE
  }, config.libs)
  let peers = config.peers || window.STENCILA_PEERS
  if (peers) peers = peers.split(',')

  let functionManager = new FunctionManager()
  Object.keys(libs).forEach((libName) => {
    functionManager.importLibrary(libName, libs[libName])
  })
  let host = new Host({
    functionManager,
    peers: peers,
    discover: Boolean(config.discover),
  })
  // setup the Engine
  let engine = new Engine(host)
  let docEntries = documentContainer.getDocumentEntries()
  docEntries.forEach((entry) => {
    let editorSession = documentContainer.getEditorSession(entry.id)
    if (entry.type === 'article') {
      let engineAdapter = new ArticleEngineAdapter(editorSession)
      engineAdapter.connect(engine)
    } else if (entry.type === 'sheet') {
      let engineAdapter = new SheetEngineAdapter(editorSession)
      engineAdapter.connect(engine)
    }
  })
  return { host, functionManager, engine }
}