import { useEffect, useState } from 'react'
import i18n from 'i18next'
import { initReactI18next, I18nextProvider } from 'react-i18next'

// 从 JSON 文件导入翻译
import zhCN from '../locales/zh-CN.json'

// 初始化 i18n（仅中文）
i18n
  .use(initReactI18next)
  .init({
    lng: 'zh-CN',
    fallbackLng: 'zh-CN',
    supportedLngs: ['zh-CN'],

    resources: {
      'zh-CN': { translation: zhCN }
    },

    interpolation: {
      escapeValue: false
    },

    react: {
      useSuspense: false
    }
  })

// I18nProvider 组件
function I18nProvider({ children }) {
  return <I18nextProvider i18n={i18n}>{children}</I18nextProvider>
}

export { I18nProvider }
export default i18n
