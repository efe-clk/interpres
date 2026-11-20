import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import '@/i18n'
import App from './App'
import { CaptionPage, PickerPage } from '@/screens/OverlayPages'
import { QuickTranslate } from '@/screens/QuickTranslate'

const pages: Record<string, React.ReactNode> = {
  '#quick': <QuickTranslate />,
  '#picker': <PickerPage />,
  '#caption': <CaptionPage />,
}
const root = pages[window.location.hash] ?? <App />

createRoot(document.getElementById('root')!).render(<StrictMode>{root}</StrictMode>)
