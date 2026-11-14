import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'

// no native webview menu (Reload, Inspect…): our own menus handle right-click; text fields keep theirs for spellcheck
document.addEventListener('contextmenu', (e) => {
  if (!(e.target as Element).closest?.('input, textarea')) e.preventDefault()
})

export default mount(App, { target: document.getElementById('app')! })
