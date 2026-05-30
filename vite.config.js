import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import path from 'path'
import pkg from './package.json'

export default defineConfig({
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  plugins: [
    react(),
    tailwindcss(),
  ],
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  clearScreen: false,
  server: {
    port: 7410,
    strictPort: true,
    // 优化开发服务器性能
    hmr: {
      overlay: false, // 禁用错误覆盖层，减少渲染开销
    },
    // 预热常用文件，加快首次加载
    warmup: {
      clientFiles: ['./src/main.tsx', './src/App.tsx'],
    },
  },
  // 优化依赖预构建
  optimizeDeps: {
    include: [
      'react',
      'react-dom',
      'lucide-react',
      '@tauri-apps/api',
    ],
    // 强制预构建，避免首次启动慢
    force: false,
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    target: process.env.TAURI_PLATFORM === 'windows' ? 'chrome105' : 'safari13',
    minify: !process.env.TAURI_DEBUG ? 'oxc' : false,
    sourcemap: !!process.env.TAURI_DEBUG,
    // 代码分割优化
    rollupOptions: {
      output: {
        // Oxc 原生压缩器：保留移除 console / debugger 的行为
        minify: process.env.TAURI_DEBUG
          ? false
          : { compress: { dropConsole: true, dropDebugger: true }, mangle: true },
        manualChunks(id) {
          if (!id.includes('node_modules')) return
          if (/[\\/]node_modules[\\/](react|react-dom|scheduler)[\\/]/.test(id)) return 'vendor'
          if (id.includes('lucide-react')) return 'icons'
          if (id.includes('@tauri-apps')) return 'tauri'
        },
      },
    },
    // 大项目可关闭压缩报告加速构建
    reportCompressedSize: false,
  },
})
