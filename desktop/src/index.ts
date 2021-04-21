import { app, BrowserWindow, ipcMain, protocol } from 'electron'
import { main } from './main'
import { requestHandler, scheme } from './main/app-protocol'
import { initStore } from './main/store/bootstrap'

let store: ReturnType<typeof initStore>

// declare const MAIN_WINDOW_WEBPACK_ENTRY: string
declare const MAIN_WINDOW_PRELOAD_WEBPACK_ENTRY: string

const isDevelopment = process.env.NODE_ENV === 'development'

// Handle creating/removing shortcuts on Windows when installing/uninstalling.
if (require('electron-squirrel-startup')) {
  app.quit()
}

const createWindow = (): void => {
  /* eng-disable PROTOCOL_HANDLER_JS_CHECK */
  protocol.registerBufferProtocol(scheme, requestHandler)

  // Create the browser window.
  const mainWindow = new BrowserWindow({
    height: 860,
    width: 1024,
    webPreferences: {
      // TODO: Fix sandboxing, currently prevents `preload` script access
      sandbox: false,
      nodeIntegration: false,
      contextIsolation: true, // protect against prototype pollution
      enableRemoteModule: false,
      preload: MAIN_WINDOW_PRELOAD_WEBPACK_ENTRY,
      additionalArguments: [`storePath:${app.getPath('userData')}`],
    },
  })

  store = initStore(mainWindow)

  if (isDevelopment) {
    mainWindow.loadURL('http://localhost:3333')
  } else {
    mainWindow.loadURL(`${scheme}://rse/`)
  }

  if (isDevelopment) {
    // Open the DevTools.
    mainWindow.webContents.openDevTools()
  }
}

protocol.registerSchemesAsPrivileged([
  {
    scheme: scheme,
    privileges: {
      standard: true,
      secure: true,
    },
  },
])

// This method will be called when Electron has finished
// initialization and is ready to create browser windows.
// Some APIs can only be used after this event occurs.
app.on('ready', createWindow)

// Quit when all windows are closed, except on macOS. There, it's common
// for applications and their menu bar to stay active until the user quits
// explicitly with Cmd + Q.
app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit()
  } else {
    store.clearMainBindings(ipcMain)
  }
})

app.on('activate', () => {
  // On OS X it's common to re-create a window in the app when the
  // dock icon is clicked and there are no other windows open.
  if (BrowserWindow.getAllWindows().length === 0) {
    createWindow()
  }
})

// In this file you can include the rest of your app's specific main process
// code. You can also put them in separate files and import them here.
main()
