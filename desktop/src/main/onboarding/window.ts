import { BrowserWindow } from 'electron'
import { onboardingHandlers } from '.'
import { i18n } from '../../i18n'
import { configHandlers } from '../config'
import { launcherHandlers } from '../launcher'
import { registerBaseMenu } from '../menu'
import { createWindow } from '../window'
import { onUiLoaded } from '../window/windowUtils'

let onboardingWindow: BrowserWindow | null

const onboardingUrl = '/onboarding'

export const openOnboardingWindow = () => {
  if (onboardingWindow) {
    onboardingWindow.show()
    return onboardingWindow
  }

  onboardingWindow = createWindow(onboardingUrl, {
    width: 800,
    height: 600,
    maxWidth: 1000,
    minWidth: 600,
    maxHeight: 800,
    minHeight: 500,
    show: false,
    title: i18n.t('onboarding.title'),
    fullscreenable: false,
    center: true,
  })

  // The ID needs to be stored separately from the window object. Otherwise an error
  // is thrown because the time remove handlers are called the window object is already destroyed.
  const windowId = onboardingWindow.id

  launcherHandlers.register(windowId)
  configHandlers.register(windowId)
  onboardingHandlers.register(windowId)

  onboardingWindow.on('closed', () => {
    launcherHandlers.remove(windowId)
    configHandlers.remove(windowId)
    onboardingHandlers.remove(windowId)
    onboardingWindow = null
  })

  onUiLoaded(onboardingWindow.webContents)(() => {
    onboardingWindow?.show()
  })

  onboardingWindow.on('focus', () => {
    registerBaseMenu()
  })

  onboardingWindow?.loadURL(onboardingUrl)

  return onboardingWindow
}

export const closeOnboardingWindow = () => {
  onboardingWindow?.close()
}
