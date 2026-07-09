'use client'

import { ReactNode, useEffect } from 'react'
import { useThemeStore } from '@/lib/store'

export function Providers({ children }: { children: ReactNode }) {
  const { theme, toggleTheme } = useThemeStore()

  useEffect(() => {
    const root = window.document.documentElement
    root.classList.remove('light', 'dark')
    root.classList.add(theme)
  }, [theme])

  return <>{children}</>
}
