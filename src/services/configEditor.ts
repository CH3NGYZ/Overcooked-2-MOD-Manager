import * as monaco from 'monaco-editor/editor/editor.api.js'
import 'monaco-editor/languages/definitions/ini/register.js'
import 'monaco-editor/editor/contrib/find/browser/findController.js'
import 'monaco-editor/editor/contrib/clipboard/browser/clipboard.js'
import 'monaco-editor/editor/contrib/contextmenu/browser/contextmenu.js'
import 'monaco-editor/editor/contrib/bracketMatching/browser/bracketMatching.js'
import EditorWorker from 'monaco-editor/editor/editor.worker.js?worker'

self.MonacoEnvironment = { getWorker: () => new EditorWorker() }

export { monaco }
