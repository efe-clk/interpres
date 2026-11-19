import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import '@/i18n'
import App from './App'
import { QuickTranslate } from '@/screens/QuickTranslate'

const root = window.location.hash === '#quick' ? <QuickTranslate /> : <App />

createRoot(document.getElementById('root')!).render(<StrictMode>{root}</StrictMode>)
