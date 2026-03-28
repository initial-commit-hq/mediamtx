import { fileURLToPath } from 'node:url'
import { dirname, resolve } from 'node:path'

const __dirname = dirname(fileURLToPath(import.meta.url))

// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  ssr: false,

  // Output static files directly to internal/viewerserver/dist/ for Go binary embedding.
  // Using an absolute path avoids ambiguity in Nitro's relative-path resolution.
  nitro: {
    output: {
      publicDir: resolve(__dirname, '../internal/viewerserver/dist'),
    },
  },

  app: {
    baseURL: '/',
    head: {
      title: 'Camera Grid — MediaMTX',
      meta: [
        { charset: 'utf-8' },
        { name: 'viewport', content: 'width=device-width, initial-scale=1.0' },
      ],
    },
  },

  css: ['~/assets/css/main.css'],

  compatibilityDate: '2024-11-01',
})
